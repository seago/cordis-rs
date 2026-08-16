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
| 满足谓词 `σ⊧d` | Def 24 | `Store::satisfies` / `Context::satisfies`（经 `ρ` 解析）/ `InterpState::satisfied` | 单元 | 完成（PR #2/#4）；**注意**（M0 走查）：这是**原始表**谓词（含未 Active 绑定）；Def 45 的**状态级** `γ⊧d = σγ⊧d`（仅 Active fiber 表并集）由 `Runtime::satisfied`（`provider_of` 派生）实现，是生命周期规则（`compute_target`）所用者 |
| `Γ∞`（统一上下文类型） | Def 32 | `cordis_core::context::Context`（`ρ` + `ι` + 累加器）+ `Runtime`（共享 `σ`） | 单元 | 完成（PR #3–4） |
| `𝔈Γ` / `𝔈iter_Γ`（效应函数/迭代器） | Def 8/51 | `cordis_core::effect::{Disposer, EffectIter, Step, once}` | 单元 | 完成（PR #3，同步版） |
| `effectΓ(𝑒)` / `ctx.effect` | Def 12, Alg 1 | `Context::effect` | 单元 | 完成（PR #3：armed + 组合入累加器） |
| `get` / `set`（共效应操作） | Def 23, Alg 2 | `Context::get` / `set`（`Store` 表操作，realm 键控） | 单元 | 完成（PR #4：可逆 + 双侧 notify） |
| `isolate` | Def 28/29 | `Context::isolate`（派生实现，Def 27） | 单元 | 完成（PR #4） |
| `intercept` | Def 30/31 | `Context::intercept` / `intercept_of`（`InterceptMeta` 右偏合并） | 单元 | 完成（PR #4，**仅元数据累积侧**；provider 函数求值形态未实现，M0 走查实现缺口——见已知偏差） |
| `notify`（分类通知） | Def 26, Alg 3 | `notify::classify` + `Context::notify` → `Runtime::notify_fibers`（fiber 反应器内置） | 单元 | 完成（PR #4 分类 + PR #5 fiber 反应器）；M0 走查注明：`classify` 为纯函数（矩阵测试）但**未接入运行时**——分类效果由 refresh 幂等隐式承担 |
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
| Cor 21：独立效应乱序撤销 | `context::tests::cor21_independent_effects_revert_in_any_permutation`（**穷举全部 4! = 24 种排列**，每步断言 Thm 20(1) 中间态） | 完成（PR #9 + M0 走查强化：不同键 `set` 满足 Def 19 独立性——变换可交换；clause(2) 因状态无关逆退化，见已知偏差） |
| Thm 63：依赖者先停、teardown 可读依赖 | `runtime::tests::withdrawal_cascade_disposes_dependents_first`（teardown 检查逆） | 真实引擎已验（PR #5）；停用**结果**经 property（活跃集一致）覆盖，teardown 可读的**顺序性**由集成测试直接验证（M0 走查：表述厘清；oracle 两态模型不携带转换内顺序） |
| Thm 64：单转换不跨两次解析 | `runtime::tests::target_change_mid_reload_chains_unload`（目标中途变化 → 惯性链卸载） | 真实引擎已验（PR #5）；M0 走查：括号"guard 步界中断"属 §4.3.2 效应层（`execute_interrupts_at_step_boundary`），Thm 64 为解析层惯性——两层级测试分开对应 |
| Thm 66：Progress、guard 不死锁 | `interp::tests::drive_*`（oracle 自检）+ `tests/property.rs`（每个动作后 `is_quiet` 断言） | oracle 已验（PR #2）；真实引擎已验（PR #6）；M0 走查：到达静止已验，定量上界 `(K+4)(V+1)` 未断言（记录为覆盖缺口） |
| Thm 73 / Cor 62：Confluence、离场无残留 | `interp::tests::confluence_all_interleavings`（穷举交错）+ `tests/property.rs`（oracle 对比：活跃集/σγ/绑定总数逐步一致）+ **`thm73_canonical_form_static_assembly`**（M0 走查补：动态历史 == 静态装配，up to names） | oracle 已验（PR #2）；真实引擎已验（PR #6）；**Thm 73(1) canonical form 补测（M0 走查）**；Cor 62 近似覆盖：绑定总数 == σγ 无泄漏已验，`≈` 的值维度因 Def 69 规范组件全用常量值 1 被平凡化（M0 走查注明） |
| Thm 59：Preservation（Def 58 四条款守卫不变式） | 无专门测试——Def 58(3)(4)（installed 的 `ω` 落于 registry 且 provider 已安装）无直接断言；供给不相交/π 前提/移除前置有片段覆盖（`provision_clash_rejected`、`unknown_parent_rejected`、`remove_preconditions`） | M0 走查：覆盖缺口（间接经 Thm 66/73 property 支撑；列入 M1 首批任务） |
| Thm 61：Recovery exactness（式 56，累加器恢复精确） | 由 §3.1 局部 LIFO 测试（Thm 7/16）间接支撑；多 fiber 交错的全局形态未直接验证 | M0 走查：覆盖缺口（时间组合性核心；列入 M1 首批任务） |
| Def 65 / Lemma 68 / Lemma 70：support 系、静止时 support = Active | `interp::tests`（`drive_reaches_quiet_and_lemma70`、`withdrawal_cascade`、`confluence_all_interleavings` 内 `active_set == support_set` 断言） | 已覆盖（PR #2/#6）；M0 走查补入覆盖表 |
| 元理论内部引理（Lemma 55/56/57、Lemma 71/72、Thm 54） | 无直接测试——≃/≈/重命名等价未在实现中显式建模，属证明内部引理（非可观察性质） | M0 走查：注明"证明内部、无直接测试" |
| Def 26：通知分类正确性 | `notify::tests::classification_matrix`（8 组合分类矩阵） | 完成（PR #4） |

