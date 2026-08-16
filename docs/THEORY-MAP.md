# THEORY-MAP：论文符号 ↔ 代码映射与偏差记录

> 活文档。每个 PR 合入时回填映射、记录偏差；里程碑走查时逐条处置（PLAN §7）。
> 论文：`paper/paper.pdf` — *A Programming Paradigm for Spatiotemporal Composability*（Shi / Zhang / Cui，2026）

## 符号 ↔ 代码映射

| 论文符号 / 术语 | 章节 | 代码（crate::path） | 测试 | 备注 |
|---|---|---|---|---|
| `k: K`（键符号） | Def 22 | `cordis_core::symbol::Symbol`（全局驻留） | 单元 | 完成（PR #2，审查后补测） |
| `𝒱 k`（值类型族） | Def 24 | `cordis_core::key::Key`（`type Value` + `const SYMBOL`） | —（经 `Store` 间接覆盖） | 完成（PR #2） |
| `𝔇Σ` / `𝔓Γ`（键集合） | Def 25/43 | `cordis_core::keyset::KeySet` | 单元 | 完成（PR #2） |
| `Σ`（依赖表） | Def 22 | `cordis_core::store::Store` | 单元 | 完成（PR #2）；`get`/`set`/撤销带 Def 23 前置条件 |
| 满足谓词 `σ⊧d` | Def 24 | `Store::satisfies` / `Context::satisfies`（经 `ρ` 解析）/ `InterpState::satisfied` | 单元 | 完成（PR #2/#4） |
| `Γ∞`（统一上下文类型） | Def 32 | `cordis_core::context::Context`（`ρ` + `ι` + 累加器）+ `Runtime`（共享 `σ`） | 单元 | 完成（PR #3–4） |
| `𝔈Γ` / `𝔈iter_Γ`（效应函数/迭代器） | Def 8/51 | `cordis_core::effect::{Disposer, EffectIter, Step, once}` | 单元 | 完成（PR #3，同步版） |
| `effectΓ(𝑒)` / `ctx.effect` | Def 12, Alg 1 | `Context::effect` | 单元 | 完成（PR #3：armed + 组合入累加器） |
| `get` / `set`（共效应操作） | Def 23, Alg 2 | `Context::get` / `set`（`Store` 表操作，realm 键控） | 单元 | 完成（PR #4：可逆 + 双侧 notify） |
| `isolate` | Def 28/29 | `Context::isolate`（派生实现，Def 27） | 单元 | 完成（PR #4） |
| `intercept` | Def 30/31 | `Context::intercept` / `intercept_of`（`InterceptMeta` 右偏合并） | 单元 | 完成（PR #4） |
| `notify`（分类通知） | Def 26, Alg 3 | `notify::classify` + `Context::notify` → `Runtime::notify_fibers`（fiber 反应器内置） | 单元 | 完成（PR #4 分类 + PR #5 fiber 反应器） |
| 组件 `(d, p, e)` | Def 43 | `cordis_core::component::Component` + `cordis_macro::component`（声明式 DX） | 单元 + 示例 | 完成（PR #5 引擎 / PR #7 DX 层） |
| fiber `⟨d, p, e, π, σ, τ, θ⟩` | Def 44 | `cordis_core::fiber::Fiber`（`σ` 由绑定 provider + ctx 累加器隐含） | 单元 | 完成（PR #5） |
| `n: 𝔑`（fiber 名） | Def 44/45 | `cordis_core::fiber::FiberId` | 单元 | 完成（PR #2/#5） |
| `dom(𝐹𝛾)`（registry） | Def 45 | `runtime::Runtime::fibers`（`Runtime::fiber`/`len`） | 单元 | 生产版完成（PR #5）；oracle 版仍为 `interp` |
| `target_n(γ)` / 静止判定 | Def 46 | `Runtime::compute_target` / `is_quiet` / `Fiber::target` | 单元 | 完成（PR #5）；oracle 版仍为 `interp` |
| `ΘΓ`（生命周期状态） | Def 49 | `fiber::FiberState`（四状态，同步适配） | 单元 | 完成（PR #5） |
| `σγ` / `provider_k(γ)` | Def 45 式 (40) | `Runtime::provider_of` / `provided_of`（绑定携带 provider，仅 Active 计入） | 单元 | 完成（PR #5） |
| `O-Insert` / `O-Retire` / `O-Remove` | §4.2 | `Context::use_component`（`Runtime::register`）/ `Fiber::retire` / `Runtime::remove_fiber` | 单元 | 生产版完成（PR #5） |
| `L-Reload` / `L-Unload` | §4.2 | `Runtime::reload` / `unload`（含 L-Leave 标记） | 单元 | 生产版完成（PR #5，同步版） |
| `recover` / accumulator `g` | Def 6, Alg 1 | `effect::execute`（LIFO 折叠）/ `Context::dispose_all` / `StepGuard` / `Fiber::dispose` | 单元 | 完成（PR #3/#5） |
| `relied_n(γ)`（撤离 guard） | Def 50 | 同步级联天然保证（依赖者先撤，Thm 63 测试）；显式 guard 随 async 化实现 | 单元 | 部分完成（PR #5 语义等价；显式化 PR #6） |
| `use`（组件实例化） | Alg 4 | `Context::use_component`（`Runtime::register`，Def 47 注册回调效应） | 单元 | 完成（PR #5） |
| `refresh` / `reload` / `unload` | Alg 5 | `Runtime::{refresh, reload, unload}`（惯性状态机） | 单元 | 完成（PR #5，同步版） |
| 配置 Entry | Def 74 | `cordis_loader::{Entry, Loader}`（`register_component`/`apply`/`fiber`，§5.2.1 增量协调） | 单元 | 完成（PR #8 最小版：`id`/`config`（经 `revision`）/`disabled`/组件名） |

