# 代码审查报告：commit `32a913d`（PR #17 / M2-PR1——L-Raise 失败模型落地，处置⑤/⑧ 收官）

- **审查对象**：`32a913dd3c539fbe855d3d8c3594a3d3ad673469`（`crats/cordis-core/src/fiber.rs` +15/−2、`crates/cordis-core/src/runtime.rs` +32/−1、`crates/cordis-core/tests/failure_model.rs` +184、`crates/cordis-wasm/src/lib.rs` +26/−5、`crates/cordis-wasm/tests/sandbox_isolation.rs` +47/−74）及配套 docs 提交 `f26f81b399d933a2476af622facd51fe3976baff`（`docs/THEORY-MAP.md` +7/−4、`docs/PLAN.md` +1/−1）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show 32a913d` / `git show f26f81b` 逐行核对 diff；读 `fiber.rs`（FiberError::raise / FiberState 形状）、`runtime.rs`（reload / unload / is_quiet / refresh / compute_target / register）、`effect.rs`（execute / StepGuard / PushingIter 通道）、`context.rs`（set / set_dyn / effect / dispose_all / guarded_pair）、`cordis-wasm/src/lib.rs`（WasmTaskIter / forward_pending / call_step / InstanceState / Host）；读 THEORY-MAP「M1 Wasm 后端走查记录」§6.3 行 + 处置清单⑤/⑧；**实跑**：在干净 worktree（`git worktree add /tmp/cordis-review-17 f26f81b`，避开工作树未提交的 PR #18 中间态）构建 4 个 Rust wasm guest + Go guest，`cargo test --workspace`（**27 二进制全部 `ok`、0 failed**）、`cargo fmt --all -- --check`（exit 0）、`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets`（干净）、`cargo run -p hello-plugin`（全部断言通过）。

---

## 结论：**通过**

L-Raise 失败模型（论文 §4.3.4 𝔈fail、Table 2 L-Raise、Def 49 式 (45) 的 ζ 析取）实质落地，与实现语义一致：组件迭代器以 `FiberError` 载荷 raise → `reload` 的 `catch_unwind` 识别 → 目标置 ⊥、经卸载路径恢复已完成步骤、终态 `Inactive(Some(ζ))`；其余 panic（宿主 bug）`resume_unwind` 重抛。wasm 桥接把 guest trap / 越界 set（核心纪律 panic 载荷）/ AlreadyBound 统一转 `FiberError::raise`，沙箱隔离从"catch_unwind 兜底"升级为失败模型；可信原生组件的越界写仍为 panic = bug（边界记录准确）。已完成步骤的恢复经 `ctx` 累加器（`set`/`set_dyn` → `effect` → `PushingIter` 产出时入栈）成立，`execute` 本地 `acc` 丢失由共享 `StepGuard` 幂等句柄绕开双路径不自洽。发现 0 项 blocker/major、若干 nit（测试覆盖缺口 + 文档细小失准），均不阻塞合入。

> **范围说明**：审查基准为 PR #17 的**已提交状态**（`32a913d` + `f26f81b`）。审查期间发现工作树存在并发开发的**未提交** PR #18（M2-PR2 interception 求值形态）中间态，且其一度因 `Component` 未导入 `fiber.rs` 导致本地 `cargo test` 编译失败（E0405）——此非 PR #17 缺陷。为隔离，在干净 worktree 复现 PR #17 全绿（见上），结论不受并发工作干扰。

---

## ⚪ 细节（nit）

### nit1. 无「非 FiberError panic → resume_unwind 重抛」的直接测试——L-Raise 的核心判别（失败 vs 宿主 bug）仅单向验证
**位置**：`crates/cordis-core/src/runtime.rs:371-379`（负向分支 `Err(other) => resume_unwind(other)`）；测试侧 `crates/cordis-core/tests/failure_model.rs` 全文与 `crates/cordis-wasm/tests/sandbox_isolation.rs` 全文均无对应 `#[should_panic]` 或 `catch_unwind` 断言。

