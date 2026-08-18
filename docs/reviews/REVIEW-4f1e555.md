# 代码审查报告：commit `4f1e555`（M0.6 Remote 桥，Phase 0）

- **审查对象**：`4f1e5553eba374aaf461ae1319b32eb1e565f7a7` — `feat(async): M0.6 Remote 桥——Remote trait + TokioRemote(spawn_blocking) + AsyncCx::spawn_remote + 单测（Phase 0）`
- **审查日期**：仓库时区（2026-08-18）
- **审查人**：independent-review-agent
- **审查范围**：`git show 4f1e555`（`crates/cordis-async/src/lib.rs` +110/-0、`crates/cordis-async/tests/protocol.rs` +121、`Cargo.toml` +1/-1 dev-dep `rt-multi-thread`），对照 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§2/§4、`docs/cordis-async-PHASE0-PLAN.md` §Step 5（M0.6）。上一里程碑结论：REVIEW-23383f3（PASS WITH NITS，0 Major/0 Minor）。
- **验证手段**：静态阅读 + 实际运行工程门禁命令（见「验证记录」）。

**改动统计**：3 文件，+232/-4。
- `Cargo.toml`：dev-dep `tokio` 增补 `rt-multi-thread` feature（测试需建多线程 worker runtime）。
- `lib.rs` +110：新增 M0.6 段——`RemoteValue`（`Box<dyn Any + Send>` 类型擦除）、`RemoteRequest`（`Box<dyn FnOnce() -> RemoteValue + Send>` + `boxed`/`From` 构造）、`RemoteJoin<T>`（`LocalBoxFuture<T>` 别名）、`Remote` trait（`submit`）；`TokioRemote`（持 `tokio::runtime::Handle`，`submit` = `Handle::spawn_blocking` + join 回灌；`WasmRemote` 为 M1 接入点仅标注）；`AsyncCx` 增 `remote: Option<Rc<dyn Remote>>` 字段 + `spawn_remote`（未安装 panic 诊断）；`AsyncRegistrar` 透传 `remote`；`AsyncRuntime` 增 `remote` 槽 + `set_remote` + `use_component`/`wrap_component` 注入。
- `tests/protocol.rs` +121：新增 `m06` 模块测试 `spawn_remote_submits_to_worker_and_joins_back`；`m05::async_inverse_owned` 升 `pub(super)` 供 m06 复用（同步改动 1 行可见性）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（无直接测试覆盖「挂起 spawn_remote join 期间组件被卸载」的 O-6/I-2 交互路径——新跨线程 await-cancel 语义仅凭同形 i2 泛型测试与代码论证成立，无专属用例）
- **nit**：3（worker 线程隔离未显式断言；set_remote 按注册器快照捕获取代时序未注明「先挂载后安装者需重挂载」；远端确定性的轮询基数约定未在 m06 明确载明 worker 调度假设）

M0.6 Remote 桥（Remote trait + TokioRemote v1 = spawn_blocking + AsyncCx::spawn_remote，草案 §2/§4，计划 §Step 5）实现与草案 v1.4、计划 Step 5 逐条对齐，6 个核查要点全部核实通过：接口形态与草案签名一致（`spawn_remote(&self, impl Into<RemoteRequest>) -> RemoteJoin<RemoteValue>` / `Remote::submit`），`RemoteValue` 类型擦除 + downcast 符合评审点 G 授予的实现自由度；TokioRemote v1 语义完全符合「Send future 分池(spawn_blocking)」描述，join 回灌不触碰组合线程资源（仅 await 跨 runtime 的 `JoinHandle`，为 tokio 显式支持形态）；线程拓扑安全（组合线程 await 跨 runtime JoinHandle 合法，worker Handle 生命周期要求 doc 明示，测试以 `#[test]`+手动 block_on 规避「async 上下文 drop runtime panic」已注释）；注入链无新环（`Rc<dyn Remote>` → `Handle` 无回边）、set_remote 覆盖幂等、未安装 panic = 配置错误纪律；挂起取消语义 safe（settle 恒 await drive 句柄不中止，join 必完成，I-2 在途步逆正常入账；远端任务 worker 侧跑完即弃 = pending-set 泛化语义，无组合线程野任务泄漏）；测试 m06 直证「提交 + join 回灌」（日志 submit→joined、downcast 值 42、退役 settle 收账 rev:remote）、条件轮询决定论约定延续。工程门禁（fmt/clippy -D warnings/doc 0 告警/test 15/15/workspace 全绿/deny(missing_docs)）全部通过，无 unsafe，命名与草案术语一致。

