# 代码审查报告：commit `281c6ac`（P1.3 R1 Send-future 分池形态）

- **审查对象**：`281c6aca67eca4036b39069451a8efdbaca9c9c6` — `feat(async): P1.3 R1 Send-future 分池形态——RemoteRequest 双变体（Closure|Future）+ TokioRemote 双形态调度（spawn_blocking/handle.spawn）+ 单测（提交+join 回灌+O-6 隔离），submit 冻结签名保持（P1.3）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 281c6ac`（crates/cordis-async/src/lib.rs +58/- 与 tests/protocol.rs m06 +66），对照 P1.3 计划 §2 Step R1、草案 v1.4 §2/§4（Remote 桥泛化：Send future 分池 / spawn_blocking 双形态）、P1.2 H3 冻结标注（`submit` 签名不破坏）。
- **验证手段**：静态阅读 + 实测 `GOCACHE=.../gocache cargo +1.97.0 test -p cordis-async` = **24/24**（protocol 21 + spikes 3；含 m06 新增 2 条）。clippy/fmt/doc 由委派方本地已验证绿（本次未复跑，nf 不阻塞）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：3（函数内 `use` 可读性；future/闭包形态分工 doc；AsyncCx future 路径集成测试暂缺）

P1.3 R1 双形态落地与计划 Step R1、草案 §4 完全对齐：`RemoteRequest` 双变体（Closure | Future）封装良好、`submit(RemoteRequest) -> RemoteJoin<RemoteValue>` **冻结签名不变**（扩展在内部变体）；`TokioRemote::submit` 双形态调度（闭包→`spawn_blocking` blocking 池、Send-future→`handle.spawn` multi_thread 池）+ 共用 `await_remote_join` 回灌组合线程；O-6 隔离由测试 2 直证（worker 池线程 ≠ 组合线程）。实测 24/24 全绿。可放行 R2（WasmRemote 接入点 + 协议 doc）。

---

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1（低）：`use RemoteRequestInner as I;` 于函数体内

- **位置**：`src/lib.rs` `TokioRemote::submit`（`use RemoteRequestInner as I;`）。
- **问题**：合法（函数体局部 use），但可读性上建议移至模块顶层（`RemoteRequestInner` 已同模块私有可见）或去掉别名直接列 `RemoteRequestInner::Closure`。无缺陷，纯风格。
- **修法**：可选。

### Nit-2（低）：future 与闭包两形态的分工 doc 未明示「阻塞 vs 异步」职责

- **位置**：`src/lib.rs` `RemoteRequest` doc（「Send 闭包 v1 spawn_blocking 运行」/「Send async future 分池形态 spawn 运行」）。
- **问题**：草案 §4 / O-6 隐含分工——**future 形态适合非阻塞异步计算**（multi_thread 池 `spawn`，线程数有限，阻塞任务会占住 worker 线程）；**阻塞 / CPU 密集用闭包形态**（`spawn_blocking` = 专用 blocking 池，隔离良好）。当前 doc 只述「哪形态走哪池」未言「何时选哪种」。
- **修法**：`RemoteRequest`（或 `from_future`）doc 补一句分工建议，呼应草案 §4 与 O-6（CPU 密集/阻塞 → `boxed` 闭包；异步 IO / 非阻塞计算 → `from_future`）。

### Nit-3（低）：`AsyncCx::spawn_remote` 的 future 路径集成测试暂缺

- **位置**：`tests/protocol.rs` m06 新增两条为**桥层单元**（直接 `TokioRemote::submit`）；`AsyncCx::spawn_remote(RemoteRequest)` 在 async 组件内经 from_future 的**集成路径**未覆盖。
- **问题**：`spawn_remote` 收 `impl Into<RemoteRequest>`；`RemoteRequest::from_future` 是显式构造（future 非 `FnOnce` 故无 `From` 自动通道）——语义上可经 `cx.spawn_remote(RemoteRequest::from_future(...))` 用，但无测试直证组件内 future 提交 + 卸载收账。
- **修法**：R3 双运行时集成示例可用 future 形态覆盖（或 m06 补一条组件内 future 提交用例）。记录即可，不阻塞。

### 核查通过项（逐条）

- **双变体封装**：`RemoteRequest(RemoteRequestInner)` 私有 enum——外部仅经 `boxed`/`from_future`/`From<FnOnce>` 构造，封装良好；`submit` 只接收 `RemoteRequest`，**冻结签名 `submit(RemoteRequest) -> RemoteJoin<RemoteValue>` 未变**（P1.2 H3 保持）✓。
- **双形态调度**：`I::Closure(f) → spawn_blocking(f)`；`I::Future(fut) → worker.spawn(fut)`——前者 blocking 池、后者 multi_thread 池（`TokioRemote.worker` 为宿主 multi_thread Handle）；两路统一 `await_remote_join` 回灌（`JoinHandle<RemoteValue>.await` → `expect` panic=bug 诊断，O-6）✓。
- **Send-future 线程安全**：`from_future(fut: impl Future<Output=RemoteValue> + Send + 'static)`——`Send` 上界编译期强制（future 不得捕获 `Rc`/非 Send），无跨线程竞争路径；`Pin<Box<dyn Future + Send>>` 存储 ✓。
- **跨 runtime join**：组合线程（current_thread + LocalSet）await worker runtime 的 `JoinHandle`——M0.6 已验安全（内部 notify 跨线程唤醒），R1 复用同机制 ✓。
- **O-6 隔离**：闭包/future 均在 worker 侧执行，组合线程只 await join（不触碰 worker 内部资源）；测试 2 直证 `worker_tid != combo_tid`（future 在 multi_thread 池执行）✓。
- **无裸 spawn**：双形态 JoinHandle 均进 `submit` 返回的 join（组合线程持句柄 await），`await_remote_join` 消费句柄——无野任务泄漏（契约 C-5）✓。
- **worker drop 时序**：`#[test]` + 手动 `combo.block_on`（非 async 上下文）+ worker 在 block_on 返回后 drop；`tv`（持 Handle）在 run_until 内已 drop——顺序安全 ✓。
- **单测直证**：R1-a（from_future async 6*7 → join 回灌 42）；R1-b（O-6 隔离线程断言）；闭包形态（m06 既有 spawn_remote 测试）回归——**24/24 全绿** ✓。
- **范围克制**：未触及 WasmRemote（R2 职责）、未破 `submit` 签名、无 core 改动、run-deps 未新增（tokio 既有）✓。

---

## 验证记录（实际执行）

1. `cargo +1.97.0 test -p cordis-async` — **PASS，24/24**：protocol 21（含 m06 新增 `send_future_submits_to_worker_pool_and_joins_back` + `send_future_executes_on_worker_pool_not_combo_thread`）+ spikes 3。
2. `git show 281c6ac` — 静态核对 diff 无越范围改动（仅 lib.rs Remote 区域 + tests m06）。

---

## 结论

P1.3 R1（Send-future 分池形态）与计划 Step R1、草案 §4 完全对齐，双形态调度、冻结签名保持、O-6 隔离、跨 runtime join、无裸 spawn 全部核验通过，测试 24/24 全绿，无逻辑缺陷。**建议放行进入 R2**（WasmRemote 接入点 + 协议 doc）。3 项 Nit 记录在案（函数内 use / 形态分工 doc / future 集成测试），可于 R2/R3 顺手处理，不阻塞。
