# cordis-async Phase 0 开发计划

**依据**：`docs/cordis-async-protocol-draft.md` v1.4（已冻结，2026-08-18 用户确认开工范围；评审闭环：`docs/ASYNC-PROTOCOL-REVIEW.md` v1.1–v1.3 全部采纳，无遗留阻塞）
**状态**：**待开工指令**（按草案执行纪律：开工由用户另行下达；本计划为执行方案，不含实现代码，未下达前不写实现）
**里程碑门禁**：Phase 0 出口标准 = 草案 §9 的 11 条协议单测 + 3 项 spike 全部通过

---

## 0. 目标与交付物

在 sync `cordis-core` **零语义改动**（唯一例外：评审动作 3 的两个既有工作线小修，见 §1 前置）之上，新增一等 `cordis-async` crate：异步效应协议、取消/错误通道、可 await 卸载编排、`Remote` 桥（TokioRemote v1 / WasmRemote 接入点），并跑通 3 项产品假设 spike。

**最终交付**：
1. `crates/cordis-async`（workspace member）——草案 §0–§7 的协议实现；
2. 11 条协议单测全绿（草案 §9 清单）；
3. 3 项 spike 原型（事件总线、spawn_remote、agent loop）验收通过；
4. 评审动作 3 的两个 core/loader 小修落地（§1）。

## 1. 前置：评审动作 3（并入同一工作线，约 1 commit）

| # | 小修 | 位置 | 说明 | 验证 |
|---|---|---|---|---|
| P1 | `Fiber::target_view()` 只读访问器 | `crates/cordis-core/src/fiber.rs` | `pub fn target_view(&self) -> Option<View>`（borrow 克隆，`target: RefCell<Option<View>>`），与 reload 的 `guard_target` 同款；零语义变化 | `cargo test -p cordis-core` 全绿（现有测试不应受影响） |
| P2 | loader hook 闭包引用环小修 | `crates/cordis-loader/src/lib.rs`（`register_update_hook`/`register_retire_hook`） | 闭包捕获 `Rc<Loader>` 存在 `Loader→runtime→hook→Loader` 环——参考草案 B 方案改捕获弱引用或条目级句柄，消除关停泄漏 | `cargo test -p cordis-loader` 全绿 + 一个"loader drop 后 runtime 可回收"断言 |

> 这两个小修独立可先行（不依赖 async 层），也可与 Step 0 一起；本身是既有工作线收尾，**可纳入本计划第一 commit**。

## 2. 分步计划

### Step 0：crate 骨架

- **目标**：`crates/cordis-async` 立项，workspace 接入，编译通过，空协议类型占位。
- **任务**：
  1. `Cargo.toml`（workspace.package 继承；依赖：`cordis-core` path；run-deps 加 tokio（`current_thread`/`rt` + `macros`/`sync`，按需最小 feature）；dev-deps 加 `tokio` test feature）；
  2. `workspace.members` 增 `crates/cordis-async`；
  3. `src/lib.rs`：草案 §1 类型占位（`AsyncEffectIter`/`AsyncStep`/`AsyncDisposer`/`AsyncFiberError`）+ `#![deny(missing_docs)]` + workspace lints。
- **产出**：可编译 crate；`cargo fmt/clippy/test -p cordis-async` 干净。
- **验证**：`cargo build -p cordis-async` ＋ 空测试通过。
- **门禁**：`cargo fmt --check` + `cargo clippy -p cordis-async --all-targets -- -D warnings`（`unsafe_code=deny` 继承）全绿。

### Step 1：核心协议（草案 §1）——I-1 / I-2

- **目标**：`drive` 引擎实现，I-1（LIFO 逆）与 I-2（步界 guard）单测直证。
- **任务**：
  1. `drive(iter, guard) -> Result<AsyncDisposer, AsyncFiberError>`（草案 §1 转写；guard 每步界检查、Failed → LIFO await 恢复、Ok → 复合逆）；
  2. 测试 1（I-1）：三步 async 效应 + 复合逆，断言逆按应用逆序 await（用日志序/顺序计数器）；
  3. 测试 2（I-2）：长 await 中途 guard 翻假 → 在途步完成、逆入账、恢复正确。
- **产出**：协议核心 + 2 条单测。
- **验证**：`cargo test -p cordis-async`（本步 2 条）。
- **风险**：`LocalBoxFuture`/`Pin<Box<dyn Future>>` 生命周期细节；guard 闭包捕获 `&cx` 的借用。