---

## 发现

### Major：无

### Minor：

### Minor-1（中）：无直接测试覆盖「挂起 spawn_remote join 期间组件被卸载」的 O-6/I-2 交互路径

- **位置**：`tests/protocol.rs` `m06` 仅含 `spawn_remote_submits_to_worker_and_joins_back` 一个测试（挂载 → submit → join 回灌 → retire/settle 卸在 join 完成后；全程无「join 挂起中卸载」）；`lib.rs` `TokioRemote::submit`（`Box::pin(async move { handle.await.expect(...) })`）。
- **问题**：`spawn_remote` 引入了一条**新的跨线程挂起**路径——drive 停在 `iter.next().await` 内的 `join.await` 上，与此同时远端 worker 线程仍在跑。核查要点第 4 条要求核对「join 结果被丢弃是否安全、是否有泄漏/野任务」。代码论证成立：settle 恒 `handle.await` drive 任务、从不中止，故 `join.await` 必然等到远端完成、逆经共享槽恰一次收账（I-2 在途步完成后逆正常入账）；`JoinHandle` future 的 drop 不 abort `spawn_blocking`（worker 侧跑完即弃 = 草案 pending-set 泛化语义，结果随句柄 drop 释放）。但这条**跨线程 await + 卸载取消**的组合是本里程碑新增、且与既有 i2 泛型测试不同（i2 用同线程 oneshot/PendingOnce 构造在途步，非真实跨 runtime 的周期性完成）——其「卸载时 worker 仍在算、join 结果被丢弃/被等待」的时序**没有任何专属用例直接断言**。
- **草案/计划依据**：草案 §2（`spawn_remote` join 语义）、§4 O-6、不变量 I-2（挂起中步完成后逆入账）；计划 §Step 5 风险「组合线程 LocalSet 与远端 runtime 的边界（不触碰组合线程资源——O-6 纪律）」。
- **建议**（可选，非阻塞）：新增一个 m06 用例「挂载 spawn_remote 组件 → 等 `submit` 落盘（join 在途）→ `rt.retire` + `settle`，远端稍后完成」：断言①日志最终出现 `joined`（证明卸载未中止在途 join，I-2 成立）与 `rev:remote`（逆恰一次收账）②未出现 panic ③`is_quiet` 真。可直接把 `RemoteBehavior` 的闭包换成带 `tokio::time::sleep` 或 thread::sleep 的慢计算以构造「卸载时仍未完成」窗口。此用例把本里程碑新增的跨线程 await-cancel 语义纳入契约回归面。

### Nit-1（低）：worker 线程隔离未显式断言——值回灌证明跨桥成功，但未证明「计算发生于 worker 线程而非组合线程」

- **位置**：`tests/protocol.rs` `m06` RemoteBehavior/RemoteIter（`cx.spawn_remote(|| -> RemoteValue { Box::new(6u32 * 7) })` + `downcast::<u32>()` 断言 42）。
- **问题**：核查要点第 5 条追问「worker 计算是否真实发生在 worker（spawn_blocking 路径）而非组合线程」。当前断言只证明「结果经桥回灌且类型/值正确」（42 直证 downcast + 回灌），而**未**断言闭包执行线程。从代码结构上必然成立——闭包只交给 `self.worker.spawn_blocking(...)`，tokio 显式调度到与 `worker` runtime 关联的 blocking 池线程，组合线程（current_thread LocalSet）不可能执行它。但测试层面缺少一条「线程分离」的硬断言，使 O-6 隔离停留在结构性论证而非用例直证。
- **草案/计划依据**：草案 §4 O-6（worker 侧仅纯外部 IO/CPU）、契约 C-3（组合线程唯一）；计划 §Step 5 风险注记。
- **建议**（可选）：在 `RemoteBehavior` 的闭包内记录 `std::thread::current().name()`（worker 侧对 blocking 线程命名）写入 log，或断言 `std::thread::current().id() != 组合线程 id`，把 O-6 隔离提为显式断言。

