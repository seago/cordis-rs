# 代码审查报告：commit `f883492`（W1b 宿主驱动，M1 wasm 桥专项）

- **审查对象**：`f883492`（feat(wasm): W1b 宿主驱动）+ `532d703`（fix(wasm): W1b clippy）——`crates/cordis-wasm/src/lib.rs`（+293/-8）+ `Cargo.toml`（+cordis-async run / +tokio dev）；覆盖 W1a（`96af34c`，wit `remote` 接口 + bindgen + Host stub）为前置上下文。工作树与 HEAD 一致（`git status` 无未提交）。
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：对照 `docs/cordis-wasm-WASMREMOTE-PROTOCOL.md` §2-6 与决策 W-D1..D4（`docs/cordis-wasm-WASMREMOTE-PLAN.md` §1）；线程/沙箱/借用核查。
- **验证手段**：静态阅读 + `cargo +1.97.0 test -p cordis-wasm`（lib 5 + 集成 13 = 全绿；含 go_guest 12s）+ `clippy -p cordis-wasm --all-targets -- -D warnings`（0 告警）+ `fmt --check`（干净）+ `doc --no-deps`（**7 条既有历史告警，非 W1b 引入**，见 Minor-2）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：2（协议 §5「错参 → err」未由桥预校验，依赖宿主 op 实现；doc 7 条既有历史告警）
- **nit**：3（Host take 就绪/drop 无直接单测；就绪槽无自动回收；busy-poll 需 W2 端到端确认轮询频率）

W1b 宿主驱动与协议细化逐条对齐：submit 入队 + rep 分配（无 Err 通道、错误延迟 take）、take 未就绪= None、drop 清槽、`drive_pump_remote` 三分支（op+remote→提交 / 未知 op→err / 未配置→err，沙箱不 panic）、`drive_poll_remote`（noop-waker 轮询在途 join → 回填结果槽、完成移除）、适配器 downcast、单测 5 条直证。**线程模型正确**（`RemoteJoin` 非 Send 存 `InstanceState` 而非 `Host`；`Arc<RemoteOp>` Send+Sync 跨 worker；JoinHandle 完成即 Ready 的 busy-poll 语义成立）。**core 零改动**、无 RefCell 借用冲突、沙箱边界（guest 恶意输入不 panic 宿主）。无逻辑缺陷，可放行 W2。

---

## 发现

### Minor

### m-1：协议 §5「参数值类型不匹配 → 句柄 err」未由桥预校验，依赖宿主 op 实现

- **位置**：`drive_pump_remote`（`(Some(op), Some(remote))` 分支直接 `RemoteRequest::boxed(move || op(params))`，无参数预校验）。
- **问题**：协议 §5 期望「参数值类型不匹配 → 句柄 err」；实现中 `op` 是宿主注册的 `Fn(Vec<Value>) -> Value`——**桥不做参数签名预校验/适配**，错参行为完全依赖宿主 op 自身对 `Vec<Value>` 的 downcast 处理：若 op 不校验直接 panic → 在 worker 线程 panic → `await_remote_join` 的 `.expect("远端任务 panic = 宿主 bug")` → `RemoteJoin` poll 时 panic → **组合线程 step 边界 panic**（绕过"错参 err"的沙箱承诺）。未知名/未配置分支已由桥兜底（err，不 panic），唯独错参未兜。
- **建议**：① 桥增可选参数签名预校验/捕获（如对 `op(params)` 套 catch_unwind 转 err、或注册时声明参数 arity/类型），把「错参 → err 不 panic」在桥层收口；或 ② 协议 doc 明示「参数校验与 err 由宿主 op 实现负责；桥仅拦截未知名/未配置」——二选一，推荐 ①（沙箱承诺更硬）并至少在 W2 端到端加「错参 → err 不崩宿主」断言。

### m-2：doc 7 条既有历史告警（非 W1b 引入，呼应仓库 broken rustdoc links 收口纪律）

- **位置**：`crate doc`（InstanceState/WasmTaskIter 私链 + `get_dyn` unresolved）+ `Host`/`run`/`drop` doc 链接 `InstanceState::core_inverses` 私有项——共 7 条（`cargo doc --no-deps` 实测）。
- **问题**：均属 W1b 之前的既有 doc（历史 crate 文档），非本次引入；但与本仓库「broken rustdoc links 归零」的既定收口纪律冲突（M0.x/events/async 各线已归零）。
- **建议**：顺手修复（私链改反引号文本、`get_dyn` 全限定 `Context::get_dyn`）——一行级收口，不阻塞 W2。

### Nit