### Step 2：AsyncCx + 两阶段卸载（草案 §2/§3）——I-3 + drain 重入

- **目标**：AsyncRegistrar 挂 core 生命周期；settle/drain/代次；I-3（依赖者 async 逆先 settle）与 drain 重入直证。
- **任务**：
  1. `AsyncCx`（ctx/fiber/cancel/generation + `get_cloned`/`set`/`cancellation`/`fiber`/`spawn_remote` 签名）；
  2. `AsyncRegistrar`（sync 包装组件：`once` 中 spawn drive + 共享 disposer 槽 + 逆 = cancel + enqueue_tail；捕获 `Rc<AsyncFiberEntry>` 防环，逆契约 C-6）；
  3. `AsyncFiberEntry`（slot/tail 队列/代次/状态）；
  4. `settle`（FIFO 排空：await handle → slot.take → d?.await；drain 重入 → 下一代队列 + 最大轮数 64 守卫；slot Rc 三处持有 liveness）；
  5. 测试 3（I-3）：async 提供者+消费者，退役提供者，消费者 async 逆 settle 先于提供者（日志序直证）；
  6. 测试 5（drain 重入）：逆中注册新效应入下一代被排空；自再生死循环触发守卫 panic。
- **产出**：生命周期核心 + 2 条单测。
- **验证**：`cargo test -p cordis-async`（累计 4 条）。
- **关键**：I-3 顺序由 core Thm 63 级联免费获得（依赖者注册器逆先入队）——实现借用 core unload 顺序即可，勿破坏级联。

### Step 3：失败通道（草案 §3.3）——I-4 + 测试 11

- **目标**：`Failed(e)` → 静止终态 + 自退役（disabled 写回）+ 复活；shutdown 一致性。
- **任务**：
  1. `AsyncFiberState::Failed` + `on_failed`（复用 core/loader G1 通道：`fiber.retire()` → retire hook 写回 disabled；复活 = 重启用 → `update_fiber` → 新代 spawn）；
  2. `AsyncRuntime::shutdown`（契约 C-7：编排方先退役；本方法兜底 cancel + enqueue + settle；双真断言——**按评审建议至少一处正式 assert**，见开放项 §4）；
  3. 测试 4（I-4）：Failed 后 settle 恒完成、is_quiet 真、disabled 写回、重启用复活；
  4. 测试 11：编排方退役 → shutdown 双真；未退役 → 违约捕获；退役零配置污染。
- **产出**：失败/关停 + 2 条单测。
- **验证**：累计 6 条。
- **风险**：failed 时 slot 留空语义、复活路径的新代 spawn 与旧尾巴 settle 的时序。

### Step 4：AsyncRuntime 门面（草案 §5）——测试 8/9/10

- **目标**：门面 API 齐备（use_component/retire/update/settle/shutdown/is_quiet）+ 代次更新 + 无环关停 + H 竞态。
- **任务**：
  1. 门面方法与 `AsyncComponent`/`use_component`（sync 壳挂载）；
  2. 测试 8（代次与更新）：update → 旧代 cancel + 新代 spawn、旧尾巴先 settle；
  3. 测试 9（无环关停）：shutdown 后 AsyncRuntime 可 drop（Weak 计数）；
  4. 测试 10（H 竞态）：drive 恰在 cancel 后 settle 前完成 Ok → 共享槽被 settle 恰一次 take；Failed slot 留空；shutdown 补 enqueue。
- **产出**：完整门面 + 3 条单测。
- **验证**：累计 9 条。
- **风险**：门面与 core 生命周期操作（retire/update）的一致性；C-4 门面纪律文档配套。

### Step 5：Remote 桥（草案 §2/§4）——spawn_remote

- **目标**：`Remote` trait + `TokioRemote`（Send future 分池 / spawn_blocking）v1 实现；WasmRemote 接入点（M1 宿主驱动协议）标注实现位置。
- **任务**：
  1. `RemoteRequest`/`RemoteJoin`/`RemoteValue` + `Remote::submit`；
  2. `TokioRemote`（组合线程向 worker 提交 Send future，join 回灌）；
  3. 单测：spawn_remote 提交 + join 回灌（同步壳 + 远端计算）。
- **产出**：桥 + 1 条单测。
- **验证**：累计 10 条 + 与既有 wasm 桥（M1）语义对照确认不破坏。
- **风险**：`RemoteValue` 跨线程类型；组合线程 LocalSet 与远端 runtime 的边界（不触碰组合线程资源——O-6 纪律）。