### Nit-2（低）：set_remote 的「按注册器快照捕获」取代替换时序未在 doc 注明「先挂载后安装者需重挂载」

- **位置**：`lib.rs` `AsyncRuntime::set_remote` doc（「须在 use_component/wrap_component 之前调用（注册器捕获桥句柄）；覆盖幂等」）与 `use_component`（`AsyncRegistrar::new(..., self.remote.borrow().clone())`）、`wrap_component`（同捕获）。
- **问题**：`remote` 句柄在 `use_component`/`wrap_component` **构造注册器时**快照进 `AsyncRegistrar`（非 apply 时读取 runtime 活状态）。因此 `set_remote` 覆盖幂等指的是「替换 runtime 槽」，**不**回溯已挂载注册器——已挂载组件保持首次捕获（多为 `None`）直到重挂载。doc 前句已提示「调用在 use_component 之前」、语义正确，但未言明「对已挂载组件的注册器无后效」。属 doc 表述层面的时序边界说明不足，非语义错误（单宿主正常按顺序 set_remote → 挂载即可）。
- **草案/计划依据**：草案 §2/§4（Remote 桥注入方 = 宿主）；审查要点第 3 条「set_remote 覆盖幂等」。
- **建议**（可选）：`set_remote` doc 补一句「覆盖只影响随后 `use_component`/`wrap_component` 快照的组合；已挂载注册器不回溯，需重挂载」。

### Nit-3（低）：m06 远端确定性的轮询基数未载明「worker 阻塞线程及时调度」假设

- **位置**：`tests/protocol.rs` `m06` `for _ in 0..64 { tokio::task::yield_now().await; if log.any("joined") break; }` 与实验证注释（「决策论约定：next() 内 await join，回灌完成后才记 joined——条件轮询即就绪即停」）。
- **问题**：断言的确定性依托 64 次 `yield_now` 轮询 + 远端闭包瞬时完成（6×7）。此处回灌完成依赖「blocking 池线程被 OS 及时调度 + 跨线程 notify 唤醒 current_thread 任务」，比既有 m05/m04 的同线程 next()-单poll 落盘多一层线程调度依赖（远端快故轮询内必达，概率上稳固，但非纯单线程决定论）。break-on-condition 形态正确、与 m05 约定延续，无现实 flaky。
- **草案/计划依据**：草案 §9 测试风格 + m04/m05 决定论约定（REVIEW-596125d nit-1 / REVIEW-23383f3 nit-3）；审查要点第 5 条「条件轮询决定论约定是否延续」。
- **建议**（可选）：注释补一句「远端任务瞬时完成；若其慢于 64 次 yield 轮询则 break 不命中、断言失败=调度依赖，非 flaky 主诉」（或把远程计算换成本地可控的主动 notify 直证），把线程调度假设显式化。

### 未发现问题的核查点（逐条确认）