**事实**：`failure_model.rs` 的 2 个测试只覆盖**正向**路径（`FiberError` 载荷被识别为失败、宿主存活、重试激活）；`sandbox_isolation.rs` 的 3 个测试覆盖 trap/越界 set 的失败 outcome 与 `FiberError` 载荷类型约定。**没有任何测试**验证「组件迭代器抛普通 panic（如 `panic!`、`unwrap` 失败、非 `FiberError` 载荷）时，`reload` 的 `catch_unwind` 经 `downcast::<FiberError>()` 失败后 `resume_unwind` 原样重抛、**不**被静默吞成失败 outcome」。而这正是 L-Raise 设计的**核心判别语义**——`catch_unwind` 是"宽捕获"，其正确性完全依赖 `downcast` 精确区分 `FiberError`（组件失败）与其它（宿主 bug）。若未来有人把 `downcast` 分支改成"任何 panic 都当失败"（一类真实的回归风险），现有测试会**照常全绿**。

**影响（非 block/major）**：当前实现正确（`resume_unwind` 分支清晰、注释完备），且单线程宿主下"panic = bug"由 ADR 兜底；但这是该 PR **首次引入的 panic 判别边界**，负向行为无测试护住，与仓库「关键边界必直证」的纪律（对比 PR #15 对记录⑧的直证要求）存在口径落差。**建议**：在 `failure_model.rs` 补一个 `#[should_panic]` 测试——组件迭代器 `panic!("宿主 bug")`（非 FiberError）→ 断言 reload 重抛（`use_component` 整体 panic），固化"其余 panic = bug 不吞"。

### nit2. `fiber.rs` 的 `FiberState` 枚举文档注释仍称 ζ「当前恒 `None`」——与落地状态矛盾
**位置**：`crates/cordis-core/src/fiber.rs:68`（`FiberState` 的 doc comment，L-Raise 落地后未更新）

**事实**：该行原文 `/// - `ζ`（错误结果）：当前恒 `None`（L-Raise 随 async 接入）。`——PR #17 已让 `Inactive(Some(ζ))` 成为真实可达终态（`runtime.rs:383`），`ζ` 不再"恒 None"；同一 PR 的 `FiberError` 文档（`fiber.rs:32-34`）已正确更新为"M2-PR1 落地，L-Raise"，但 `FiberState` 枚举头注释（L68）漏改，形成同文件内自相矛盾。属文档卫生 nit，建议改为「`ζ`（错误结果）：`Inactive(Some(ζ))` 为 L-Raise 失败终态（M2-PR1 落地）」。

### nit3. `Unloading { outcome }` 字段在全仓库仍恒为 `None`——Def 49 的 `Unloading(g, ω, ζ)` 中间形态未贯通，`outcome` 为**死字段**
**位置**：`crates/cordis-core/src/fiber.rs:83-90`（`Unloading { view, outcome: Option<FiberError> }`）；`crates/cordis-core/src/runtime.rs:335`（`mark_unloading` 唯一构造点 `outcome: None`）

**事实**：`grep outcome` 全仓库仅 3 处命中——`fiber.rs:89`（字段声明）、`runtime.rs:335`（构造恒 None）、文档注释。L-Raise 失败路径的时序为 `Reloading →（catch）→ mark_unloading（Unloading{outcome:None}）→ unload 收尾 Inactive(None) → 覆写 Inactive(Some(ζ))`，即 ζ 直接落入 `Inactive(Some(ζ))`，**绕过了**论文 `Unloading(g, ω, ζ)` 携带 `ζ`（卸载结果）的中间态。因同步核心中 `Unloading` 是瞬态（`is_quiet`/`state()` 在两次同步步骤间不可观测），此为**良性的建模简化**；但 `outcome` 字段从此成为「声明了却永不承载值」的死字段，且与 `fiber.rs:89` 注释「ζ：错误结果」的承载位置（实际承载于 `Inactive(Some)` 而非常规 Unloading）存在概念错位。建议：要么在注释中如实说明「同步核心 ζ 直落 Inactive，Unloading.outcome 恒 None（历史字段）」，要么随 async 化（PR #5 后转换真正拆分时）贯通。