## 定理覆盖

| 定理 / 结论 | 测试位置 | 状态 |
|---|---|---|
| Thm 7 / Thm 16：LIFO 恢复、声音不变量 | `effect::tests` / `context::tests`（`execute_runs_inverses_in_lifo`、`thm16_*`、`accumulator_reverts_all_effects_lifo`、**`nested_effect_reverts_in_application_order`**） | 完成（PR #3；嵌套顺序审查后修复） |
| Cor 21：独立效应乱序撤销 | | 未开始（PR #3 后续，§3.3.2 就绪后） |
| Thm 63：依赖者先停、teardown 可读依赖 | `runtime::tests::withdrawal_cascade_disposes_dependents_first`（teardown 检查逆） | 真实引擎已验（PR #5）；property 化 PR #6 |
| Thm 64：单转换不跨两次解析 | `runtime::tests::target_change_mid_reload_chains_unload`（guard 步界中断 + 惯性链） | 真实引擎已验（PR #5） |
| Thm 66：Progress、guard 不死锁 | `interp::tests::drive_*`（oracle 自检）+ `tests/property.rs`（每个动作后 `is_quiet` 断言） | oracle 已验（PR #2）；真实引擎已验（PR #6） |
| Thm 73 / Cor 62：Confluence、离场无残留 | `interp::tests::confluence_all_interleavings`（穷举交错）+ `tests/property.rs`（oracle 对比：活跃集/σγ/绑定总数逐步一致） | oracle 已验（PR #2）；真实引擎已验（PR #6） |
| Def 26：通知分类正确性 | `notify::tests::classification_matrix`（8 组合分类矩阵） | 完成（PR #4） |

## 已知偏差

> 每 PR 合入时追加；里程碑走查逐条处置：**修正 / ADR 保留 / 公开差异声明**。