- **接口形态（第 1 点 / §2 与 §4 Remote）—核实通过**：草案只定 `spawn_remote(&self, impl Into<RemoteRequest>) -> RemoteJoin<RemoteValue>` 与 `Remote::submit(req: RemoteRequest) -> RemoteJoin<RemoteValue>` 签名，载荷具体形态留给评审点 G。实现 `RemoteValue = Box<dyn Any + Send>`（类型擦除 + join 侧 `downcast`）+ `RemoteJoin = LocalBoxFuture<T>` + `RemoteRequest(Box<dyn FnOnce() -> RemoteValue + Send>)` + `boxed`/`From` 两构造，与计划 §Step 5 任务 1（RemoteRequest/RemoteJoin/RemoteValue + Remote::submit）逐字对齐，`#[deny(missing_docs)]` 下文档完整。TokioRemote v1（`Handle::spawn_blocking` + `handle.await.expect(...)` 回灌）完全符合草案「Send future 分池 / spawn_blocking」描述；O-6 纪律「worker 侧仅纯外部 IO/CPU」与 join 回灌不触碰组合线程资源（仅在组合线程 await 跨 runtime 的 `JoinHandle`）成立；`WasmRemote` 接入口 = trait 位（doc 明确 M1 PR #11–13、guest 无自发线程、submit=入队宿主驱动、语义不变、Phase 0 不实现）——符合计划「标注实现位置」要求。
- **线程拓扑（第 2 点 / C-3 / O-6）—核实通过**：组合线程（current_thread + LocalSet）await 跨 runtime 的 `JoinHandle` 是 tokio 显式支持的形态（JoinHandle 的 future 仅通过内部 notify 唤醒，无 LocalSet/runtime 绑定限制，跨线程可安全 await）。`TokioRemote` doc 明示生命周期「worker（Handle）须比桥存活更久，worker 关闭后 submit 会 panic=宿主配置错误=bug」。测试 worker drop 时序处理正确且已注释：用 `#[test]` + 手动 `combo.block_on`，worker runtime 在 `block_on` 返回后（非 async/blocking-guard 上下文）drop，规避 tokio「Cannot drop a runtime in a context where blocking is not allowed」panic；远端任务在 drop 前已 join 完成（`joined`+42 断言先达成），无在途任务随 worker drop 被截断。
- **防环/所有权（第 3 点）—核实通过**：注入链 `AsyncRuntime.remote (RefCell<Option<Rc<dyn Remote>>>)` → `AsyncRegistrar.remote` → `AsyncCx.remote`，`TokioRemote` 仅持 `tokio::runtime::Handle`（廉价内部 `Arc`、`'static`，无任何回边到 AsyncRuntime/组合线程）——不引入新环；`Rc<dyn Remote>` 克隆沿 cx/iter 传播不跨线程（组合线程内），RemoteValue 才跨线程（`Send`）。`set_remote` 覆盖幂等（`*self.remote.borrow_mut() = Some(...)`），RefCell 在 `use_component`/`wrap_component` 处 `borrow().clone()` 快照后即释放、跨 sync 调用无借冲突。未安装时 `spawn_remote` panic（「宿主配置错误=bug」诊断，契合约 C-3 同款纪律）符合审查要点第 3 条「配置错误=bug」。
- **挂起取消语义（第 4 点 / C-5 / I-2 / pending-set 泛化）—核实通过**：drive 在步界查 guard，`next()` 内 `join.await` 挂起不中断；fiber 卸载（cancel）后 join 仍完成、步产逆、drive 回环再查 guard 退场 → 逆入共享槽经 settle 恰一次收账（I-2 成立）。settle 恒 `handle.await` drive 句柄、从不 drop 中止，故在途 join 必等待完成；`spawn_blocking` 的 JoinHandle 被 `submit` 返回的 LocalBoxFuture 持有，其 drop 不 abort 远端任务（worker 侧继续跑完即弃 = 草案 pending-set 泛化语义，结果随句柄释放）——**无组合线程野任务泄漏**（组合线程侧的记入账延续 = drive 记账任务的一部分，可追达；worker 侧远端任务不在组合线程记账范围，与草案对 pending-set 远端任务的「完成即弃」语义一致）。`i2::guard_flips_while_inflight_step_pending` 同形悬起路径已先行覆盖。（注：跨线程 await-cancel 组合未见专属用例，见 Minor-1。）
- **测试质量（第 5 点）—核实通过（除 Minor-1）**：m06 直证「提交（`submit` 落盘）+ join 回灌（`joined` 落盘）+ downcast 值 42 + 退役 settle 逆 `rev:remote` 恰一次」日志序，worker 计算经 `worker.spawn_blocking` 结构性必然发生于 worker 池线程（见 Nit-1 的未显式断言）。条件轮询 `if log.any("joined") break` + 64 上限延续 m05 约定（见 Nit-3）。`async_inverse_owned` 升 `pub(super)` 最小化复用改动，无行为变化。
- **工程门禁（第 6 点）—核实通过**：见「验证记录」。`#![deny(missing_docs)]` 生效（所有新增公开项—`RemoteValue`/`RemoteJoin`/`RemoteRequest`+`boxed`/`Remote` trait+`submit`/`TokioRemote`+`new`/`AsyncCx::spawn_remote`/`AsyncRuntime::set_remote`—均有 doc 注释；私有元组字段不需 doc）。无 unsafe（workspace `unsafe_code="deny"` 在 Cargo.toml:26，src 无 `unsafe` 词）。命名与草案术语一致（RemoteValue/RemoteRequest/RemoteJoin/submit/spawn_remote），dev-dep `rt-multi-thread` 恰为测试建多线程 worker 所需、不自带进 lib 依赖（lib 仅 `tokio = {features=["rt"]}`，worker async 拓扑在本层 doc 约定、unused 特征不放生产 deps）。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `cargo test -p cordis-async` — **PASS**，15/15（0 unit + 15 integration protocol.rs：`i1`、`i2`×3、`m03`×2（含 1 个 `should_panic`）、`m04`×3、`m05`×4、`m06`×1）。其中 `m06::spawn_remote_submits_to_worker_and_joins_back ... ok`。0 doc。
2. `cargo test --workspace` — **PASS**，exit 0，全部 `test result: ok`，cordis-async 15、cordis-core 55、cordis-loader 49 等各套件无 FAILED / `error[` / `warning:` 行。
3. `cargo clippy -p cordis-async --all-targets -- -D warnings` — **PASS**，exit 0，无警告。
4. `cargo fmt --check` — **PASS**，exit 0。
5. `cargo doc -p cordis-async --no-deps`（`RUSTDOCFLAGS="-D warnings"`）— **PASS**，0 告警，exit 0。