### Step 6：Spike 1–3（草案 §9 产品假设验证）→ Phase 0 出口

| spike | 假设 | 步骤 | 通过标准 |
|---|---|---|---|
| S1 事件总线 + `ctx.effect` 订阅原型 | DX 税可承受（随 fiber 卸载自动退订） | 轻量事件总线（`Symbol` 事件名 + emit 派发）；订阅经 `ctx.effect` 注册；async 监听器经 local.spawn_local 投递 | 订阅/退订/级联退订原型跑通；DX 形态可评估 |
| S2 tokio 服务 sync 壳 + `spawn_remote` | 组合线程二分不别扭 | 一个含 tokio 服务的组件（如 LLM client / http 壳）经 spawn_remote 接入 | 同步壳 + 远端调用回路跑通 |
| S3 agent loop 注册器模式组件 | 三端（sync 树/async 层/LLM client）协作完整 | LLM SSE 流式 + 工具调用；卸载时 flush session（逆 await 收尾） | 卸载时 flush 完整、无泄漏 |

**Phase 0 出口判定**：11 条协议单测 + 3 spike 全过 → 进入 Phase 1 决策（草案 §9：任一失败 → 回架构决策表重审 C）。

## 3. 里程碑与时间量级

| 里程碑 | 内容 | 依赖 | 量级（单人） |
|---|---|---|---|
| M0.1 | 前置 P1/P2 + Step 0 骨架 | — | 0.5–1 天 |
| M0.2 | Step 1 协议（I-1/I-2） | M0.1 | 1–2 天 |
| M0.3 | Step 2 生命周期（I-3/drain） | M0.2 | 2–3 天 |
| M0.4 | Step 3 失败通道（I-4/C-7） | M0.3 | 1–2 天 |
| M0.5 | Step 4 门面（代次/关停/H） | M0.3–0.4 | 1–2 天 |
| M0.6 | Step 5 Remote 桥 | M0.5 | 1 天 |
| M0.7 | Step 6 spike 1–3 + 出口判定 | 全部 | 2–3 周（草案量级） |

**每个里程碑门禁（含硬性审查门禁）**：
- **门禁 A（代码门禁）**：`cargo fmt --check` + `cargo clippy -p cordis-async --all-targets -- -D warnings` + 该里程碑单测全绿 + （后续）`cargo test --workspace` 无回归。
- **门禁 B（里程碑间独立审查门禁——强制）**：每个里程碑 M0.N 完成后，**必须先做独立代码审查**（报告入 `docs/reviews/`），评审通过（含修复 split commit 闭环）后**才允许开工下一里程碑 M0.(N+1)**；审查不通过 → 修复循环，**不得跨里程碑推进**。此硬门禁贯穿 M0.1–M0.7（Phase 0 出口判定亦需审查闭环）。

## 4. 开放事项（执行期处理或记录，不阻塞开工）

- **R1（评审建议 §13）**：shutdown 双真断言——草案用 debug_assert，**执行时对关停路径至少一处正式 `assert`**（或写入文档：release 下编排方对退役负全责）；可回提草案 v1.5。
- **O-2**：settle 粒度（批次边界显式调用）——门面默认显式；"自动 settle"模式留 app 层。
- **O-4/O-5**：Failed 载荷富化、非 Arc 值——等真实场景，Spike S3 若暴露再定。
- **C-4 门面纪律文档**：插件作者分界（绕过门面的直接 core 调用不记账）。
- **执行纪律**：本计划不含实现；**开工指令由用户下达**；下达后按里程碑门禁推进（每里程碑提交 + 审查闭环 + 修复 split，沿用仓库纪律）。

## 5. 依赖与纪律约束

- 零第三方依赖纪律仅约束 sync 侧（cordis-core/loader/hmr/macro 保持零第三方）；`cordis-async` 按草案引入 tokio（run-dependency）——**是新 crate 自身依赖，不污染既有 crate**；wasm 桥复用 M1 宿主驱动协议。
- `unsafe_code=deny`、`clippy -D warnings`、`#![deny(missing_docs)]` 全程生效。
- 每里程碑：commit 分 code/docs；独立审查报告入 `docs/reviews/`；修复 split commit。**里程碑间独立审查为硬门禁**（见 §3 门禁 B）——审查未闭环不得进入下一里程碑。