### nit4. wasm 桥接 `forward_pending` 的 `catch_unwind(AssertUnwindSafe(|| set_dyn))` **捕获面过宽**——notify/reactor 级联中的宿主 bug panic 会被误判为「组件失败」
**位置**：`crates/cordis-wasm/src/lib.rs:382-397`

**事实**：`set_dyn` 并不只是「检查供给纪律 + 绑定」——它内部经 `effect → once 回调 → store.borrow_mut().bind → ctx.notify(&[realm])`，而 `notify` 会级联触发 `notify_fibers → refresh → reload/unload`（fiber 反应器）与用户注册的 reactor（`runtime.rs` 的 Reactor 列表）。因此 `catch_unwind` 包住 `set_dyn` 实际也包住了**整个 notify 级联**。若级联中出现一个真实的宿主 bug panic（例如用户 reactor 或 fiber 反应器内部 `expect` 失败），它会被 `Err(payload)` 分支捕获，经 `downcast_ref::<&'static str>`/`<String>` 命中（`panic!(...)` 的载荷恰是 `String`），**转成 `FiberError::raise`**——即把宿主 bug 静默降级为「组件失败 outcome」，而非按 `runtime.rs:378` 的契约 `resume_unwind` 重抛。

**为何是 nit 而非 major**：(1) 当前核心纪律 panic（"越界写入未声明键"）与 `AlreadyBound` 都发生在**绑定之前**（`context.rs:211-224` 前置检查、无状态变更），`catch_unwind` 实际要捕获的「组件侧失败」都发生在 mutation 之前，不会残留半绑定；(2) 同步核心下 reactor 是内置且可信的（`notify_fibers`），用户 reactor panicking 属 ADR "panic = bug" 范畴，现实触发概率低；(3) 桥接本意是「不可信 guest 的越界写」→ 失败 outcome，而「越界写」的判别本应**收窄到供给纪律检查本身**（`set_dyn` 的 `!allowed` 分支）。**建议**：把纪律检查从 `set_dyn` 内部的前置 `panic!` 改为返回 `Result`（或桥接层先查 `provide` 再决定是否调 `set_dyn`），使 `catch_unwind` 只为「精确的组件纪律违反」设防，避免把 notify 级联的宿主 bug 一并吞掉；至少在注释中标注「本 catch_unwind 的捕获面含 notify 级联，宿主 reactor panic 会被误判为失败」的已知边界。

### nit5. `FiberError::raise` 采用 `panic_any` 作为控制流手段——`panic = "unwind"` 依赖、且 `catch_unwind` 仅捕获 `UnwindSafe` 载荷，跨版本/`panic=abort` 配置下语义会退化
**位置**：`crates/cordis-core/src/fiber.rs:50-52`（`panic_any(self)`）

**事实**：用 panic 作为**可恢复的控制流**（效果/异常化）在 Rust 中合法但依赖若干隐式前提：(1) 未设 `panic = "abort"`（该 profile 下 `catch_unwind` 永不捕获，`raise` 直接终止进程——当前仓库未设 abort，OK）；(2) `panic_any` 载荷须 `Send + 'static` 才能过 `catch_unwind` 的 `downcast`（`FiberError(String)` 满足）；(3) `-C panic=unwind` 下的栈回退成本。这些前提在当前配置下均成立（实跑全绿），且 `FiberError` 的设计（`panic_any` + `downcast::<FiberError>`）是**正确且惯用的**判别式异常实现。属「实现取舍」记录而非缺陷，仅提示：若未来引入 `panic = "abort"` release profile 或依赖库触发 abort，`raise` 会变成硬崩溃。建议在 `FiberError::raise` 的 doc 中补一句「依赖 `panic = unwind`（本项目未设 abort）」。

### nit6. `call_step` 的 `unwrap_or_else(|err| ...raise())` 把**所有** wasmtime 错误（含非 guest trap 的宿主侧资源错误）统一转失败 outcome——判别粒度与 trap 语义注释存在口径落差
**位置**：`crates/cordis-wasm/src/lib.rs:420-422`（`call_step(...).unwrap_or_else(|err| FiberError::new(...).raise())`）