---

## 结论

M0.6（Step 5：Remote 桥 —— Remote trait + TokioRemote v1(spawn_blocking) + AsyncCx::spawn_remote + 单测，草案 §2/§4，计划 §Step 5）实现与草案 v1.4、计划 Step 5 完全对齐，6 个核查要点所有关键路径核实通过，无逻辑缺陷。TokioRemote v1 语义符合「Send future 分池 / spawn_blocking」描述，join 回灌不触碰组合线程资源；线程拓扑安全（跨 runtime await JoinHandle 合法、worker 生命周期 doc 明示、worker drop 时序规避正确且注释）；注入链无新环、set_remote 覆盖幂等、未安装 panic = 配置错误；挂起取消语义 safe（settle 恒 await、join 必完成、I-2 逆恰一次收账、远端 worker 跑完即弃、无组合线程野任务泄漏）；m06 直证「提交 + join 回灌 + downcast 42 + 退役收账」且条件轮询决定论约定延续。工程门禁（fmt/clippy -D warnings/doc 0 告警/test 15/15/workspace 全绿/deny(missing_docs)）全部通过，无 unsafe，命名与草案术语一致。

**建议放行进入 Phase 0 出口（Step 6：Spike 1–3 + 出口判定，草案 §9 / 计划 §Step 6）。**

通过前无必须修复项（Major 0）。1 项 Minor（无专属用例覆盖「挂起 spawn_remote join 期间组件被卸载」的 O-6/I-2 交互，代码语义正确但建议补一用例进契约回归面）与 3 项 Nit（worker 线程隔离显式断言、set_remote 快照取代替换时序 doc 注记、m06 远端确定性调度假设注记）记录在案，可在 M0.7 或后续小修一并处理，不阻塞合入。