- **n-1**：`Host::take` 就绪路径（`Some(Ok/Err)`）与 `drop` 清槽**无直接单测**——`host_submit_enqueues_and_take_pending` 只测提交入队 + 未就绪 `None`；就绪回填在纯函数层（`drive_poll_remote`）测，`Host::take` 就绪读取依赖 W2 端到端补。
- **n-2**：已就绪结果槽**无自动回收**——结果槽只在 guest `drop` 句柄或实例卸载时清；协议 §6「宿主在 step 边界清扫」未实现。低影响（就绪槽内存有界 + 卸载清），可观察。
- **n-3**：`busy-poll`（noop waker 不唤醒）——正确性经论证成立（`JoinHandle` 完成标志独立于 waker，下次 poll 即 Ready；跨 runtime poll 安全，M0.6 已验），但 W2 端到端须确认 guest `take` 的轮询频率（step 边界）能及时读到回填。

---

## 通过项（逐条确认）

- **wit `remote` 接口（W1a）**：`submit(name, params) -> handle`、`handle.take() -> option<result<value, string>>`、world 增 `import remote`——与协议 §2 一致 ✓。
- **`Host::submit` 入队 + rep 分配**：`remote_pending.push(RemotePending{rep,name,params})` + `Resource::new_own(rep)`；无 Err 通道、错误恒延迟 take（W-D3 异步契约）✓。
- **`Host::take`**：`remote_results.get(&rep).and_then(|o| o.clone())`——未就绪（槽 None）/无槽均 → `None`；就绪 → `Some(Ok/Err)`；幂等可重读 ✓。
- **`Host::drop` 清槽**：`remote_results.remove(&rep)`——guest 弃句柄/卸载清理 ✓。
- **时序**：`WasmTaskIter::next` 先 `poll_remotes`（回填上批）→ `drive step`（guest 内 take 读回填 / submit 入队）→ `forward_pending` → `pump_remotes`（本步提交 → 注入 Remote）——轮询语义与 W-D3 一致 ✓。
- **`drive_pump_remote` 三分支**：op+remote → `RemoteRequest::boxed(op(params))` + `remote.submit` → `joins`；未知 op / 未配置 → 结果槽 `Err(...)`（不提交、不 panic——沙箱）✓。
- **线程模型**：`RemoteJoin`（`LocalBoxFuture`，非 Send）存 `InstanceState`（`Rc<RefCell>`，非 `Host`——`Host` 在 `Store<T>` 受 `WasiView: Send` 约束）✓；`Arc<RemoteOp>`（`Send+Sync`）经 `RemoteRequest::boxed` move 进 worker（TokioRemote `spawn_blocking`）——`RemoteOp` 的 `Send` 上界编译期约束 op 捕获，`op` 无法抓组合线程 `Rc`（O-6）✓。
- **`drive_poll_remote`**：noop-waker `poll` 在途 join；`Ready` → `value_from_remote` 回填 + 从 joins 移除（完成即弃，契约 C-5）✓。
- **适配器**：`value_from_remote` downcast `Value`；非 Value → `err` ✓；`boxed` 自动装箱 `Value`（`Value: Send` 由编译证实）✓。
- **借用**：`pump_remotes`/`poll_remotes` 中 `store`/`remote`/`remote_ops`/`remote_joins` 均为不同 `RefCell` 或语句级临时（`store.borrow_mut()` 525 与 533 不重叠）——无 `RefCell already mutably borrowed` 风险 ✓。
- **单测 5 条**：真 `TokioRemote` worker 提交 + 轮询回填（`greet→hi`，O-6 worker 执行直证）/ 未知 op err / 未配置 err / 适配拒非 Value / Host 入队 + take None——直证性 ✓（pump/poll 纯函数式设计便于单测，无 wasmtime 依赖）。
- **`532d703` fix**：`Waker::noop` 引用（`&'static Waker` → `from_waker(waker)`）+ `map_err(|e| e)` 收敛——正确 ✓。
- **core 零改动 / 依赖**：`git show` 仅 `crates/cordis-wasm`；run 增 `cordis-async`（本仓库，复用 `Remote`），tokio 仅 dev ✓。

---

## 结论

W1b（宿主驱动：入队/回填/清理 + 操作注册表 + pump/poll 纯函数 + 适配器 + 单测）与 W0 协议细化、W-D1..D4 对齐，核心语义正确、线程/沙箱/借用核查通过、无逻辑缺陷。2 项 Minor（错参桥层收口、doc 历史告警）不阻塞，**建议放行进入 W2**（guest 接线端到端：rust guest 示例 extension + 提交→worker→回灌断言 + 错参 err 不崩断言；并在 W2 补充 n-1 的 take 就绪/drop 单测、确认 n-3 轮询频率）。