## 已知偏差

> 每 PR 合入时追加；里程碑走查逐条处置：**修正 / ADR 保留 / 公开差异声明**。

| 日期 / PR | 偏差描述 | 论文依据 | 处置 | 状态 |
|---|---|---|---|---|
| 2026-08-15 / PR #2 | 参考解释器把抽象效应函数 `e` 规范化建模为「激活恰好安装 `provide` 全键、停用清空」 | Def 43/69（论文在 Def 69 假设下使用相同模型） | 公开差异声明（oracle 选择，性质保持） | 记录 |
| 2026-08-15 / PR #2 | `step_lifecycle` 固定按 fiber id 升序取首个可启用规则；论文的规则不规定调度 | §4.2（规则对任何序列成立） | 公开差异声明（oracle 确定性需要） | 记录 |
| 2026-08-15 / PR #3 | 效应迭代器为同步版；论文 Algorithm 1 的 `await iter.next()` 由 PR #5 接入 tokio 时提供（引擎逻辑不变） | Def 51, Alg 1 | 公开差异声明（阶段实现选择） | 记录 |
| 2026-08-15 / PR #3 | **M0 走查更正**：`ctx.dispose ← dispose ∘ ctx.dispose` 在**注册期**执行——论文 Alg 1 第 17 行同样位于 `effect` 函数体内（注册期组合），原行"论文伪代码置于 dispose 内部"表述失准；且累加器入栈已细化为**每步逆产出时**（M-A 行，应用序 LIFO） | Alg 1 第 17 行 | 公开差异声明（表述更正：与论文一致） | 记录 |
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
| 2026-08-15 / PR #4 审查 | 分类衔接留白（m1）：`classify` 需前后快照而 `notify` 只广播键；**M0 走查更新**：PR #5 后引擎以 `refresh` 幂等实现分类效果（activating → target 变化 → reload；deactivating → target 变化 → unload；neutral → target 不变 → 无操作），`classify` 仍为独立验证的纯函数、系统内无生产调用点——原"PR #5 提供快照/变更日志"的预期未落地（refresh 幂等替代） | Def 26, Alg 3 | 公开差异声明（引擎经 refresh 幂等隐式分类） | 记录 |
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
| 2026-08-16 / PR #9 | Cor 21 落地：`cor21_independent_effects_revert_in_any_permutation`——不同键 `set` 作为两两独立效应族（Def 19 clause(1)：变换只动自己的键→可交换），**穷举全部 4! = 24 种排列**（LIFO/正序/乱序）撤销均回到 γ₀，每步断言 Thm 20(1) 中间态（只撤自己的贡献）；§3.1 效应论收尾。**M0 走查更正**：Def 19 clause(2)（逆不被外来变换干扰）因测试用状态无关逆（`unbind` 捕获 realm，不读当前状态）**退化未测**——原"两 clause 均覆盖"表述与事实不符 | Cor 21, Thm 20, Def 19 | 完成（穷举 24 排列）；clause(2) 退化声明见走查行 | 记录 |
| 2026-08-16 / M0 走查 | **实现缺口（公开差异）**：Def 30/31 的 interception 完整形态（Σinter = 每键 provider 函数 `σ(k): ℳ→𝒱`；`get(k,μ) = σ(k)(μ⊕ₖι(k))`）未实现——当前 `intercept` 仅累积元数据到 `ι` 表（§5.1.2 的实现描述部分）、`get` 直读 store 从不咨询 `ι`，无 provider 函数概念与生产消费点；§5.1.2 的"@@intercept is consulted only when a binding is accessed"无对应。**补充（审计 F2）**：`get` 签名无 `μ` 参数（`Context::get<K: Key>()`，component-declared metadata 概念整体缺席）；论文自身"右偏"表述存在张力——Def 31 get 侧为 `ι(k)` 覆盖组件 `μ`，而 intercept 操作式 `ι[k↦ι(k)⊕ν]` 为 `ν`（新元数据）优先（§5.1.2 文字）；实现采纳 intercept 操作侧语义（new 优先），get 侧无实现 | Def 30/31, §5.1.2 | 公开差异声明（实现缺口；拦截元数据累积与右偏合并已实现；求值形态列入 M1 首批任务） | 记录 |
| 2026-08-16 / M0 走查 | **实现缺口（公开差异）**：§5.1.4 Algorithm 6 的 Proxy 访问层整节未实现——沿 fiber 链解析（committed/inject/root）的 `ctx[key]` 属性访问、规格强制（`INACTIVE_ACCESS`/`UNDECLARED_ACCESS`）无对应物；`ctx.get` 为裸 store 查找（与论文一致地"从不失败"），但"读视图而非 store 的规格强制访问"缺席 | §5.1.4, Alg 6 | 公开差异声明（实现缺口；DX 层后续，列入 M1 首批任务） | 记录 |
| 2026-08-16 / M0 走查 | `Disposer = Box<dyn FnOnce>` 为命令式载体，替代论文 `g: Γ→Γ` 纯函数——变换幺半群 `𝔐(e)`（Def 17–19 的独立性形式定义）在代码中无可表达结构，独立性退化为一阶约定（"逆只撤自己的键"）；语义保证（Thm 7/16/20/21）经命令式测试成立 | Def 8/17–19 | 公开差异声明（结构性；wasm 边界句柄化时按论文形态对齐） | 记录 |
| 2026-08-16 / M0 走查 | `Step` 无「无逆步骤」变体（Alg 1 的 `if value` 无对应）；实际对齐 Def 51 三元组（`g` 恒存在）更贴切——`Yielded`/`Finished` 恒携带逆 | Def 51, Alg 1 | 实现说明（与 Def 51 一致，Alg 1 的 `Maybe` 分支为一般化） | 记录 |
| 2026-08-16 / M0 走查 | Thm 7（单组件累加器恢复）无独立测试——与 Thm 16 合并声明（`accumulator_reverts_all_effects_lifo` 兼作两者）；声音不变量在终态断言 | Thm 7/16 | 实现说明（合并覆盖声明） | 记录 |
| 2026-08-16 / M0 走查 | Def 33–42 观测等价 ≃ 无对应物——`InterpState` 的 `PartialEq` 是结构相等而非商结构；≃ 为理论构造（导出独立性），非运行时实体 | Def 33–42 | 公开差异声明（理论构造不落运行时） | 记录 |
| 2026-08-16 / M0 走查 | O-Insert 的 `π ∈ dom(Fγ) ∪ {root}` 前提未在引擎侧执行——`RegistryError::UnknownParent` 仅由 property harness 合成，`register` 不校验父存在性（依赖 `HasChildren` 移除前提维持父存活不变式） | §4.2 O-Insert | 公开差异声明（防御深度缺口；不变式由移除前提维系） | 记录 |
| 2026-08-16 / M0 走查 | `is_quiet` 的 `Inactive(_)` 分支只判 `target.is_none()`，未实现 Def 49 式 (45) 的 `ζ≠⊥ ∨ target=⊥` 析取——当前 ζ 恒 None（L-Raise 未实现）掩盖；**L-Raise 落地时必改** | Def 49 式 (45), §4.3.4 | 记录（L-Raise 必改项） | 记录 |
| 2026-08-16 / M0 走查 | **实现缺口（公开差异）**：Table 2 的 **L-Raise 整条未落地**——`FiberError` 无生产者、`FiberState` 的 `outcome`/ζ 恒 None、组件 apply panic 直接传播（panic = bug）而无「error recorded + target=⊥」回记；§4.3.4 失败模型（`𝔈fail`、`Either(Ξ,…)`）整体留待 async 阶段 | §4.3.4, Table 2 | 公开差异声明（实现缺口；随 async 化落地，届时同步修正 `is_quiet` 的 ζ 分支） | 记录 |
| 2026-08-16 / M0 走查 | Def 47 注册：引擎在 `register` 时**立即非可逆地 O-Insert**，逆仅含 refresh→retire；论文把 O-Insert 建模为可逆效应本身（逆 = O-Retire）——可观察等价（retire 恒被累加器持有），oracle 侧 `Fiber.registered` 已记录，引擎侧结构差异补记 | Def 47 | 实现说明（可观察等价） | 记录 |
| 2026-08-16 / M0 走查 | Def 48 纪律 clause(2)（读仅限声明依赖）无运行时执行——越界**写**有 panic 检查（已记录），读任意 realm 无 confine 检查（论文为组件义务，非证明前置） | Def 48 | 公开差异声明（义务不检查） | 记录 |
| 2026-08-16 / M0 走查 | `Fiber::target` 存完整 `Option<View>` 而非 §4.2 所述 hash（论文：`fiber.committed` 存 map、`fiber.target` 存 hash）——语义等价（hash 为优化），§5.1.3 实现细节 | §4.2, §5.1.3 | 实现说明（优化取舍） | 记录 |
| 2026-08-16 / M0 走查 | §5.1.2 isolate 默认 realm 生成未实现——`Context::isolate` 要求显式 realm 参数（Def 29 本体一致；"freshly generated symbol by default"为 §5.1.2 描述性便利） | §5.1.2 | 公开差异声明（显式 realm 为 API 设计） | 记录 |
| 2026-08-16 / M0 走查 | Alg 3 的 `return affected` + Alg 5 的 `await all(...)` 显式等待语义无对应——同步核心以递归级联隐式完成（notify 返回 `()`，转换同步跑完）——语义等价，机制不同 | Alg 3/5 | 公开差异声明（同步适配） | 记录 |
| 2026-08-16 / M0 走查 | Thm 59（Preservation 四条款守卫不变式）与 Thm 61（Recovery exactness 全局交错）无直接测试——经 Thm 66/73 property 与 §3.1 局部 LIFO 测试间接覆盖 | Thm 59/61 | 记录（覆盖缺口；列入 M1 首批任务） | 记录 |
| 2026-08-16 / M0 走查 | **修复（F4）**：Thm 73(1) canonical form 补测 `thm73_canonical_form_static_assembly`——动态历史（乱序注册+退役+移除+重装）与静态装配（按 ⊲ 序一次性装入）静止态按 (inject, provide) 签名比较一致（up to names）——"动态历史无痕迹 = 静态装配"招牌承诺落地 | Thm 73(1), §4.4.5, Lemma 56 | 修正（含测试） | 已修复 |
| 2026-08-16 / M0 走查 | **修复（PR #9 审查 nit1）**：Cor 21 测试由 4 种代表排列强化为**穷举全部 24 种排列**（字典序迭代器） | Cor 21 | 修正（测试强化） | 已修复 |
| 2026-08-16 / M0 审查 | **修复（REVIEW-M0 风险提示）**：`Context::intercept`/`intercept_of` 为公开 API 但语义半成品（`get` 不消费 `ι`）——rustdoc 补**半成品警示**（"读路径消费由 M1 落地，此前按'拦截已生效'使用将静默无效果"） | Def 30/31 | 修正（rustdoc 警示） | 已修复 |
| 2026-08-16 / PR #10 | Wasm 后端起步（M1）：wit 世界 v1（`cordis-wasm/wit/cordis.wit`：import context（get/set + `inverse` 资源句柄化）+ export plugin（`component` 资源 = Def 43 的 (d,p,e)、`task` 资源 = Def 51 𝔈iter 跨边界））；工具链定型——guest 以 **wasm32-wasip2** target 编译（rustc 直接产出组件二进制）+ **no_std + alloc**（能力面 = 仅 context 接口，论文 §6.3 import 面即能力面）；宿主 `Host` 实现 context/inverse/WasiView（wasip2 标准库引用 WASI p2，经 `wasmtime_wasi::p2` 提供）；端到端测试 `tests/load_guest.rs`（constructor → inject/provide 核对 → start/step 激活绑定 → 逆 run 撤销）；guest 示例 `examples/wasm-plugin-rust`（db 提供者）为独立 crate（wit-bindgen ABI 胶水用 unsafe，与 workspace `unsafe_code=deny` 冲突） | Def 8/43/51, §6.3, Alg 4/5 | 完成（PR #10：加载/驱动原语闭环；逆句柄表） | 记录 |

