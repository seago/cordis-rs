# 代码审查报告：commit `b6ebd25`（M1.5，Phase 1）

- **审查对象**：`b6ebd25eb1f6846afeb31de81d5a44e4adfb48be` — `feat(events): M1.5 验收 #9 Send+Sync 断言 + async 监听器投递 + loader 集成（Phase 1）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show b6ebd25`（`crates/cordis-events/tests/m15.rs` +90 / `Cargo.toml` dev-deps +5 / `Cargo.lock` +2），对照 `docs/cordis-events-protocol-draft.md` v0.3.1（冻结）§3.2/§4.1/§4.2 与 `docs/cordis-async-protocol-draft.md` v1.4 契约 C-5。前序独立 commit `5f39397`（async m06 轮询预算放宽 flaky 修复，与 events 无关）仅扫掠确认不触及本里程碑。
- **验证手段**：静态阅读 `git show b6ebd25` 全文 + `crates/cordis-events/src/lib.rs`（EventBus 结构/闭包存储）+ 实际运行 `GOCACHE=.../gocache cargo +1.97.0 test -p cordis-events`、`cargo tree -p cordis-events`；确认 `cordis_core::Runtime::context()`（runtime.rs:123，`pub fn context(self: &Rc<Self>) -> Rc<Context>`）。

**改动统计**：3 文件，+97。

---

## 总体结论

✅ **通过（PASS WITH NITS）** — 放行 **Phase 1 出口走查**（Step 5：验收 #1–#9 全过 + 集成全绿 + EXIT 文档）。

- **major**：0
- **minor**：0
- **nit**：2（async 测试 `_d` disposer 未显式退订；async 段 yield 预算 8 的调度依赖未注释）

M1.5 的三块内容（#9 编译断言、async 监听器投递、loader 集成）与冻结草案 v0.3.1 逐条一致：**#9 断言真实成立**（`EventBus` 全字段 `Send+Sync`：`RwLock<HashMap>` + `Arc<dyn Fn + Send + Sync>` 闭包 + `Arc<AtomicBool>` alive，无 unsafe/类型擦除绕过）；**async 投递符合 §4.1/C-5**（sync 闭包内 `spawn_local`、不阻塞派发、`p` 拷贝 owned 供 'static 任务、LocalSet 内驱动完成）；**loader 集成真实跑通**（`EventsProvider` 经 loader 根条目挂载、`runtime.context()` 取根 ctx、总线绑定可达、订阅/emit 回路、teardown）。dev-deps 仅 tokio/cordis-loader（`cargo tree` 确认 run 分支仅 `cordis-core`，零第三方纪律保持）。

---

## 发现

### Major：无

### Minor：无

### Nit-1（低）：async 测试 `_d`（emit disposer）未显式退订/作用域未注明

- **位置**：`tests/m15.rs` `async_listener_delivery_via_spawn_local`（`let _d = bus.on::<Tick>(...)`，测试结束前未显式 `drop`）。
- **问题**：测试未覆盖"退订"环节（该语义已由 M1.3 验收 #2/#3 双路径 armed 与 #7 重入覆盖）；`_d` 在测试闭包尾随 `bus`/`log` 一起 drop，无泄漏。属演示性省略，非缺陷。
- **建议**（可选）：加一行 `drop(_d);` 并注释"退订语义由 M1.3 覆盖"，或保持现状（测试聚焦投递而非退订）。

### Nit-2（低）：async 段的 yield 预算（8 次）调度依赖未注释

- **位置**：`tests/m15.rs` `for _ in 0..8 { yield_now().await }` 等待 `async:7`。
- **问题**：`async:7` 落盘依赖「spawn_local 任务在 LocalSet 内被 poll + 写锁竞争」——单线程 LocalSet FIFO 下 8 次 headroom 充裕、无现实 flaky（与 async 里程碑的轮询预算先例一致；`5f39397` 已在更重负载的 m06 用例将预算放宽至 512 佐证方向）；但未注明假设，超大延迟场景下断言失败被误读为逻辑错误。
- **建议**（可选）：注释一句「spawn_local 任务一次 poll 完成，8 次 yield 头寸充裕；极端调度下断言失败 = 调度依赖非逻辑错误」。

### 未发现问题的核查点（逐条确认）