**事实**：注释（L410-412）明示「guest trap（wasmtime 错误）」，但 `call_step` 返回的 `wasmtime::Result` 错误**不止 guest trap 一种**——理论上也含宿主 embedder 侧错误（资源表耗尽、WASI 内部错误等）。当前实现把 `err` 一律转 `FiberError`（组件失败），对"guest trap"是正确语义，对"宿主侧驱动失败"则是**粒度过度**（本应 panic = bug）。实际触发面极小（`call_step` 的宿主侧错误在单线程同步驱动下罕见），故为 nit；与 nit4 同源（桥接层 `catch_unwind`/错误转失败的面宽于"不可信 guest 行为"）。如追求精确，应在 `call_step` 的 Err 分支区分 `Trap`（转失败）与其它（`expect`/panic）。

### nit7. loader 的 `instantiate` 仍 `panic!` 于 `RegistryError`，但对「失败 fiber（`Inactive(Some(ζ))`）」静默视为实例化成功——`use_component` 返回语义变化后 loader 未检测 fiber 失败态
**位置**：`crates/cordis-loader/src/lib.rs:225-240`（`instantiate` 的 `use_component(...).unwrap_or_else(|err| panic!(...))`）

**事实**：PR #17 改变了 `use_component` 的**可观察返回**——此前组件 `apply` 失败会 panic（宿主崩溃），现在返回 `Ok(fiber)` 且 fiber 处于 `Inactive(Some(ζ))`。loader 的 `instantiate` 只对 `RegistryError`（ProvisionClash 等 O-Insert 前置失败）`panic!`，对「成功注册但组件失败」的 fiber **不检查 `state()`**，直接 `update(...)` 存为 `loaded.fiber`。这意味着一个 wasm 组件若在首次 `step` 时 trap/越界 set，loader 会**静默**把失败的 fiber 当作"已加载条目"记录，不向上报告任何错误。对本 PR 而言这是**行为变化而非回归**（M2-PR1 的验收恰是"失败不 panic"），且 loader 尚未实现「失败态回填/上报」需求；但这是一个**未显式标注的假设**（loader 依赖调用方自行 `fiber.state()` 判失败），值得在 loader 文档或 M2 清单中记录为后续任务（loader 上报 fiber 失败态 / HMR 回滚时区分「加载失败」与「组件运行失败」）。

### nit8. `PLAN.md` M2 验收准则「in-flight 任务不中断、其他组件状态保留」与 M2-PR1 的 L-Raise 内容关联性弱——「进行中」行的进度描述未对齐 M2 主验收口径
**位置**：`docs/PLAN.md:313`

**事实**：M2 行的验收准则列仍为「改插件代码保存即生效，in-flight 任务不中断、其他组件状态保留；回滚用例；走查 §5.2」（HMR 主目标），而「进行中」的进度描述填入的是 L-Raise 失败模型（处置⑤/⑧）。两者在语义上**不冲突**（L-Raise 是 M2 首批任务的失败模型前置，处置⑤/⑧ 确属 M2 清单），但读者只看进度行会误以为"进行中"的里程碑是 HMR 本身。属文档组织 nits，与 nit 段落风格一致：建议在 M2 行进度中显式标注「（首批任务：失败模型前置；HMR 主目标未开始）」，避免把"处置清单消项"与"里程碑主验收"混排。

### nit9. 测试强度：`failure_model.rs` 未覆盖「已完成步骤 → 依赖者级联停用」的失败路径（现有恢复断言只验证 `k0` 绑定清空 + `is_quiet`，无依赖者/子组件的级联正确定性断言）
**位置**：`crates/cordis-core/tests/failure_model.rs:69-89`

**事实**：`l_raise_records_error_outcome_and_recovers_completed_steps` 验证了三件事（错误 outcome ✓、已完成步骤恢复 ✓、静止 ✓），但它的失败组件 `FailAfterFirst` 无 `inject`（`KeySet::new()`），即失败时**没有依赖者**。L-Raise 路径的 `unload` 会执行 Thm 63 的「依赖者先撤」级联（`runtime.rs:410-411` `fiber.ctx.notify(&provided)`），但若失败 fiber 本身提供了键、下游有 consumer，失败时的级联停用正确性**未测**。「失败 → 依赖者随之停用」的语义（本 PR 注释 `runtime.rs:347` "依赖者随之停用"）无直证。属测试强度 nit，建议补「失败 provider + 活跃 consumer」场景，断言 provider 失败后 consumer 亦 Inactive 且 store 全清。