## 里程碑走查记录

| 里程碑 | 日期 | 覆盖章节 | 结论 | 未决偏差 |
|---|---|---|---|---|
| M0 原生闭环 | 2026-08-16 | §3.1–3.3、§4.1–4.4、§5.1（§5.1.1–5.1.3 主体；§5.1.4 见未决） | **门禁判定：通过（含处置清单）**——核心演算语义逐规则一致：可逆效应（execute/LIFO/armed/累加器）、共效应（满足谓词/三类分类/realm 键控/isolate 派生/notify 快照）、fiber 演算（组件三元组/七元组/registry 派生量/target/静止/O-Insert-Retire-Remove/L-Reload-Unload/L-Leave/惯性/步界中断）均有测试闭环（Thm 7/16/21/63/64/66/73/Cor 62 + oracle×引擎 2000 用例 + Cor 21 穷举 24 排列）；**本次走查修正**：Thm 73(1) canonical form 补测、Cor 21 全排列强化、3 处表述失准更正（Alg 1 第 17 行 dispose 组合时机、classify 衔接留白、Thm 63/64 表述）、映射表补 `Runtime::satisfied`（σ⊧d 与 γ⊧d=σγ⊧d 语义区分） | 处置清单（下里程碑首批任务）：① interception provider 函数形态（Def 30/31，实现缺口）② §5.1.4 Proxy 访问层（Alg 6，实现缺口）③ Thm 59/61 直接测试 ④ Thm 66 定量上界 `(K+4)(V+1)` 断言 ⑤ L-Raise 落地时 `is_quiet` 补 ζ 析取 ⑥ 命令式 Disposer 结构（wasm 句柄化时按论文形态对齐） |