| 日期 / PR | 偏差描述 | 论文依据 | 处置 | 状态 |
|---|---|---|---|---|
| 2026-08-15 / PR #2 | 参考解释器把抽象效应函数 `e` 规范化建模为「激活恰好安装 `provide` 全键、停用清空」 | Def 43/69（论文在 Def 69 假设下使用相同模型） | 公开差异声明（oracle 选择，性质保持） | 记录 |
| 2026-08-15 / PR #2 | `step_lifecycle` 固定按 fiber id 升序取首个可启用规则；论文的规则不规定调度 | §4.2（规则对任何序列成立） | 公开差异声明（oracle 确定性需要） | 记录 |
| 2026-08-15 / PR #3 | 效应迭代器为同步版；论文 Algorithm 1 的 `await iter.next()` 由 PR #5 接入 tokio 时提供（引擎逻辑不变） | Def 51, Alg 1 | 公开差异声明（阶段实现选择） | 记录 |
| 2026-08-15 / PR #3 | `ctx.dispose ← dispose ∘ ctx.dispose` 于注册时执行；论文伪代码置于 dispose 内部（armed 幂等保证可观察等价） | Alg 1 第 17 行 | 公开差异声明（实现选择） | 记录 |
| 2026-08-15 / PR #2 审查 | `Symbol` 的 `Ord`/`Hash` 为进程内分配序：跨进程不可比较、迭代序跨运行不保证；跨边界（wasm）以名称字符串为媒介，不使用 id | Def 22（键为原子） | 公开差异声明（文档已修正：进程内确定性） | 记录 |
| 2026-08-15 / PR #2 审查 | O-Insert 的供给不相交检查覆盖 `dom(Fγ)` 全部 fiber（含已退役未移除者）：退役组件的供给名在 remove 前保持占用 | §4.2 O-Insert 前提 `∀m ∈ dom(Fγ)`（与论文一致，无偏差；补充说明） | 无偏差（注释 + 记录） | 记录 |
| 2026-08-15 / PR #3 审查 | **修复（M-A）**：原实现按「效应级注册完成时」入栈累加器，嵌套效应（外层迭代步骤间注册的内层效应）撤销顺序错误（外层整组先撤）；改为**每步逆产出时入栈**（应用序 LIFO，嵌套正确交错），与论文 "prepending each new inverse therefore yields LIFO recovery" 及 track 模型（`φ ∘ g`）一致 | Alg 1 前导句、Def 3、Thm 16 | 修正（含嵌套回归测试） | 已修复 |
| 2026-08-15 / PR #3 审查 | **约束（M-B）**：同步核心要求效应迭代器有限终止（论文效应序列有限，Def 51 的 `Maybe(ℑ)`）；无限/订阅型效应由 PR #5 async 支持 | Def 51 | 公开差异声明（阶段限制，文档已明示） | 记录 |
| 2026-08-15 / PR #3 审查 | armed 标志当前仅作 execute 的 guard 输入（同步核心中恒真）；「dispose 中断在途迭代」在 PR #5 async 时代实现；撤销幂等由每步 `StepGuard` 保证 | Alg 1 第 10–16 行 | 公开差异声明（阶段实现选择） | 记录 |
| 2026-08-15 / PR #3 审查 | panic 策略：panic = bug（单线程宿主，无 unwind 保护；单步逆 panic 中止剩余撤销） | — | 记录（模块文档已明示） | 记录 |
| 2026-08-15 / PR #4 | notify 的 fiber 遍历推迟到 PR #5（registry 未建立）；当前 `Context::notify` 仅保留反应器机制（`Runtime::on_notify`），无反应器时为空操作 | Alg 3（依赖 Def 45 的 registry） | 公开差异声明（阶段实现选择） | 记录 |
| 2026-08-15 / PR #4 | 拦截元数据读取 API（`intercept_of`）要求 `M: Clone`（对象安全的 `clone_box` 之外的最小约束）；合并始终右偏（`new` 优先，§5.1.2） | Def 30/31 | 实现说明 | 记录 |
| 2026-08-15 / PR #4 | `Store` 错误携带 realm 而非用户键（`AlreadyBound(realm)` 等）——realm 与键在未隔离时相同，隔离场景以 realm 为准（Def 29 前置条件沿 `ρ` 转译） | Def 23/29 | 实现说明 | 记录 |
| 2026-08-15 / PR #4 审查 | **修复（M1）**：notify 改为**快照迭代**（先克隆反应器列表）——反应器内注册新反应器不再 RefCell panic（原实现 borrow 跨循环存活）；反应器内同步 set 的递归广播语义已文档化并加守卫测试（PR #5 以 refresh 惯性切断） | Alg 3 | 修正（含 2 个重入测试） | 已修复 |
| 2026-08-15 / PR #4 审查 | **修复（m4）**：`InterceptMeta` 移除 `Send + Sync` 约束（单线程 `Rc` 宿主，ADR-0002，约束无实际需要） | — | 修正 | 已修复 |
| 2026-08-15 / PR #4 审查 | 分类衔接留白（m1）：`classify` 需前后快照而 `notify` 只广播键；快照/变更日志机制由 PR #5 提供（届时确定 `notify` 携带 `prev` 的形态）——已文档化 | Def 26, Alg 3 | 公开差异声明（PR #5 设计点） | 记录 |
| 2026-08-15 / PR #4 审查 | 错误载体区分（m2）：`Context::set` 报用户键、`Store::bind` 报 realm——已文档化 | Def 23/29 | 实现说明 | 记录 |
| 2026-08-15 / PR #4 审查 | **PR #5 async 化必改项（m3）**：`set` 前置检查与绑定之间的 TOCTOU 窗口（同步单线程下不可达）；async 化后 `expect` 须改为可传播错误——已在代码注释标注 | Def 23, Alg 2 | 记录（PR #5 必改项） | 记录 |
| 2026-08-15 / PR #4 审查 | 反应器注册表只增不减（m5）：fiber 卸载路径需要移除句柄；PR #5 提供 `ReactorId` 或改注册表——已文档化 | Alg 5 第 26 行 | 记录（PR #5 设计点） | 记录 |
| 2026-08-15 / PR #4 审查 | `Reactor` 类型别名补导出（nit2） | — | 修正 | 已修复 |
| 2026-08-15 / PR #5 | 同步核心的 `Reloading`/`Unloading` 状态只携带 `ω`：`i`（剩余迭代器）活在转换调用栈上，`g`（累加器）由 ctx 累加器承载（Table 2 的 `fiber.dispose`）；async 化时 `i` 移入状态 | Def 49 | 公开差异声明（同步适配） | 记录 |
| 2026-08-15 / PR #5 | 根上下文（fiber = None）`set` 的绑定不参与 `σγ`（Def 45 仅 Active fiber 的 `σ` 并集）——编排器级的全局提供需经组件/fiber 完成 | Def 45 | 公开差异声明（与论文一致，补充说明） | 记录 |
| 2026-08-15 / PR #5 | `relied_n` guard（Def 50）：同步级联天然实现「依赖者先撤、提供者绑定保持到依赖者停用」（Thm 63 测试验证）；显式 guard 随 async 化实现 | Def 50 | 公开差异声明（同步语义等价） | 记录 |
| 2026-08-15 / PR #5 | 执行期检查 Def 43/48 纪律（组件 `set` 越界写入未声明供给 → panic）——论文中为组件义务，实现升级为运行时检查（panic = bug） | Def 43/48 | 实现说明（强化检查） | 记录 |
| 2026-08-16 / PR #5 审查 | **修复（M1）**：notify 载荷统一为 **realm**（`set` 原传用户键，与 `reload`/`unload` 的 realm 载荷不一致）+ `notify_fibers` 改按 realm 语义匹配（`f.ctx.resolve_realm(inject_key) == payload_realm`）——修复隔离场景（realm ≠ key）下依赖者收不到激活/停用通知的级联断裂；补 2 个交叉测试（同 realm 级联 / 跨 realm 负例） | Alg 3, Def 28/29 | 修正（含隔离×fiber 交叉测试） | 已修复 |
| 2026-08-16 / PR #5 审查 | 载荷语义文档化（m1）：`Reactor`/`notify` 文档明示 keys 为已解析 realm | Alg 3 | 实现说明 | 已修复 |
| 2026-08-16 / PR #5 审查 | 幽灵 fiber 文档化（m2）：`remove_fiber` 仅移除 registry 条目，fiber 对象仍被父 ctx 注册回调持有至父 `dispose_all`——预期语义已注明 | Def 47 | 实现说明 | 记录 |
| 2026-08-16 / PR #5 审查 | 级联栈深度边界（m3）：依赖链深度 N → N 层嵌套调用栈（同步核心已知边界），async 化自然缓解——runtime 模块文档已注明 | §4.3 | 实现说明 | 记录 |
| 2026-08-16 / PR #5 审查 | `Fiber::state()` 借用警告（m4）：持 Ref 期间调 `retire` 会 RefCell panic——doc 已注明（与 `store()` 同纪律） | — | 实现说明 | 记录 |
| 2026-08-16 / PR #6 | 元理论 property suites 落地：`tests/property.rs` 以 oracle（Def 69 规范组件 + 随机编排 ≤12 步，`proptest_config` **固定 2000 用例**）对比真实引擎——Thm 66（动作后必静止）、Thm 73（活跃集/σγ 逐步一致）、Cor 62（绑定总数 == σγ，无残留）；动作错误一致性（供给冲突/未知 fiber/移除前提同侧报错）亦断言 | §4.4 | 完成（oracle × 引擎闭环） | 记录 |
| 2026-08-16 / PR #6 | `Runtime` 补充公开只读 API（`active_fibers`/`provided`/`store`）供 oracle 对比与监控——无语义变更 | — | 实现说明 | 记录 |
| 2026-08-16 / PR #6 审查 | **修复（m1）**：验证强度与文档一致——`proptest_config` 固定 2000 用例（原默认 256，文档声称 2000 不符） | — | 修正（随仓库固化） | 已修复 |
| 2026-08-16 / PR #6 审查 | **修复（m2）**：动作空间补 parent 维度——oracle 建模 **Def 47 注册**（`Fiber.registered`：父卸载时注册子代被 O-Retire，interp `unload` 扩展 + 测试重构）；引擎侧经 `Fiber::ctx()` 在父 fiber 的 ctx 上实例化；`RegistryError` 补 `UnknownParent`（O-Insert π 前提）——`HasChildren` 移除前提与父级联退役进入随机覆盖 | Def 47, §4.2 | 修正（oracle 建模扩展 + harness parent 维度） | 已修复 |
| 2026-08-16 / PR #6 审查 | `Fiber::ctx` 公开访问器（`fiber.ctx`，Algorithm 4 第 8 行的文档化实体）——harness 父级实例化所需 | Def 44 | 实现说明 | 已修复 |
| 2026-08-16 / PR #7 | DX 层落地：`cordis-macro` 的 `#[component(inject=[..], provide=[..])]`（生成 `inject`/`provide`，`apply` 委托 `apply_impl`）；`cordis` 门面 re-export 全部 API + 宏；`cordis-native` 提供 `with_ctx` 单步效应辅助 | Def 43 | 完成（宏生成代码引用 `::cordis::` 路径，依赖门面） | 记录 |
| 2026-08-16 / PR #7 | M0 验收示例 `examples/hello-plugin`：server（提供）+ auth（注入），激活顺序 → 退役级联 → 移除后重连（auth 自动重连新 server）——全部断言通过 | §3.2.2, Thm 63 | 完成（端到端验证） | 记录 |
| 2026-08-16 / PR #7 审查 | **修复（m1）**：门面 re-export 改 `pub use cordis_core::*`（glob）——`execute` 等逐一漏列问题不再复发（原列表缺 `execute`） | — | 修正（glob 全量导出） | 已修复 |
| 2026-08-16 / PR #7 审查 | **修复（m2）**：CI 增加 `cargo run --quiet -p hello-plugin`——M0 验收断言纳入门禁（`cargo test` 只编译 bin 不运行） | — | 修正（CI 门禁） | 已修复 |
| 2026-08-16 / PR #7 审查 | 宏重复参数覆盖语义文档化（nit2：后者覆盖前者） | — | 实现说明 | 记录 |
| 2026-08-16 / PR #8 | loader 最小协调落地：`Entry`（id/component/config/revision/disabled）+ `Loader`（`register_component`/`apply`/`fiber`），§5.2.1 增量协调（新增实例化、消失卸载、`disabled` 切换、`component`/`revision` 变更重建，未变条目幂等）——覆盖 Def 74 三字段 + 组件名（`url` 原生版）；`isolate`/`intercept` 注解、嵌套 group/include、托管 realm（Algorithm 7）留 M2 | Def 74, §5.2.1 | 公开差异声明（最小范围，M2 补齐） | 记录 |
| 2026-08-16 / PR #8 | 组件级 config diff 由调用方递增 `Entry::revision` 承担（配置值 `Rc<dyn Any>` 不可比较）——论文将 config 变更交由组件自决，协调键承担 id/url 层 diff；此处以协调键代行 | §5.2.1 | 公开差异声明（最小版取舍） | 记录 |
| 2026-08-16 / PR #8 | 条目全部实例化于 root 上下文（根级、无子代）——`remove_fiber` 的 `HasChildren` 前提不受影响；嵌套实例化随 group/include（M2）落地 | Def 74, §4.2 | 公开差异声明（最小版取舍） | 记录 |
| 2026-08-16 / PR #8 | 配置错误（未注册组件名 / 供给冲突）→ panic（panic = bug，与核心同策略）；幂等性以「不重建则 fiber id 不变」断言覆盖 | — | 实现说明 | 记录 |
| 2026-08-16 / PR #8 | `Context::use_component` / `Runtime::register` 的 `config` 参数由 `Box<dyn Any>` 改为 `Rc<dyn Any>`——编排方（loader）重建条目需保留并复用配置（`Box` 不可克隆）；调用点随迁（native/示例/测试） | Def 47 | 实现说明（API 调整） | 记录 |
| 2026-08-16 / PR #8 审查 | **修复（m1）**：`apply` 改**两阶段**——先卸载侧（移除消失条目、卸载 `disabled` 置位/需重建条目的旧 fiber，释放供给名）再实例化侧——同供给键替换（desired 用 Y 替换 X）可单次 `apply` 完成，否则 Y 实例化命中 X 的供给检查而 `ProvisionClash`；补回归测试 `same_supply_replacement_in_single_apply` | §5.2.1 per-field dispatch | 修正（含回归测试） | 已修复 |
| 2026-08-16 / PR #8 审查 | **修复（m2）**：`HasChildren` panic 消息如实化（"条目下存在子代 fiber……叶子约束"）+ 模块文档注明**叶子约束**（不得经 `Loader::fiber(id)?.ctx()` 实例化子组件，嵌套随 group/include 在 M2 落地） | §4.2（`remove_fiber` 前提） | 修正（消息 + 文档） | 已修复 |
| 2026-08-16 / PR #8 审查 | **修复（m3）**：补 `disabled_period_changes_take_effect_on_reenable`——disabled 期间 component/revision 变更不落记录（保持旧值），enabled 后以新 entry 实例化（最终一致）——固化路径防未来重构破坏 | §5.2.1 | 修正（测试固化） | 已修复 |
| 2026-08-16 / PR #8 审查 | `desired` 重复 id：last-wins（后项覆盖，可能浪费一次实例化）——已文档化，调用方应保证唯一 | — | 实现说明 | 记录 |

## 里程碑走查记录

| 里程碑 | 日期 | 覆盖章节 | 结论 | 未决偏差 |
|---|---|---|---|---|