---

## ✅ 正面确认

1. **catch_unwind 范围判定正确**：`reload` 仅包 `execute(iter, guard)`（`runtime.rs:371`），`resolve_view`/`(fiber.apply)()`/`notify` 均在 catch 之外。`apply` 对 native 组件是"宿主代码"（panic = bug 直接传播，与 ADR 一致）；`apply` 对 wasm 组件经 `.expect("跨边界 start 调用")` 处理 `start` trap——`start`/`inject`/`provide` 属 load/静态导出，不在 §4.3.4 的"效应失败"（`e` 的迭代）范畴，此划界合理。
2. **已完成步骤恢复自洽**：`set`/`set_dyn` 经 `effect → PushingIter::next → push_step` 在**产出时**即入 `ctx.dispose` 累加器（`context.rs:427-449`），故 `execute` 本地 `acc` 因 unwind 丢失后，`unload → ctx.dispose_all()`（`runtime.rs:418`）仍能精确恢复已完成步骤；`execute` 组合逆与累加器共享 `StepGuard` 幂等句柄（`effect.rs:93-123`），双路径（`fiber.dispose` 的 `recover` vs `ctx` 累加器）不重复撤销——`failure_model.rs` 的 store 清空断言 + 全部 27 二进制全绿实证。
3. **is_quiet 的 ζ 析取精确**：`Inactive(Some(_)) => true`（ζ ≠ ⊥）+ `Inactive(None) => target.is_none()`（ζ = ⊥ ∧ target = ⊥）完整实现 Def 49 式 (45) 的 `ζ≠⊥ ∨ target=⊥`；`Inactive(None) ∧ target=Some`（本应 reload 却未转换）判 false，约束正确。
4. **载荷 downcast 与 panic 载荷类型匹配**：核心纪律 panic 带格式参数 → 载荷为 `String`（非 `&'static str`），wasm 桥接 `downcast_ref::<&'static str>()` 后 `.or_else(downcast_ref::<String>())` 双兜（`lib.rs:391-395`），`String` 分支命中；`FiberError` 经 `panic_any` + `downcast::<FiberError>()` 判别（`runtime.rs:376`），类型/`Send` 约束均满足。
5. **失败后可重试（L-Begin）语义一致**：失败路径 `target=None`（`runtime.rs:382`）后，提供者退役→重装触发 `notify_fibers → refresh → compute_target` 翻转 None→Some → `reload` 重试（`refresh` 的 `Inactive(_)` 非转换态、`target.is_some()` 触发 reload），`failure_model.rs` 的 `l_raise_failed_fiber_can_retry_activation` 直证通过。
6. **可信原生组件越界写仍 panic = bug**：`set_dyn` 的纪律 `panic!`（`context.rs:219`）**在 core 保留**，仅 wasm 桥接在 `forward_pending` 以 `catch_unwind` 捕获后转 `FiberError`；native 组件直接调 `set_dyn` 越界写仍 panic——边界记录（THEORY-MAP §6.3 行「可信原生组件的越界写仍为 panic = bug（宿主不变式，保留）」）**准确无误**。
7. **文档一致性**：THEORY-MAP 的 M0 两行（`is_quiet` ζ 析取、L-Raise 整条）置"已修复/已落地"、清单⑤/⑧ 置"已落地"、PR #17 行（L142）落地、PLAN M2 置"进行中"——与实现一致。测试计数（27 二进制全绿）实证无回归，hello-plugin/dual_backend/go_guest（2 passed，15.83s）等正常路径不受影响。
8. **卫生**：`cargo fmt --all -- --check` exit 0；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets` 干净；命名/注释（`FiberError::raise`、`catch_unwind` 语义注释、`runtime.rs:344-348` 的 L-Raise doc）清晰且与论文 §4.3.4/Def 49 逐条对应。