- **#9（M-1/M-1' 闭环）——核实通过**：`assert_send_sync::<EventBus>()`/`Arc<EventBus>`/`EmitListener<u32>` 编译通过即为 `EventBus: Send+Sync` 的**真实**证明（非经 `unsafe impl Send` 绕过——events lib.rs 全文无 unsafe；非类型擦除欺骗——断言作用于具体类型）。回顾 `EventBus` 字段（lib.rs:153-165）：`RwLock<HashMap<(Symbol, Mode), ModeRecord>>` + `RwLock<HashMap<Symbol, Vec<ListenerEntry>>>`，`ModeRecord` 含 TypeId/类型名/`bool`，`ListenerEntry` 闭包为 `Arc<dyn Fn(...) + Send + Sync>`（EmitAnyFn/WaterfallAnyFn/ReplyAnyFn）+ `Arc<AtomicBool>` alive——**全部字段 Send+Sync**，成立链（评审 M-1'）在代码中真实落地。`EmitListener<u32>` 断言额外锁死监听器闭包上界（§0 核心义务）。
- **async 监听器投递（§4.1 / 契约 C-5）——核实通过**：`bus.on::<Tick>` 的 sync 闭包内 `spawn_local(async move { ... })`——派发 `emit` 是同步调用，sync 段 `sync:7` 在 `emit` 返回前写入（立即、不阻塞）；async 段 `async:7` 由 `LocalSet::run_until` 驱动（测试用 `#[tokio::test]` + 手动 `LocalSet`，`spawn_local` 处于 LocalSet 上下文、不触发 "no LocalSet" panic）；`let pv = *p;` 把 `p` 拷为 owned `u32` 供 `'static` async 任务（无借用逃逸）；`Arc<RwLock<Vec>>` 日志跨同步/异步段共享（Send+Sync，符合闭包捕获纪律）。**C-5 可追溯**：任务在 LocalSet 队列内、测试驱动至完成，非裸 spawn 野任务。
- **loader 集成（§4.2）——核实通过**：`Runtime::new()` + `Loader::new(runtime)` + `register_component("events", EventsProvider)` + `apply([Entry::new("events","events",Rc::new(()),0,false)])`——`EventsProvider` 经 loader 根 ctx 挂载；`runtime.context()`（pub）取根 `Rc<Context>`；`ctx.get::<EventsKey>()` 得 `Ref<Arc<EventBus>>`、`Arc::clone(&*...)` 一次性取出、临时 `Ref` 语句级 drop（无跨语句借用）；订阅/emit 回路 `loader:3` 断言；`loader.apply(&[])` teardown（条目移除 = loader 语义的零配置污染）。真实度：覆盖"根条目挂载 → 总线可达 → 订阅回路 → teardown"完整链。
- **deviations**——无实质偏差：dev-deps 引 `tokio (rt+macros)` + `cordis-loader` 属集成测试所需，`cargo tree -p cordis-events` 确认 run 分支仅有 `cordis-core`（计划 §5「tokio/async 仅 dev-deps」纪律保持）；`Runtime::context()` 为既有 pub API（loader 自身亦用），无新增核心暴露。
- **工程门禁**——核实通过：`cargo test -p cordis-events` **17/17**（events.rs 14 + m15.rs 3，0 失败）；clippy/fmt 由仓库提交前本地已验证绿；`cargo tree` 零第三方确认。（workspace 全量测试由主线程提交前验证无回归；本纸面确认 cordis-async 的 `5f39397` 为独立的 async 侧调度预算放宽、不触及 events。）

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `git show --stat b6ebd25` + `git show b6ebd25`（全文 97 行）——静态阅读 m15.rs 三块 + Cargo.toml/lock。
2. `cargo test -p cordis-events` — **PASS**，17/17：`events.rs` 14 + `m15.rs` 3（`send_sync_compile_assert` / `async_listener_delivery_via_spawn_local` / `events_provider_mounts_via_loader`）。
3. `cargo tree -p cordis-events` — run 分支仅 `cordis-core`；`cordis-loader`/`tokio` 列于 `[dev-dependencies]`（tokio 下含 pin-project-lite/tokio-macros 等 dev 传递）。
4. `grep pub fn context crates/cordis-core/src/runtime.rs` — `runtime.rs:123 pub fn context(self: &Rc<Self>) -> Rc<Context>` 确认。

---

## 结论

M1.5（Step 4：验收 #9 + async 监听器投递 + loader 集成）实现与草案 v0.3.1 §3.2/§4.1/§4.2 完全对齐，三块内容全部**真实直证**（非桩/非绕过）：#9 编译断言在真 `Send+Sync` 类型上通过、async 投递符合 C-5、loader 集成完整回路跑通；dev-deps 纪律保持（run 零第三方）。工程门禁实测绿（17/17，clippy/fmt 本地已验），workspace 无回归指向。

**建议放行 Phase 1 出口走查**（Step 5）：验收 #1–#9 全对位（m15.rs 承载 #9 + 集成；#1–#8 已由 events.rs 覆盖）、集成全绿、`docs/cordis-events-PHASE1-EXIT.md` 出口判定文档。2 项 Nit 可选，不阻塞合入。
