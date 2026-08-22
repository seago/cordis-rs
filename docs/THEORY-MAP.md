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
| `Fiber::update` / `update_fiber`（§5.2.1 双向绑定组件侧） | Alg 5（reload/unload 的**强制实例**）, §5.2.1 | `Fiber::update`（换 config 闭包 → unload 逆转当前效应 → 目标未变链式 reload，fiber 身份保留）/ `Runtime::update_fiber`（Active 与失败态双路径；失败态清 ζ + `refresh` 重算 target = **复活**）+ `Runtime::set_update_hook`（观察者先于重跑，loader 条目书签写回经 `update_entry`/`register_update_hook`） | `tests/update_binding.rs`（4：就地重跑身份保留/观察者序/失败复活/退役 panic）+ loader 6（update_entry 就地/自更新写回/组内映射/inject 消费/遮蔽序/组继承） | 完成（PR #29，REVIEW-97bb598 major-1 采纳 TS `_error = undefined` 复活语义；复活通道 = `l_raise_failed_fiber_can_retry_activation` 的 update 变体；退役粘滞/条目权威语义不变） |
| 配置 Entry | Def 74 | `cordis_loader::{Entry, Loader}`（`register_component`/`apply`/`fiber`，§5.2.1 增量协调） | 单元 | 完成（PR #8 最小版：`id`/`config`（经 `revision`）/`disabled`/组件名） |

## 定理覆盖

| 定理 / 结论 | 测试位置 | 状态 |
|---|---|---|
| Thm 7 / Thm 16：LIFO 恢复、声音不变量 | `effect::tests` / `context::tests`（`execute_runs_inverses_in_lifo`、`thm16_*`、`accumulator_reverts_all_effects_lifo`、**`nested_effect_reverts_in_application_order`**） | 完成（PR #3；嵌套顺序审查后修复） |
| Cor 21：独立效应乱序撤销 | `context::tests::cor21_independent_effects_revert_in_any_permutation`（**穷举全部 4! = 24 种排列**，每步断言 Thm 20(1) 中间态） | 完成（PR #9 + M0 走查强化：不同键 `set` 满足 Def 19 独立性——变换可交换；clause(2) 因状态无关逆退化，见已知偏差） |
| Thm 63：依赖者先停、teardown 可读依赖 | `runtime::tests::withdrawal_cascade_disposes_dependents_first`（teardown 检查逆） | 真实引擎已验（PR #5）；停用**结果**经 property（活跃集一致）覆盖，teardown 可读的**顺序性**由集成测试直接验证（M0 走查：表述厘清；oracle 两态模型不携带转换内顺序） |
| Thm 64：单转换不跨两次解析 | `runtime::tests::target_change_mid_reload_chains_unload`（目标中途变化 → 惯性链卸载） | 真实引擎已验（PR #5）；M0 走查：括号"guard 步界中断"属 §4.3.2 效应层（`execute_interrupts_at_step_boundary`），Thm 64 为解析层惯性——两层级测试分开对应 |
| Thm 66：Progress、guard 不死锁 | `interp::tests::drive_*`（oracle 自检）+ `tests/property.rs`（每个动作后 `is_quiet` 断言） | oracle 已验（PR #2）；真实引擎已验（PR #6）；M0 走查：到达静止已验，定量上界 `(K+4)(V+1)` 未断言（记录为覆盖缺口） | **（P-6 补测 2026-08-22：`progress_quantitative_upper_bound`——K=2 链深 2 拓扑，总步数 6 ≤ ΣB(n)=612 直证，缺口关闭）**
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
| 2026-08-16 / M0 走查 | **实现缺口（公开差异）**：Def 30/31 的 interception 完整形态（Σinter = 每键 provider 函数 `σ(k): ℳ→𝒱`；`get(k,μ) = σ(k)(μ⊕ₖι(k))`）未实现——当前 `intercept` 仅累积元数据到 `ι` 表（§5.1.2 的实现描述部分）、`get` 直读 store 从不咨询 `ι`，无 provider 函数概念与生产消费点；§5.1.2 的"@@intercept is consulted only when a binding is accessed"无对应。**补充（审计 F2）**：`get` 签名无 `μ` 参数（`Context::get<K: Key>()`，component-declared metadata 概念整体缺席）；论文自身"右偏"表述存在张力——Def 31 get 侧为 `ι(k)` 覆盖组件 `μ`，而 intercept 操作式 `ι[k↦ι(k)⊕ν]` 为 `ν`（新元数据）优先（§5.1.2 文字）；实现采纳 intercept 操作侧语义（new 优先），get 侧无实现。**更新（M2-PR2，PR #18）**：读路径消费 `ι` 落地（`get_meta`）；`get` 签名仍无 `μ` 参数（常量 provider 函数，见处置） | Def 30/31, §5.1.2 | **部分落地（M2-PR2，PR #18）**：元数据侧求值（`Context::get_meta` 右偏合并 + `Component::declared_metadata`（𝔇inter）+ `Context::intercept_in_place`（§5.2.1 就地分派、不触发 reload））；provider 函数形态（σ(k): ℳ→𝒱 价值核心）公开差异声明，随⑨ typed world 评估 | 部分落地 |
| 2026-08-16 / M0 走查 | **实现缺口（公开差异）**：§5.1.4 Algorithm 6 的 Proxy 访问层整节未实现——沿 fiber 链解析（committed/inject/root）的 `ctx[key]` 属性访问、规格强制（`INACTIVE_ACCESS`/`UNDECLARED_ACCESS`）无对应物；`ctx.get` 为裸 store 查找（与论文一致地"从不失败"），但"读视图而非 store 的规格强制访问"缺席 | §5.1.4, Alg 6 | 公开差异声明（实现缺口；DX 层后续，列入 M1 首批任务） | 记录 |
| 2026-08-16 / M0 走查 | `Disposer = Box<dyn FnOnce>` 为命令式载体，替代论文 `g: Γ→Γ` 纯函数——变换幺半群 `𝔐(e)`（Def 17–19 的独立性形式定义）在代码中无可表达结构，独立性退化为一阶约定（"逆只撤自己的键"）；语义保证（Thm 7/16/20/21）经命令式测试成立 | Def 8/17–19 | 公开差异声明（结构性；wasm 边界句柄化时按论文形态对齐） | 记录 |
| 2026-08-16 / M0 走查 | `Step` 无「无逆步骤」变体（Alg 1 的 `if value` 无对应）；实际对齐 Def 51 三元组（`g` 恒存在）更贴切——`Yielded`/`Finished` 恒携带逆 | Def 51, Alg 1 | 实现说明（与 Def 51 一致，Alg 1 的 `Maybe` 分支为一般化） | 记录 |
| 2026-08-16 / M0 走查 | Thm 7（单组件累加器恢复）无独立测试——与 Thm 16 合并声明（`accumulator_reverts_all_effects_lifo` 兼作两者）；声音不变量在终态断言 | Thm 7/16 | 实现说明（合并覆盖声明） | 记录 |
| 2026-08-16 / M0 走查 | Def 33–42 观测等价 ≃ 无对应物——`InterpState` 的 `PartialEq` 是结构相等而非商结构；≃ 为理论构造（导出独立性），非运行时实体 | Def 33–42 | 公开差异声明（理论构造不落运行时） | 记录 |
| 2026-08-16 / M0 走查 | O-Insert 的 `π ∈ dom(Fγ) ∪ {root}` 前提未在引擎侧执行——`RegistryError::UnknownParent` 仅由 property harness 合成，`register` 不校验父存在性（依赖 `HasChildren` 移除前提维持父存活不变式） | §4.2 O-Insert | 公开差异声明（防御深度缺口；不变式由移除前提维系） | 记录 |
| 2026-08-16 / M0 走查 | `is_quiet` 的 `Inactive(_)` 分支只判 `target.is_none()`，未实现 Def 49 式 (45) 的 `ζ≠⊥ ∨ target=⊥` 析取——当前 ζ 恒 None（L-Raise 未实现）掩盖；**L-Raise 落地时必改** | Def 49 式 (45), §4.3.4 | **已修复（M2-PR1，PR #17）**：`Inactive(Some(ζ))` 恒静止 | 已修复 |
| 2026-08-16 / M0 走查 | **实现缺口（公开差异）**：Table 2 的 **L-Raise 整条未落地**——`FiberError` 无生产者、`FiberState` 的 `outcome`/ζ 恒 None、组件 apply panic 直接传播（panic = bug）而无「error recorded + target=⊥」回记；§4.3.4 失败模型（`𝔈fail`、`Either(Ξ,…)`）整体留待 async 阶段 | §4.3.4, Table 2 | **已落地（M2-PR1，PR #17，同步适配）**：`FiberError::raise` 载荷 + `reload` `catch_unwind` 识别（其余 panic = bug 重抛）→ 目标 ⊥ + 卸载恢复已完成步骤 + `Inactive(Some(ζ))`；`Either(Ξ,…)` 的异步形态仍随 async 阶段 | 已修复 |
| 2026-08-16 / M0 走查 | Def 47 注册：引擎在 `register` 时**立即非可逆地 O-Insert**，逆仅含 refresh→retire；论文把 O-Insert 建模为可逆效应本身（逆 = O-Retire）——可观察等价（retire 恒被累加器持有），oracle 侧 `Fiber.registered` 已记录，引擎侧结构差异补记 | Def 47 | 实现说明（可观察等价） | 记录 |
| 2026-08-16 / M0 走查 | Def 48 纪律 clause(2)（读仅限声明依赖）无运行时执行——越界**写**有 panic 检查（已记录），读任意 realm 无 confine 检查（论文为组件义务，非证明前置） | Def 48 | 公开差异声明（义务不检查） | 记录 |
| 2026-08-16 / M0 走查 | `Fiber::target` 存完整 `Option<View>` 而非 §4.2 所述 hash（论文：`fiber.committed` 存 map、`fiber.target` 存 hash）——语义等价（hash 为优化），§5.1.3 实现细节 | §4.2, §5.1.3 | 实现说明（优化取舍） | 记录 |
| 2026-08-16 / M0 走查 | §5.1.2 isolate 默认 realm 生成未实现——`Context::isolate` 要求显式 realm 参数（Def 29 本体一致；"freshly generated symbol by default"为 §5.1.2 描述性便利） | §5.1.2 | 公开差异声明（显式 realm 为 API 设计） | 记录 |
| 2026-08-16 / M0 走查 | Alg 3 的 `return affected` + Alg 5 的 `await all(...)` 显式等待语义无对应——同步核心以递归级联隐式完成（notify 返回 `()`，转换同步跑完）——语义等价，机制不同 | Alg 3/5 | 公开差异声明（同步适配） | 记录 |
| 2026-08-16 / M0 走查 | Thm 59（Preservation 四条款守卫不变式）与 Thm 61（Recovery exactness 全局交错）无直接测试——经 Thm 66/73 property 与 §3.1 局部 LIFO 测试间接覆盖 | Thm 59/61 | 记录（覆盖缺口；列入 M1 首批任务） | 记录 |
| 2026-08-16 / M0 走查 | **修复（F4）**：Thm 73(1) canonical form 补测 `thm73_canonical_form_static_assembly`——动态历史（乱序注册+退役+移除+重装）与静态装配（按 ⊲ 序一次性装入）静止态按 (inject, provide) 签名比较一致（up to names）——"动态历史无痕迹 = 静态装配"招牌承诺落地 | Thm 73(1), §4.4.5, Lemma 56 | 修正（含测试） | 已修复 |
| 2026-08-16 / M0 走查 | **修复（PR #9 审查 nit1）**：Cor 21 测试由 4 种代表排列强化为**穷举全部 24 种排列**（字典序迭代器） | Cor 21 | 修正（测试强化） | 已修复 |
| 2026-08-16 / M0 审查 | **修复（REVIEW-M0 风险提示）**：`Context::intercept`/`intercept_of` 为公开 API 但语义半成品（`get` 不消费 `ι`）——rustdoc 补**半成品警示**（"读路径消费由 M1 落地，此前按'拦截已生效'使用将静默无效果"） | Def 30/31 | 修正（rustdoc 警示） | 已修复 |
| 2026-08-16 / PR #11 | **WasmComponent 接入 cordis-core**：`cordis_wasm::WasmComponent` 实现 `Component`（inject/provide 跨边界调用；`apply` 返回 `WasmTaskIter`——宿主驱动 `task.step()`，guest 在 step 内的 `context::set` 记录为 pending，迭代器转发到核心 [`Context::set_dyn`]（**符号级动态绑定**：core 新增 `bind_value`/`unbind_value` + `set_dyn`，ADR-0004 值语义——跨边界 wit `Value` 装箱进核心 store），逆按 rep 存入实例 `core_inverses`（**非 Send**：核心逆捕获 `Rc<Context>`，故存于 `Rc<RefCell>` 实例状态而非 `Host`——`WasiView: Send` 约束）；迭代器每步逆 = 经 rep 执行核心 Disposer（unbind + notify），与 `PushingIter` 共享 `StepGuard` 幂等；`Host::set` 镜像先行（guest 的 get 立即可读）、逆执行时清理；测试 `bridge_core.rs`（激活 → 核心 store 绑定 → σγ 计入 → retire 级联清除 + 镜像断言） | Def 8/43/45/47/51, §6.3, Alg 4/5, ADR-0004 | 完成（PR #11：wasm 组件进入核心演算） | 记录 |
| 2026-08-16 / PR #12 | **wasm 依赖者消费（桥接完整化）**：`Context::get_dyn`（符号级动态读，`Ref::filter_map` 类型擦除）——guest 的 `get` 读注入依赖；`WasmTaskIter` step 前 `sync_injected` 把核心 store 中本组件 inject 键的值同步进镜像（值为另一 wasm 组件的 wit `Value` 装箱时可同步；原生组件提供的值**不同步**——跨类型值翻译 M1 不支持）；consumer guest 示例 `examples/wasm-plugin-rust-consumer`（注入 db → 读 `wasm-pg` → 提供 `derived(wasm-pg)`）；测试 `dependency_consumption.rs`（provider+consumer 双 wasm 激活 → 注入读取 → retire 级联停用 → 绑定全清）；**借用纪律**：`Runtime::store()` 的 `Ref` 是 Drop 类型、借用活到作用域末——持有者不得跨退役调用（测试块级隔离） | Def 23/24, §6.3, ADR-0004 | 完成（PR #12：wasm 依赖消费闭环） | 记录 |
| 2026-08-16 / PR #12 审查 | **修复（m1，REVIEW-54a9b08）**：补 **isolate × wasm 交叉回归测试** `tests/isolated_wasm.rs`（2 测试）——固化 REVIEW-2a7a686 m1/m2 修复在隔离上下文 + wasm 组合下的正确性：wasm provider 在隔离 ctx 激活 → 绑定落**隔离 realm**（ρ 解析）；同 realm consumer 激活、跨 realm Inactive（O-Insert 键级供给不相交语义：断言后退役+移除释放供给名）；供给纪律按键判定（realm 解析后仍合法）；退役级联绑定全清 | Def 23/29/43/48, REVIEW-2a7a686 | 修正（交叉测试） | 已修复 |
| 2026-08-16 / PR #13 审查 | **修复（m1，REVIEW-1df64a1）**：`Cargo.lock` 补提交（cordis-wasm dev-dep 新增 cordis-loader 的锁变更）——可复现构建纪律恢复 | — | 修正（锁文件） | 已修复 |
| 2026-08-16 / PR #13 审查 → **2026-08-22 P-2 闭环** | **记录（m2，REVIEW-1df64a1）→ 已下沉（产品验证线 P-2）**：`Value` 原住 cordis-wasm（原生↔wasm 互通须依赖 wasm crate，依赖方向"原生 → wasm"违背分层）——P-2 方案 C：独立 `crates/cordis-value`（统一值类型，零第三方零 core 依赖）+ cordis-wasm 桥接转换层（trait 边界 to_cv/from_cv）——原生与 wasm 均经 `cordis-value` 互通（依赖方向"原生 → cordis-value"），跨类型值翻译边界消除（dual_backend 双向互通直证；wit external 映射因工具链限制不可行，转换层为最终形态——REVIEW-3a3591c M-1） | ADR-0004 | 完成（P-2：值类型下沉 + 互通消除边界） | 已闭环 |
| 2026-08-16 / PR #13 | **双后端共存（M1 门禁）**：同一 `Loader`/`Runtime` 同时加载原生与 wasm 组件——`WasmComponent` 实现 `Component` trait，loader 的 `register_component`/`apply` 天然兼容两类；**值类型统一决策**：原生组件经 `Context::set_dyn`/`get_dyn` 使用与 wasm 相同的 **wit `Value` 装箱**——跨类型值翻译的 M1 边界收窄为"双方都走动态值 API 即可互通"；测试 `dual_backend.rs`（2 测试）：wasm provider + 原生 consumer（get_dyn 读 wit Value → 派生 `native(wasm-pg)`）、原生 provider + wasm consumer（读 `native-pg` → 派生 `derived(native-pg)`）——双向互通 + 条目移除级联停用 + 绑定全清 | Def 43, §5.2.1, §6.3, ADR-0004 | 完成（PR #13：双后端互通闭环） | 记录 |
| 2026-08-16 / PR #12 审查 | **修复（nit1，REVIEW-54a9b08）**：`sync_injected` 双查（`is::<Value>` + `downcast_ref`）合并为单次 `downcast_ref`（`if let Some` 分支） | — | 修正（单次判型） | 已修复 |
| 2026-08-16 / PR #11 | **实现说明**：`HostInverse::run` 为防御性 no-op（guest 主动调 `run` 无核心访问；真实逆经 rep 表由迭代器闭包执行）——逆句柄化的完整形态（wasmtime `Resource` 句柄作为核心逆载体）留 M2 | Def 8, PLAN §4.4 | 实现说明 | 记录 |
| 2026-08-16 / PR #10+11 审查 | **修复（B1，REVIEW-73314ba blocker）**：CI 步骤顺序——`build wasm guest (M1)` 移至 `cargo test` **之前**（guest 为独立 crate、独立 target、rust-cache 不覆盖；干净环境必红，Actions 双重实测） | — | 修正（CI 顺序） | 已修复 |
| 2026-08-16 / PR #11 审查 | **修复（m1+m2，REVIEW-2a7a686）**：`set_dyn` 改为按**键**判定供给纪律 + 内部 `resolve_realm`（与 typed `set` 完全对称）——isolate × wasm 组合语义由核心承担（原实现按 realm 判定会误杀合法写入/放过未声明写入，且 wasm 桥接缺 ρ 解析）；`WasmTaskIter` 传键 | Def 23/29/43/48 | 修正（键/realm 对称） | 已修复 |
| 2026-08-16 / PR #11 审查 | **修复（m4，REVIEW-2a7a686）**：`inverse.run` guest 调用由静默 no-op 改为 **panic（协议违反 = bug）**——撤销只归宿主驱动（组件卸载路径）；wit 注释明示"宿主专用，guest 不得调用" | Def 8 | 修正（显式失败 + wit 警示） | 已修复 |
| 2026-08-16 / PR #11 审查 | **记录（m3，REVIEW-2a7a686）**：`HostInverse::drop` no-op——核心逆在 `InstanceState`（`Host` 不可达），`next_rep` 与逆表槽位**单调增长**属已知边界（与组件生命周期 set 次数同阶；M2 提供回收） | — | 记录（已知边界，文档已注明） | 记录 |
| 2026-08-16 / PR #10 | Wasm 后端起步（M1）：wit 世界 v1（`cordis-wasm/wit/cordis.wit`：import context（get/set + `inverse` 资源句柄化）+ export plugin（`component` 资源 = Def 43 的 (d,p,e)、`task` 资源 = Def 51 𝔈iter 跨边界））；工具链定型——guest 以 **wasm32-wasip2** target 编译（rustc 直接产出组件二进制）+ **no_std + alloc**（能力面 = 仅 context 接口，论文 §6.3 import 面即能力面）；宿主 `Host` 实现 context/inverse/WasiView（wasip2 标准库引用 WASI p2，经 `wasmtime_wasi::p2` 提供）；端到端测试 `tests/load_guest.rs`（constructor → inject/provide 核对 → start/step 激活绑定 → 逆 run 撤销）；guest 示例 `examples/wasm-plugin-rust`（db 提供者）为独立 crate（wit-bindgen ABI 胶水用 unsafe，与 workspace `unsafe_code=deny` 冲突） | Def 8/43/51, §6.3, Alg 4/5 | 完成（PR #10：加载/驱动原语闭环；逆句柄表） | 记录 |
| 2026-08-17 / PR #14 | **沙箱隔离（M1 门禁 2/3）**：恶意 guest（`examples/wasm-plugin-rust-panic`，step 时 panic → wasmtime trap）在宿主侧以错误（Trap panic）可见；宿主捕获后**进程存活**——继续实例化其他组件、驱动正常 guest 均可；测试 `sandbox_isolation.rs`。论文 §6.3 的 import 面即能力面 + 宿主进程隔离的工程实现 | §6.3 | 完成（PR #14：trap → 宿主错误，沙箱不破） | 记录 |
| 2026-08-17 / PR #14 | **Go guest（M1 门禁 3/3 双语言验收）**：标准 go（**非 tinygo**）实现与 Rust 同语义的 db 消费者（注入 db → `context::get` 读注入值 → 提供 `derived(<db>)`），经预览1 适配器组件化后与 Rust/native 组件在同一 loader 互通——测试 `go_guest.rs`（2 测试：Rust provider / native provider 双路）。**工具链决策**：标准 go 只能产出 wasip1 核心模块（无组件能力），须 `-buildmode=c-shared`（导出 `_initialize`，reactor 语义；普通 exe 只有 `_start`，宿主调导出前 Go 运行时未初始化）→ `tools/componentize`（wit-component `ComponentEncoder` + 嵌入 wit 世界元数据 + 预览1 reactor 适配器，等价 `wasm-tools component embed + new`）；适配器 vendored 于 `third_party/wasi-preview1-adapter/`（wasi-preview1-component-adapter-provider 47.0.3，Apache-2.0 WITH LLVM-exception）；`go.bytecodealliance.org/pkg@v0.2.2` vendored fork `third_party/go-pkg`——移除 tinygo 专有 `runtime.sbrk`（标准 go 无此符号，链接失败），补**预初始化窗口**：适配器首次被调用（Go 运行时 schedinit 期间、包 init 之前）会经 `cabi_realloc` 分配影子栈（64KB）与 State（64KB），此窗口内既不能用 GC（`make` 需已初始化堆）也不能回调适配器（`adapter_monotonic_clock_set_paused` 需要已分配的 State）——fork 以静态缓冲 512KB + bump 指针实现上游 sbrk 语义；另补 `runtime.Handle.TakeHandle`（wit-bindgen 0.60 生成代码所需，上游 v0.2.3 尚无） | Def 43/51, §6.3, Alg 4/5 | 完成（PR #14：Rust + Go 双语言 guest 可互换） | 记录 |
| 2026-08-17 / PR #15 | **M1 走查 §6.2–6.4（门禁判定：通过，含处置清单）+ 处置③ 落地**：Thm 59（Preservation）良构四条款（Def 58）与 Thm 61（Recovery exactness）全局交错直接测试 `tests/preservation_recovery.rs`（3 测试：编排全程逐动作断言四条款——含退役级联/重连/清场；交错纤维中间退役"只撤自己的贡献"；反向顺序退役）；走查全文见「M1 Wasm 后端走查记录」（§6.2 排他绑定 = ProvisionClash + loader 两阶段 apply，broker 可表达无示例；§6.3 能力访问 = import 面 + 镜像仅 inject 键（结构性强制）+ set_dyn 纪律，沙箱 = wasmtime SFI + trap 捕获宿主存活，**新增记录⑧**：恶意 guest 越界 set → 宿主 panic 依赖 catch_unwind 兜底、随 L-Raise 失败模型处置；§6.4 时间维度 = 逆句柄化 + Wasmtime 原生 embedder 丢弃即释放（与论文原文一致），空间维度 = Rust 过程宏路径 + 运行时符号级动态中介（**新增记录⑨**）） | Def 58/59/61, §6.2–6.4 | 完成（PR #15：走查通过 + 处置③测试落地） | 记录 |
| 2026-08-17 / PR #17 | **M2-PR1：L-Raise 失败模型落地（处置⑤/⑧ 收官）**——`FiberError::raise`（panic 载荷）+ `reload` `catch_unwind` 识别（其余 panic = bug 重抛）：失败 → 目标 ⊥ + 卸载恢复已完成步骤 + 终态 `Inactive(Some(ζ))`；`is_quiet` 补 Def 49 式 (45) 的 ζ 析取；wasm 桥接层把 guest trap（`call_step` 错误）、越界 set（核心纪律 panic 载荷）、绑定冲突（AlreadyBound）统一转 `FiberError::raise`——沙箱隔离从"catch_unwind 兜底"升级为失败模型（§6.3 ⑧ 收官）；可信原生组件的越界写仍为 panic = bug（宿主不变式，保留）。测试：`tests/failure_model.rs`（2：错误 outcome + 已完成步骤恢复 + ζ 静止；失败后可重试激活）+ `sandbox_isolation.rs` 升级（trap/越界 set → 失败 outcome、宿主存活、fiber 失败态）；`inverse.run` guest 调用仍为协议违反 panic（宿主专用，保留） | §4.3.4, Def 49 式 (45), Def 8 | 完成（PR #17：失败模型同步适配落地） | 记录 |
| 2026-08-17 / PR #18 | **M2-PR2：interception 求值形态（处置① 部分落地）**——Def 31 读路径求值的元数据侧：`Context::get_meta`（`get(k,μ)=σ(k)(μ⊕ₖι(k))` 的合并，组件声明 `d(k)` 与上下文携带 `ι(k)` 右偏、ι 优先——§6.3 外层上下文约束语义）、`Component::declared_metadata`（Def 30 的 𝔇inter，默认 ε）、`Context::intercept_in_place`（§5.2.1 loader intercept 字段分派：就地更新不触发 reload，可逆性由调用方承担）；`Fiber` 存组件引用；provider 函数形态（σ(k): ℳ→𝒱 价值核心）保持公开差异（随⑨）。测试：`tests/interception.rs`（5：右偏合并、回退语义、就地不 reload、派生 vs 就地、类型冲突） | Def 30/31, §5.2.1 | 完成（PR #18：读路径消费 ι 落地） | 记录 |
| 2026-08-17 / PR #19 | **M2-PR3：loader 全字段 + 配置树 + group/include（§5.2.1）**——`Entry` 补 Def 74 的 `isolate`（`IsolateAnnotation`：Local 按条目 id 打标 / Global 命名共享；实例化期经派生 ctx 应用，**变更 = 重建**，Algorithm 7 realm 重指派随 M2-PR4）与 `intercept`（`Intercepts` 键 → 元数据；`intercept_set_boxed` 替换语义 + `intercept_clear`，**就地更新不触发 reload**——§5.2.1 "intercept — updated in place"）；`Entry::group`/`include` 分支条目（`children`）——组持有者 fiber（无注入/供给空组件）承载子条目（π = 组 fiber，Def 47），子列表按 id **keyed diff** 递归（幸存子条目不重建），组退役/移除 → 级联拆除子树（自底向上，O-Remove HasChildren 前提）；`Context::derive`（隔离派生原语）；已知边界：双向写回未实现（条目权威）、isolate 变更走重建（**PR #20 升级为 Algorithm 7**）、组条目 isolate 不应用。测试：loader 7 新增（拦截就地不重建/移除回退、Local/Global realm、group keyed diff、组拆除/禁用/重启用、include 嫁接、isolate 变更重建） | Def 74, §5.2.1 | 完成（PR #19：配置树 + 最小扰动协调） | 记录 |
| 2026-08-17 / PR #20 | **M2-PR4：托管 realm + Algorithm 7 隔离 realm 重指派（§5.2.1）**——isolate 变更从"重建"升级为**就地重指派**：`Context::isolate_in_place`（ρ 就地改写）+ `Store::move_binding`（绑定迁移）+ `Runtime::notify_affected`（自定义谓词通知）+ `provider_of_realm`；loader `patch_isolation`：Δ 变化键 → patch ρ（条目 ctx + 子树 fiber ctx）→ refresh 子树 → 移动子树绑定（own ∧ store[s1] ∧ ¬store[s2]，own 在移动前快照）→ 通知 affected 外部依赖者（resolve ∈ {s1,s2} ∧ own(D) ≠ own(P)）。**适应记录**：论文 delimiter（δk 标签）的 own 判定以 **loader 树子树成员关系**等价实现；ρ 表为拷贝继承（论文持久化结构共享）→ patch 需遍历子树 ctx。测试：`isolate_change_reassigns_realms_without_rebuild`（绑定迁移/依赖者停用/共同迁移重激活，fiber 不重建）+ `isolate_change_moves_group_child_binding`（子树 own 判定） | §5.2.1, Alg 7 | 完成（PR #20：realm 重指派落地） | 记录 |
| 2026-08-17 / PR #20 审查 | **修复（major1 + nit1-3，REVIEW-ef57804）**：major1 isolate 边界迁移补测（Local→Global、None→Global——裸键 realm ↔ 托管 realm）；nit1 loader 三处"isolate 变更走重建"陈旧表述同步（模块头/已知边界②/apply doc）；nit2 `collect_subtree_ids` 措辞修正（叶子 = 自身，分支 = 子树）；nit3 组分支注释（组 isolate 变更仍整棵重建——组无声明键，M2-PR3 边界） | Alg 7 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #21 | **M2-PR5：cordis-hmr——Algorithm 8/9/10 事务性热重载（§5.2.2）**：新 crate `cordis-hmr`——模块分类不动点（`classify`：stashed/externals 种子、自 stashed 向下传播、环默认拒绝）、过期条目检测（`detect`/`get_dependencies`：依赖树 ∩ accepted、declined 边界）、事务性重载（`Hmr::reload`：备份缓存 → 注册新模块 + revision 递增重建**仅过期条目** → 失败检测（加载错误 / L-Raise fiber 失败态）→ 回滚恢复旧组件）；`ModuleGraph` trait（`HashMapGraph` 数据驱动 + `WasmLeafGraph`——M1 wasm 插件仅导入宿主 context 接口故为叶子；cargo metadata / wit import 解析为生产化适配器，M2 边界）；`ModuleLoader` trait（wasm 重读文件 / native 编排方提供新版本）；`Loader::component_of`（备份用）。**门禁用例直证**：改代码保存即生效（stashed → 新版本生效、其他组件不重建状态保留）+ 两个回滚用例（加载失败回滚 / 组件运行失败（L-Raise）回滚——不进入半重载）。测试：hmr.rs 6（分类传播/externals/环、detect/declined 边界、重载成功 + 双回滚） | §5.2.2, Alg 8/9/10 | 完成（PR #21：HMR 三阶段 + 事务回滚） | 记录 |
| 2026-08-17 / PR #22 | **M2-PR6：Alg 6 Proxy 访问层（处置② 落地）+ 处置⑥⑨ 评估收尾**——`Context::resolve`（Algorithm 6 直译：沿 fiber 链向上，首个 committed 视图绑定 key 的 fiber 授权（返回承诺视图下解析的绑定值）；声明未提交 → `AccessError::Inactive`（INACTIVE_ACCESS）；至 root 无声明 → `Undeclared`（UNDECLARED_ACCESS）——**读视图非裸 store**，Thm 63 语义；与 `Context::get`（裸 store、从不失败）互补）；`AccessError` 导出；**处置⑥ 评估**：命令式 Disposer 保留（语义由命令式测试保证，wasm 逆句柄化已对齐，纯函数形态关闭为记录）；**处置⑨ 评估**：保持通用 context + 符号级动态解析（论文要求的运行时动态中介），typed world 列为 DX 增强（关闭为记录）。测试：`tests/access.rs` 4（授权访问 / INACTIVE / UNDECLARED / 链上行父子） | §5.1.4, Alg 6 | 完成（PR #22：处置② 落地 + ⑥⑨ 评估收尾） | 记录 |
| 2026-08-17 / PR #21 审查 | **修复（major1 + nit1-5，REVIEW-4c6e7fc）**：major1 **事务 panic 安全**——`Hmr::reload` 整个事务 `catch_unwind` 包裹：任何 panic（配置错误 = ProvisionClash/未知组件，`apply` 以 panic 表达）**先回滚再重抛**（panic = bug + 永不半重载）；nit1 回滚 revision+2 启发式注释；nit2 stale url 去重（多条目共享组件名）；nit3 失败检测仅限过期条目（非过期条目既有失败 fiber 不误判）；nit4 补测：空 stashed 空操作、供给冲突 panic 回滚直证（catch_unwind 断言 + 系统一致）、多条目共享 url；nit5 模块 doc 语义耦合注明。测试 hmr 9 | Alg 10 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #22 审查 | **修复（major1 + nit1-3，REVIEW-e8bd96e）**：major1 链上行路径**真实行使**（新测试：访问 fiber 无注入、祖先声明并已加载——原测试在子自身 committed 短路）；nit1 **realm 漂移适应记录**：committed 视图仅存 key→provider 不含 realm，`resolve` 经授权 fiber **当前** ρ 重读——Algorithm 7 重指派瞬态窗口可能漂移（THEORY-MAP 适应记录）；nit2 `AccessError` 补 Display/Error；nit3 TypeMismatch→Inactive 折损注释（typed world 时精确判别） | §5.1.4, Alg 6 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #24 | **M3-PR1：IM-bot 迷你案例（§5.3 Koishi 式三层拓扑）+ 处置⑦ broker 示例**——`examples/im-bot`：adapter 层（提供 platform）/ database 层（提供 db）/ 功能插件 bot（注入两层、提供 reply）；运行时操作直证——切换存储后端（同一条目 revision 递增重建、新 fiber 重供 db 键）只重激活解析依赖变化的依赖者（bot 重激活、fiber 不变）而 adapter 不受影响；重连 adapter（退役→移除→重装）→ bot 级联停用再自动重连、database 不受影响；依赖不可用 → bot 保持 Inactive 不报错、adapter 出现自动激活（§5.3 "stays inactive until it appears, without erroring"）；broker 示例（§6.2）：**依赖方向按原文**——broker（中央服务）provide 注册句柄 + service 入口、后备**注入**注册句柄经**可逆效应**注册（`ctx.effect` 追踪逆）、消费者注入 service 每次请求由 broker 分发；直证：更新/卸载后备**不扰动 broker 与消费者**（fiber 不变、效应不重执行（计数直证）、service 绑定不变、无 reload）、卸载后备 = 可逆注册自动撤销（**仅路由集移除**）、重注册自动恢复。CI 门禁：im-bot + broker 双 bin 运行 | §5.3, §6.2 | 完成（PR #24：案例验证起步，审查修复后闭环） | 记录 |
| 2026-08-17 / PR #24 审查 | **修复（major1+2 + nit1-3，REVIEW-d1263fa）**：major1/2 **broker 依赖方向反向**——原实现让后备 provide 注册键、broker inject（硬依赖）→ 卸载后备级联 broker→service→消费者停用（正是 §6.2 broker 要避免的扰动）；按 §6.2 原文重排：broker（中央服务）provide 注册句柄 + service、后备注入句柄经可逆效应注册、卸载仅撤路由集；新增**效应重执行计数**直证"无扰动"（消费者全程未重执行）。nit1 main.rs "同供给键替换"措辞更正（实为同条目 revision 递增重建）；nit2 消费者探针角色注释；nit3 docs 同步更正（处置⑦/PR #24 行） | §5.3, §6.2 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #25 | **M3-PR2：基准报告（§5.1.2 notify 扇出 + §5.3 切换延迟）**——`examples/im-bot/src/bin/bench.rs`（std::time 手动基准，零依赖，reps=5 中位数 + 全程 fresh loader 防 no-op）**三层分离测量**（REVIEW-bbb252a 定案）：notify 扫描本体（`ctx.notify` 微基准直证 Algorithm 3 单次 O(F) 线性扫描：0.6→367.6µs、每十倍增 ≈11×）；传播净成本（激活无 diff 污染 + 停用/再激活减 diff 基线，N≤100 近线性 ≈10×/十倍增）；loader 协调总账（diff 基线 100→1000 = 47×，O(N²) `desired.iter().rev().find()`——N≥500 主导 apply 总账，已知边界①索引优化）。场景 B 切换延迟（三层 + M∈{0,100,500} 无关组件）：**直证重激活局部性**（ExecCount：bot 恰 1 次、adapter 0 次、fiber 不变、填充全程 Active，与 M 无关）；切换耗时含 O(M²) diff，如实记录。报告 `docs/bench/M3-BENCH.md`（release 数据表 + 三层归因 + 对 §5.3 "future work" 量化测量的补充 + 已知边界 ①②③）。CI 门禁：bench bin 运行（近线性门禁仅干净路径 + 状态断言 + 绝对上界，debug 余量 ≈5×/28×） | §5.1.2, §5.3 | 完成（PR #25：基准报告 + 门禁，审查修复后闭环） | 记录 |
| 2026-08-17 / PR #25 审查 | **修复（major1-3 + nit1-4，REVIEW-bbb252a）**：major1 停用/再激活复用 loader → rep 2+ 命中 reconcile 幂等短路，中位数测到 loader O(N²) diff 而非传播——全部改 fresh 系统单次转换 + diff 基线减法；major2/3 归因更正——notify 扫描本体 **O(F) 线性**（`ctx.notify` 微基准直证）不解释 45×，真实主导是 `apply_into` 阶段一 `desired.iter().rev().find()` 的 O(N²) diff + 激活/teardown 序列残差（与扫描无关），报告三层分离重写；nit1 debug 余量更正（>60× → ≈5×/28×）；nit2 近线性门禁仅保留干净路径（激活/扫描）；nit3 场景 A 补 assert_quiet；nit4 THEORY-MAP 错别字（电机→直证）+ 联动更正 | §5.1.2, §5.3 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #26 | **M3-PR3：处置⑩⑪⑫ 评估收尾**——⑩ 双向写回：`Fiber::retire` 退役粘滞语义钉死测试（`retired_component_persists_across_unchanged_apply`：退役跨未变 apply 保持、条目权威、revision 递增恢复）+ loader 已知边界① 评估结案（组件→条目写回属编排责任，公开差异）；⑪ 组条目 isolate：候选语义（继承至子树、最近注解优先）与 Algorithm 7 patch 交互评估——Def 74 声明 isolate 应用于 entry context（组亦为 entry）但未展开组级继承，组 isolate 因 GroupHolder 空键自然 no-op（字面偏差），记录公开差异随 typed world 实现；⑫ 模块图生产化适配器：算法 crate 无 TOML/JSON 解析器依赖（hmr 仅 anyhow），`HashMapGraph` 已证算法数据驱动，适配器随构建工具 crate（可引 serde_json/toml）落地。cordis-hmr 模块图文档评估结案注记 | §5.2.1, Def 74 | 完成（PR #26：处置⑩⑪⑫ 评估收尾） | 记录 |
| 2026-08-17 / PR #19 审查 | **修复（major1 + nit1-7，REVIEW-24bfab5）**：major1 `intercept_clear` 语义定案——**不回退父（组）继承值**（扁平拷贝无父链；doc 修正 + loader 已知边界⑤ + 组子覆写再移除负向直证）；nit1 apply doc 改 `intercept_set_boxed`；nit2 typed `intercept_set` 注明预留定位；nit3 叶子重建分支标注阶段一兜底（防御性）；nit4 `instantiate_group` 复用 `annotated_ctx`；nit5 级联双注册注释（register 逆落派生 ctx 为孤儿、显式父效应为级联通道）；nit6 组内同供给替换测试；nit7 组分支防呆记录注释 | §5.2.1 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #20 审查 | **适应记录（公开差异）**：Algorithm 7 的 delimiter 机制（`δk` 标签继承判定 own）以 loader 树子树成员关系等价替代——可观察语义一致（"绑定是否属本条目"），机制不同；ρ 表拷贝继承（论文持久化结构共享）使 patch 需遍历子树 ctx——同步核心适配 | Alg 7 | 公开差异声明（机制等价替代） | 记录 |
| 2026-08-17 / PR #17 审查 | **修复（nit1-9，REVIEW-32a913d）**：nit1 负向判别直证（非 FiberError panic → resume_unwind 重抛，`should_panic` 测试）；nit2/3 `FiberState` ζ 文档更新 + `Unloading.outcome` 死字段说明（ζ 直落 Inactive，卸载中间形态随 async）；nit4 wasm 桥接**供给纪律预检**（越界写不进入核心 set_dyn，catch_unwind 面收窄）+ 已知边界注释；nit5 `raise` 依赖 panic=unwind 注明；nit6 call_step 错误分类边界注释；nit7 loader 失败 fiber 静默加载记录（M2 后续任务）；nit8 PLAN M2 行标注"首批任务前置、HMR 主目标未开始"；nit9 失败卸载路径测试（依赖者停用、绑定全清、静止） | §4.3.4 | 修正（全部落实） | 已修复 |
| 2026-08-17 / PR #18 审查 | **修复（nit1-4，REVIEW-6e0fd1e）**：nit1 `dom(d) ⊆ inject` 未强制 → 公开差异声明（义务不检查，与 Def 48 同型）；nit2 `get_meta` 两侧类型纪律不对称文档化（声明侧 panic / 携带侧 None）；nit3 component Rc 双重克隆——**无需修改**（apply 闭包与 Fiber 字段各需一份所有权）；nit4 `get_meta`/`intercept_in_place`/`intercept_set`/`intercept_set_boxed` 接收者 `&Rc<Self>` → `&self` | Def 30/31 | 修正（nit3 记录为无需修改） | 已修复 |
| 2026-08-17 / PR #18 审查 | **记录（nit1，REVIEW-6e0fd1e）**：Def 30 的 `dom(d) ⊆ inject` 约束（组件声明的元数据限于依赖键）仅作文档断言、未强制——`Component::declared_metadata` 可对任意键声明（引擎不校验）；属组件义务（与 Def 48 读纪律同型，均不检查） | Def 30, Def 48 | 公开差异声明（义务不检查） | 记录 |
| 2026-08-17 / PR #28 | **TS 参考实现对照：缺口分析（cordiverse/cordis v4 ↔ cordis-rs ↔ 论文）**——`docs/TS-REFERENCE-GAP.md`：通读 TS core/loader/hmr/include/group 后逐特性对照。真实功能缺口 9 项（G1 双向绑定（重新打开处置⑩：TS `Fiber.update` + `internal/update` 写回——§5.2.1 "runs in both directions" 的 loader 契约）、G2 每键注入配置（Def 30/31 ι 实用化，TS inject: {[key]: config}）、G3 per-key isolate 粒度、G4 hooks 最小集、G5 配置表达式插值、G6 include 文件树（watch/持久化/patch）、G7 config 校验 + 值级 diff、G8 set 就地改值、G9 Service check 谓词）；⑫ 发现零依赖可行路径（编排工具生成依赖清单文件 → HashMapGraph 消费）；已对应核验 18 项（Alg 2/3/5/6/7/8/9/10、失败模型、realm 语义等）；反向对照 5 项领先（wasm 沙箱/双语言 guest/形式化测试/类型化键/同步确定性） | §5.2.1, §5.2.2 | 完成（PR #28：缺口分析记录，行动建议排序见报告 §5） | 记录 |
| 2026-08-17 / PR #29 | **G1 双向绑定 + G2 每键注入配置（TS 参考实现对照落地）**——core：`Fiber::update`（§5.2.1 双向绑定组件侧；换 config 闭包 + unload 链式重跑，fiber 身份保留、依赖者级联、L-Raise 同路径；仅 Active 可调用）+ `Runtime::update_fiber`/`set_update_hook`（`UpdateHook` 观察者先于重跑触发，TS `internal/update` 序）；loader：`update_entry`（条目书签 + 就地重跑，不递增 revision——同 revision apply 不清除 fiber 层写回）、`entry_config`、`register_update_hook`（fiber→条目递归反查写回，组内子条目命中）、`Entry.inject`（每键注入携带配置，遮蔽同键 intercept、组条目经派生链继承，读取方 `get_meta` 右偏合并——Def 30/31 ι 实用化；变更纪律同 config）。测试：core `update_binding.rs` 3（就地重跑身份保留/观察者序/非 Active panic）+ loader 6（update_entry 就地/自更新写回/组内映射/inject 消费/遮蔽序/组继承） | §5.2.1, Def 30/31 | 完成（PR #29：G1+G2 落地，TS 对照闭环） | 记录 |
| 2026-08-17 / PR #30 | **G1 剩余 + G4 最小 hooks：退役写回（self-dispose → 条目 disabled；TS `internal/plugin` 半段）**——core `Runtime::set_retire_hook`（`RetireHook`，`Fiber::retire` 同步触发；与 update_hook 同约束：同步路径、panic = 宿主 bug、不参与 L-Raise）；loader `register_retire_hook`（过滤「条目仍在且未 disabled」= 组件自退役 → 书签 `disabled=true`）+ `entry_disabled` 访问器；apply 期间 teardown 的 retire 走 `retire_pending` 队列延迟排空（hook 不重借 entries）+ `in_apply` 标志分流；reconcile disabled 清除路径先拆除已退役 fiber（ProvisionClash 前提）；`loader.fiber(id)` 对退役 fiber 返回 None（已卸载语义）。语义钉死：自退役 → 书签写回 + desired `disabled=false` 重新启用（disabled 为协调字段，与 config 写回不同）；loader 驱动操作（重建/disabled 切换/移除）不写回；组内子条目映射。测试 loader 3（自退役写回/驱动操作不写回/组内映射） | §5.2.1, Alg 5 | 完成（PR #30：G1 剩余 + G4 最小 hooks） | 记录 |
| 2026-08-17 / PR #31 | **G3：per-key isolate 粒度（TS `EntryOptions.isolate: Dict<true | string>` 参照）**——`Entry.isolate` 改 `BTreeMap<Symbol, IsolateAnnotation>`（混合粒度；`with_isolate(key, iso)` per-key builder）；`annotated_ctx`/组分支逐键应用（realm 命名不变：`local:{id}:{key}`/`global:{name}:{key}`）；`realm_of` 逐键查表（无注解键 = 裸键 realm）；`patch_isolation` Δ 键域 = 组件声明键 ∪ 新旧 isolate 映射键（Algorithm 7 逐键重指派保持）；**⑪ 收口：组 per-key isolate 经派生链拷贝继承给子条目、子条目注解覆盖（最近注解优先）**；组 isolate 变更仍整棵重建（保守）。测试 loader +3（混合粒度 / 组继承与覆盖 / 组 isolate 变更重建） | Def 28/29, Alg 7 | 完成（PR #31：G3 落地，⑪ 收口） | 记录 |
| 2026-08-17 / PR #32 | **G5 配置插值 + G6 include patches（TS `interpolate`/`PatchOptions` 参照，安全收窄）**——G5：`cordis-loader::interpolate`（`{{name}}` 受控占位符替换，resolve 回调编排方提供；公开差异：TS `with(ctx) eval` 任意表达式求值不支持、未解析占位符保留原样）；G6：`Patch` + `apply_patches`（desired 树纯变换——`id` 递归匹配的 `name`/`config`/`revision`/`disabled` 覆盖 + `insert` 向组插入、非组目标忽略；原树不动返回新树）。公开差异声明：yaml/json 文件读取、watch、持久化属编排工具层（零第三方依赖纪律）。测试：interpolate 3 + patch 3 | §5.2.1 | 完成（PR #32：G5/G6 收窄落地） | 记录 |
| 2026-08-17 / PR #33 | **G7：config 校验 + 值级 diff（TS `Config` schema + `deepEqual` 参照，opt-in）**——`Config` trait（`validate` + `same`，默认空实现）+ `Loader::register_config::<C>()` 类型注册表（`&dyn Any` 无法 downcast 到 unsized `dyn Config`——按 `TypeId` 注册 cast fn）；`validate_config` 于实例化前调用（失败 = 配置错误 panic，公开差异：TS 为失败态可重试）；`configs_same` 于阶段一/阶段二重建判断（`same` 为真 → revision 递增免重建，TS `deepEqual` 同型）；**HMR 兼容纪律**：`String` 不实现 `same`（cordis-hmr 以 revision 递增 + 复用旧 config 触发重载，免重建会使 HMR 失效）。测试 loader +4（值级免重建/未注册保守 revision/校验失败 panic/未注册不校验） | §5.2.1, Def 74 | 完成（PR #33：G7 落地） | 记录 |
| 2026-08-17 / PR #34 | **G8 就地改值 + G9 可用性谓词（TS `reflect.set` 变异 + `provide check` 参照）**——core `Context::set_in_place`（本 fiber 已声明供给键的值替换：不 notify、不追踪（idΓ 式——论文 "overwritten in place is therefore not observed"；teardown 不恢复旧值）；未绑定 → `NotBound`、非安装者 → `AlreadyBound`（TS "multiple fibers" 同型）、越界写纪律同 `set`）；`Context::set_with_check`（绑定携带 `Rc<dyn Fn() -> bool>` 谓词，`Store::bind_value_checked`；`provider_of` 每次求值、为假视为未提供——依赖者 Inactive，谓词须纯、变化即时生效无需 notify）。测试 core `check_in_place.rs` 3（就地改值不 notify/未绑定 NotBound/check 门控依赖者） | §3.1, Def 23/29 | 完成（PR #34：G8/G9 落地） | 记录 |
| 2026-08-20 / B-A1 | **Step::Await 挂起/恢复= 论文 §4「确定性一次性效应」的产品级扩展（授权记录）**：核心效应原为有限步一次性执行（Def 51 `Maybe(ℑ)` 续体、`execute` 一口气）；`Step::Await` 允许迭代器在步间等待外部异步（wasm 远端回填——wasm 桥时序边界解锁，见 `docs/cordis-wasm-WASMREMOTE-EXIT.md` §4）。添加性：既有迭代器不产 Await → 执行语义零变化（`execute` 对 Await panic = 走错路径提示；`try_execute_with` 为可恢复路径）。恢复入口 `Runtime::advance`（组合线程单线程 push，ADR-0002 保持；未挂起调用 = panic=bug）。授权：用户 2026-08-20「授权 B」（docs/cordis-core-AWAIT-PROPOSAL/PLAN.md） | §4, Def 51 | 扩展（产品层，授权） | 记录 |
| 2026-08-22 / P-3 | **core 授权（产品验证线 P-3，REVIEW-PHASE2-PROPOSAL nit-2 单独立案）**：`Runtime.suspended` 挂起集（登记：激活挂起分支/advance 再挂起；撤销：advance 完成/unload 收账）+ `suspended_fibers()` 查询 + `advance_suspended(judge)` 批量恢复——Await 生产化（B 计划后续，宿主可枚举/上报/批量驱动挂起 fiber）；额度=本线范围不扩面。**backlog ① 跟进（2026-08-22，REVIEW-123-13）**：`Runtime.suspended` 字段已删除——挂起集改为派生自 `Fiber::is_suspended`（`resumable` 单一事实来源，内部重构、公开语义不变） | §4.3.3 异步 | 扩展（授权） | 记录 |
| 2026-08-22 / P-7 | **core 授权（产品验证线 P-7，O-1 升级）**：`context.rs` 供给纪律越界写 4 处 `panic` → `FiberError::raise`（ComponentFailure——与 wasm 对齐：`Inactive(ζ)` + 可复活；错误策略 O-1 闭环，P-5 插件生态真实场景前提满足）；其余纪律（元数据冲突/调用方违约）panic 保持 | Def 43/48, §4.3.4 | 扩展（授权） | 记录 |

## 里程碑走查记录

| 里程碑 | 日期 | 覆盖章节 | 结论 | 未决偏差 |
|---|---|---|---|---|
| M0 原生闭环 | 2026-08-16 | §3.1–3.3、§4.1–4.4、§5.1（§5.1.1–5.1.3 主体；§5.1.4 见未决） | **门禁判定：通过（含处置清单）**——核心演算语义逐规则一致：可逆效应（execute/LIFO/armed/累加器）、共效应（满足谓词/三类分类/realm 键控/isolate 派生/notify 快照）、fiber 演算（组件三元组/七元组/registry 派生量/target/静止/O-Insert-Retire-Remove/L-Reload-Unload/L-Leave/惯性/步界中断）均有测试闭环（Thm 7/16/21/63/64/66/73/Cor 62 + oracle×引擎 2000 用例 + Cor 21 穷举 24 排列）；**本次走查修正**：Thm 73(1) canonical form 补测、Cor 21 全排列强化、3 处表述失准更正（Alg 1 第 17 行 dispose 组合时机、classify 衔接留白、Thm 63/64 表述）、映射表补 `Runtime::satisfied`（σ⊧d 与 γ⊧d=σγ⊧d 语义区分） | 处置清单（下里程碑首批任务）：① interception provider 函数形态（Def 30/31，实现缺口）② §5.1.4 Proxy 访问层（Alg 6，实现缺口）③ Thm 59/61 直接测试 ④ Thm 66 定量上界 `(K+4)(V+1)` 断言 ⑤ L-Raise 落地时 `is_quiet` 补 ζ 析取 ⑥ 命令式 Disposer 结构（wasm 句柄化时按论文形态对齐） |
| M1 Wasm 后端 | 2026-08-17 | §6.2–6.4 | **门禁判定：通过（含处置清单）**——逐节对照无未解释偏差（详见下方「M1 Wasm 后端走查记录」）：§6.2 排他绑定 = O-Insert 供给不相交检查（`ProvisionClash`）+ loader 两阶段 apply（同键替换单次完成）；broker 形态模型可表达（注册 = 可逆效应）但无示例；滚动更新/跨进程为 M2/M3 场景。§6.3 能力访问控制 = wasm 能力面（import 面）+ 镜像仅含 inject 键（get 未声明键 = None，结构性强制）+ `set_dyn` Def 43/48 纪律；沙箱 = wasmtime SFI + trap 捕获宿主存活（门禁 2/3 实测）；拦截求值形态仍为缺口（承 M0 清单①）。§6.4 时间维度 = 逆句柄化 + Wasmtime 原生 embedder 丢弃即释放（与论文原文一致）+ 注册/撤回为可逆效应；空间维度 = Rust 过程宏路径（论文明确描述）+ 运行时符号级动态中介；Go guest 实证语言无关性（门禁 3/3）。**本次走查新增记录**：⑧恶意 guest 触发宿主 panic（越界 set）的边界（见 §6.3 行）⑨运行时符号级动态解析 vs 编译期类型化 DI 边界 ⑦broker 示例缺失（可表达性未演示） | 处置清单（下里程碑首批任务）：① interception provider 函数形态（Def 30/31）② §5.1.4 Proxy 访问层（Alg 6）③ Thm 59/61 直接测试 ④ Thm 66 定量上界断言 ⑤ L-Raise + `is_quiet` ζ 析取（失败模型实现时）⑥ 命令式 Disposer 结构对齐 ⑦ §6.2 broker 示例（M3 案例素材）⑧ 恶意 guest 越界 set → 宿主 panic 兜底（已直证，语义处置随 ⑤）⑨ typed world 评估时重审动态解析边界 |

## M1 Wasm 后端走查记录（2026-08-17，PR #15）

> 程序（PLAN §7）：重读 §6.2–6.4 → 逐条核对已知偏差 → 补查映射 → 输出走查记录 + 处置清单。
> 稳定态确认：`cargo test --workspace` 80 测试全绿（77 + 处置③ 的 preservation_recovery 3）、clippy/fmt 门禁干净、API 冻结（M1 边界）。

### §6.2 Service Multiplexing（服务复用）

| 论文段落 | 实现证据 | 对照 |
|---|---|---|
| 排他绑定：多实现共享一接口、同时至多一个绑定；切换 = 卸载一个 + 加载另一个（瞬间扰动消费者依赖） | `Runtime::register` 执行 O-Insert 前提（`∀m. p ∩ p_m = ∅`）→ `RegistryError::ProvisionClash`（`crates/cordis-core/src/runtime.rs`）；loader 两阶段 apply（先卸载侧释放供给名、再实例化侧）使同供给键替换可单次完成（`same_supply_replacement_in_single_apply` 测试，`crates/cordis-loader/src/lib.rs`） | **对应**（排他绑定为当前唯一形态；"切换" = loader apply 一次完成，扰动 = 级联停用/激活，Thm 63 语义） |
| 服务代理 broker：中央服务被后备提供者与消费者共同注入；多提供者共存；更新后备提供者不扰动消费者（不触发 reload） | 模型可表达：后备提供者各自提供**不同**注册键（无 clash），broker 注入注册键、激活时经 `ctx.set` 绑定服务键（broker 在 provide 声明），消费者注入服务键；卸载后备 = 逆执行撤销注册（可逆效应：set 绑定 + 逆 unbind） | **可表达但无示例**（处置⑦：M3 案例素材；语义与论文一致，无偏差） |
| 负载均衡 / 滚动更新 / 跨进程调用 | 滚动更新原语 = loader apply 的 provider 切换（新增 fiber 注册 → 激活 → 卸载旧）；流量权重调整与跨进程 RPC 无实现 | M2/M3 场景（PLAN 里程碑表已列，非 M1 范围） |

### §6.3 Access Control and Sandboxing（访问控制与沙箱）

| 论文段落 | 实现证据 | 对照 |
|---|---|---|
| 依赖声明即能力请求；组件只能访问声明过的依赖，未声明访问报错；代理 = 能力中介 | wasm guest 能力面 = import 面（wit 世界仅 `context` 接口）；`Host::get` 只读镜像——镜像 = inject 键同步 + 自身 set（`sync_injected`，未声明键读 = None，**结构性强制**）；`set_dyn` 的 Def 43/48 纪律（越界写未声明键 → panic = bug）；native 侧读任意 realm 为义务不检查（M0 走查已记录） | **对应**（wasm 侧结构性强制强于论文义务表述；native 侧边界已记录） |
| 能力静态可知 → 加载时审查 | inject/provide 为静态导出（跨边界调用核对，`load_guest.rs` 断言） | **对应** |
| 拦截机制细粒度策略（Def 30：元数据咨询、可运行时装/卸/改不触发 reload） | interception 求值形态**未实现**（仅元数据累积；`get` 不消费 ι）——M0 走查处置清单① | **缺口**（承 M0 清单①，M2 首批任务；非 M1 新增） |
| 沙箱化不可信组件：执行边界（SFI/独立运行时/沙箱进程/容器）；桥接透明性；宿主侧桥接是普通 fiber、能力可衰减 | wasmtime = 软件故障隔离（guest 内存/执行隔离）；`sandbox_isolation.rs`：恶意 guest（step panic → trap）被捕获、**宿主进程存活**（可继续实例化/驱动正常组件）——门禁 2/3 实测；WasmComponent 是普通 fiber（实现 `Component`，loader 兼容）；桥接透明性 = 镜像同步（wasm 消费 wasm/原生/Go 断言同构：dependency_consumption/dual_backend/go_guest） | **对应**（门禁 2/3 达成；与论文 "software fault isolation" + "bridge is an ordinary fiber" 一致） |
| （补查风险点→已收官）恶意 guest 调 `context::set` 写未声明键 → 核心 `set_dyn` panic!（宿主 Rust panic，非 wasm trap） | **M2-PR1（PR #17）**：桥接层把越界写（纪律 panic 载荷）与 AlreadyBound 统一转 `FiberError::raise` → fiber 失败 outcome（`Inactive(Some(ζ))`、宿主存活、is_quiet）；可信原生组件的越界写仍为 panic = bug（宿主不变式，保留） | **新增记录（⑧）→ 已落地**（PR #17：语义处置随 ⑤ 收官） |

### §6.4 Language Independence and Selection（语言无关性）

| 论文段落 | 实现证据 | 对照 |
|---|---|---|
| 时间可组合性：闭包——可逆效应 = 动作 + 逆成对、逆作为值捕获（含恢复状态）、teardown 重放 | `Disposer = Box<dyn FnOnce>` 命令式载体（M0 走查⑥：与论文 `g: Γ→Γ` 纯函数的结构差异已记录）；wasm 边界逆句柄化（`inverse` 资源 + rep 表 + 核心逆经 rep 执行，`StepGuard` 幂等） | **对应**（结构差异为既有记录） |
| 模块引入/撤回随执行模型：**WebAssembly 视 embedder——原生 embedder 下宿主丢弃时释放（如 Wasmtime）** | `WasmComponent::load`（`Component::from_file`）+ `InstanceState` 随 `Rc` 释放（Store 丢弃）；loader 移除条目 → retire 级联清理 | **对应**（与论文原文 "released when a native embedder drops it (e.g., Wasmtime)" 直接一致） |
| 加载建模为对上下文的效应，逆撤销模块引入的注册 | 注册 = ctx 上可逆效应（Def 47：应用 = refresh 启动生命周期；逆 = O-Retire）；Thm 63 级联停用测试 | **对应** |
| 空间可组合性 · 类型层：上下文类型记录每键共效应；Rust trait/typeclass 扩展 | Rust：`Key`/`Symbol` 类型化 `get`/`set`（键级类型参数）；`cordis-macro` 的 `#[component(inject=[..], provide=[..])]` 过程宏 | **对应**（论文明确描述 "Rust procedural macros ... emits, for each dependency, a typed declaration together with such an accessor" = 我们的宏路径） |
| 空间可组合性 · 运行时层：动态中介（键背后共效应随加载/卸载变化、跨上下文解析不同）；透明拦截原语或反射 | 核心 = 符号级 store + `resolve_realm` + notify/reload 级联（动态解析）；wasm 桥接 = 镜像同步 + 宿主驱动 step（拦截点在宿主） | **对应**（运行时符号级动态解析；**新增记录（⑨）**：非编译期类型化 DI——typed world（按插件家族静态类型化接口）留 M2 评估时重审该边界） |
| 语言独立性最小支撑 | Go guest：裸 wit 绑定 + 手写资源方法（无元编程层）、`Value` v1 标量集——与 Rust guest 同语义互通（门禁 3/3 实测） | **对应**（实证：机制与保证全在宿主，与作者语言无关） |

### 处置清单（M1 走查产物；承 M0 清单 + 本次新增）

| # | 处置项 | 来源 | 去向 |
|---|---|---|---|
| ① | interception 求值形态（Def 30/31） | M0 清单① | **部分落地（M2-PR2，PR #18）**：元数据侧（`get_meta` + `declared_metadata` + `intercept_in_place`）；provider 函数形态（σ(k): ℳ→𝒱）随⑨ typed world 评估 |
| ② | §5.1.4 Proxy 访问层（Alg 6） | M0 清单② | **已落地（M2-PR6，PR #22）**：`Context::resolve`（链上行解析：committed 授权 / inject 未提交 → Inactive / root 无声明 → Undeclared；读视图非裸 store——Thm 63 语义）+ `AccessError` + 4 测试 |
| ③ | Thm 59/61 直接测试 | M0 清单③ | **已落地**（PR #15：`tests/preservation_recovery.rs`——Thm 59 良构四条款逐动作断言 + Thm 61 交错/反向退役恢复精确性） |
| ④ | Thm 66 定量上界 `(K+4)(V+1)` 断言 | M0 清单④ | **已落地**（PR #16：`tests/progress_bound.rs`——K=5、V=6 受控场景：效应步总数 ≤ (K+4)(V+1) 且 ≤ (K+1)×安装期数（紧界）、转换次数 ≤ (K+4)(V+1)、每阶段 `is_quiet`（Thm 66(1)）；引擎侧精确生命周期步计数需步计数器，记录为 M2 可选增强） |
| ⑤ | L-Raise 失败模型 + `is_quiet` ζ 析取（含 ⑧ 处置） | M0 清单⑤ + 本次 ⑧ | **已落地（M2-PR1，PR #17）**：`FiberError::raise` + `reload` 捕获 → `Inactive(Some(ζ))` + 已完成步骤恢复 + ζ 析取；`inverse.run` 的 guest 调用仍为协议违反 panic（宿主不变式，保留）；**直证测试**：`tests/failure_model.rs`（2）+ `sandbox_isolation.rs` 升级（trap/越界 set → 失败 outcome、宿主存活、fiber 失败态、is_quiet） |
| ⑥ | 命令式 Disposer 结构对齐（wasm 句柄化已部分对齐） | M0 清单⑥ | **评估完成（M2-PR6）**：保留命令式 `Box<dyn FnOnce>`——语义由 Thm 7/16/20/21 命令式测试保证，wasm 边界逆句柄化（inverse 资源 + rep 表）已对齐论文形态；纯函数 `g: Γ→Γ` 的价值在形式化侧，代码无可表达结构——关闭为记录（M3/形式化阶段再议） |
| ⑦ | §6.2 broker 示例（可表达性演示） | 本次新增 | **已落地（M3-PR1，PR #24，含审查重排）**：`im-bot` broker 示例——broker（中央服务）provide 注册句柄（`reg`）+ service 入口；后备**注入**注册句柄、经**可逆效应**（`ctx.effect`）注册（卸载自动撤销）；消费者注入 service、请求由 broker 分发（字典序最小者优先）。直证：更新/卸载后备 → broker 与消费者**保持 Active、效应不重执行、service 绑定不变**（仅路由集移除该后备）；重注册自动恢复 |
| ⑧ | 恶意 guest 触发宿主 panic（越界 set → 核心 panic!）的边界——**M2-PR1（PR #17）收官**：不再依赖 catch_unwind 兜底——桥接 `forward_pending` 把越界写（核心纪律 panic 载荷）与绑定冲突（AlreadyBound）统一转为 `FiberError::raise` → 失败 outcome（fiber 失败态、宿主存活）；测试 `guest_undeclared_set_becomes_error_outcome_and_host_survives` | 本次新增（§6.3 补查风险点） | **已落地**（PR #17：语义处置随 ⑤ 一并收官） |
| ⑨ | 运行时符号级动态解析 vs 编译期类型化 DI | 本次新增 | **评估完成（M2-PR6）**：保持通用 `context` 接口 + 符号级动态值 API——§6.4 的"运行时动态中介"正是论文要求（"dependency access must be dynamically mediated"）；结构检查由宿主侧注入键核对承担（load_guest 断言）；typed world（import 段即 inject 规格）列为 DX 增强（M3 或按需）——关闭为记录 |
| M2 加载器 + HMR | 2026-08-17 | §5.2（§5.2.1/5.2.2） | **门禁判定：通过（含处置清单）**——§5.2.1 声明式配置：Def 74 全字段条目（id/url/isolate/intercept/config/disabled）落地 + 按字段最小扰动分派（url/revision 重建、**intercept 就地**、**isolate = Algorithm 7 realm 重指派**、disabled 卸载/重载）+ 配置树（group/include 分支条目、子列表 keyed diff 递归、组持有者 fiber = π 语义）+ 托管 realm（local/global + delimiter→树成员等价适应）；已知边界：双向写回未实现（条目权威）、组条目 isolate 不应用。§5.2.2 HMR：Alg 8/9/10 三阶段落地（分类不动点/过期检测/事务性重载 + 回滚——不进入半重载）；模块图 = HashMapGraph/WasmLeafGraph（cargo metadata/wit 解析为生产化适配器，边界记录）；门禁用例直证：保存即生效 + 其他组件状态保留 + 双回滚。**本次走查处置**：处置清单 ① ② ③ ④ ⑤ ⑥ ⑧ ⑨ 全部落地/评估收尾（⑦ broker 示例归 M3）；新增记录：⑩ 双向写回未实现 ⑪ 组条目 isolate 不应用 ⑫ 模块图生产化适配器 | 处置清单（下里程碑首批任务）：⑦ §6.2 broker 示例（M3 案例素材）；⑩ 双向写回（组件→条目方向，M3 评估）；⑪ 组条目 isolate 注解（M3 或 typed world 时）；⑫ cargo metadata/wit 模块图适配器（M3 或按需） |
| M3 案例验证 | 2026-08-17 | §5.3（+ §6.2 服务代理、§5.1.2/§5.3 基准量化） | **门禁判定：通过（含处置清单收口）**——逐条对照无未解释偏差（详见下方「M3 走查记录」）：§5.3 Koishi 案例形态级复现（IM adapter 提供 platform / 数据库驱动提供存储 / 功能插件声明为共效应并访问）——运行时重配置（切换存储后端 / 重连 adapter）只重激活解析依赖变化的依赖者（bot 重激活 fiber 不变、无关组件不受影响），依赖不可用保持 Inactive 直到出现不报错，跨独立作者代码经共效应键组合一致；时间组合（卸载单插件效果不需重启宿主、自动逆组合无需手写卸载路径、HMR 保存生效）经 loader/cordis-hmr 直证；§6.2 服务代理（处置⑦ broker 落地，审查重排依赖方向后闭环）；§5.3 "future work" 的量化测量由 M3-PR2 bench 补充（notify 扫描线性直证 / 传播净成本 / loader O(N²) 协调总账）。**本次走查处置**：处置清单 ⑦ 落地、⑩⑪⑫ 评估收口（PR #24/#26）；新增记录：bench 已知边界 ①②③（loader desired-diff O(N²) 索引优化、激活/teardown 序列残差、CI 余量校准） | 无未解释偏差（范围说明 2 项：协作规模 4000+ 插件不可复现、浏览器第二运行时未复现——均以形态/载体级证据对应，非偏差） |


## M2 走查记录（2026-08-17，PR #23）

> 程序（PLAN §7）：重读 §5.2 → 逐条核对已知偏差 → 补查映射 → 走查记录 + 处置清单。
> 稳定态确认：`cargo test --workspace` 30 二进制全绿、clippy/fmt 门禁干净、API 冻结（M2 边界）。

### §5.2.1 Declarative Configuration（声明式配置）

| 论文段落 | 实现证据 | 对照 |
|---|---|---|
| Def 74 条目全字段（id/url/isolate/intercept/config/disabled）；双向绑定（loader 响应条目变更调整 fiber；组件改配置/禁自身写回条目） | `Entry`（id/component=url/isolate/intercept/config/revision/disabled/children）；per-field dispatch（M2-PR3/4） | **对应**（组件→条目写回方向缺席——**新增记录⑩**：条目为权威记录，组件不能自改配置/禁自身） |
| 协调：Theorem 73（quiescent 状态只由最终配置决定）+ Cor 62（离场无残留）+ Thm 66（必静止）支撑增量协调 | 两阶段递归协调（每层先卸载侧释放供给名、再实例化侧——同供给替换单次 apply）；keyed diff 幸存子条目不重建 | **对应**（property 测试 + loader 测试实证） |
| per-field dispatch：id/url → 重建；isolate → Algorithm 7；intercept → 就地；config → 组件自决；disabled → 卸载/重载 | url/revision 变更 → 重建；isolate 变更 → `patch_isolation`（Algorithm 7，不重建）；intercept 变更 → `intercept_set_boxed`/`intercept_clear` 就地（读时咨询、不触发 reload）；disabled → 拆除/重装；config 变更经 revision 重建（协调键代行组件级 diff——M0 已记录） | **对应**（config 交由组件自决的语义由协调键承担，M0 记录） |
| 配置树 + group/include（@cordisjs/group 子列表 keyed diff；include 外部配置嫁接） | `Entry::group`/`include` 分支条目；组持有者 fiber（空组件，π = 组 fiber，Def 47）；子列表 keyed diff 递归；组拆除自底向上 | **对应**（include 与 group 结构相同——外部配置文件解析由编排方承担） |
| 托管 realm：local（按 entry id 打标、随迁）/ global（命名共享）；realm 无条目引用时丢弃；Algorithm 7 delimiter 重指派 | `IsolateAnnotation`（Local/Global）；实例化期经派生 ctx 应用；`patch_isolation`（Δ 键 → patch ρ → refresh → 移动子树绑定 → affected 通知） | **对应**（**适应记录**：delimiter own 判定 = loader 树子树成员关系等价；ρ 拷贝继承 → patch 遍历子树） |
| intercept 更新就地、读时咨询 | `intercept_set_boxed`（条目注解权威、重放幂等）+ `intercept_clear` + `get_meta` 读路径合并 | **对应**（**边界⑤**：移除注解不回退父（组）继承值——扁平拷贝语义） |

### §5.2.2 Hot Module Replacement（HMR）

| 论文段落 | 实现证据 | 对照 |
|---|---|---|
| HMR 无需开发者标注的验收边界（fiber 界定全部效应/共效应） | fiber 生命周期 + 可逆效应（L-Raise 失败模型提供重载失败的精确检测） | **对应** |
| Phase 1 分类（Alg 8）：stashed/externals 种子不动点 | `classify`（自 stashed 向下传播、环默认拒绝；Alg 9 的树 ∩ accepted 负责上游） | **对应** |
| Phase 2 过期检测（Alg 9）：依赖树 ∩ accepted、declined 边界 | `detect`/`get_dependencies` | **对应** |
| Phase 3 事务性重载（Alg 10）：backup → dispose + use → 失败 restore + 重建 | `Hmr::reload`（备份 = loader 注册表取出旧组件；重载 = 注册新模块 + revision 递增重建仅过期条目；失败检测 = 加载错误 + fiber 失败态（L-Raise）；回滚 = 恢复旧组件 + 重新 apply） | **对应**（缓存 = loader 注册表——invalidate/backup 的等价实现） |
| get_imports：native cargo metadata 依赖图 / wasm wit import 图 | `ModuleGraph` trait：`HashMapGraph`（数据驱动）+ `WasmLeafGraph`（M1 wasm 插件仅导入宿主 context 接口 = 叶子） | **边界记录⑫**（cargo metadata/wit 解析为生产化适配器） |

### 处置清单（M2 走查产物；承 M1 清单 + 本次新增）

| # | 处置项 | 去向 |
|---|---|---|
| ⑦ | §6.2 broker 示例（可表达性演示） | M3 案例素材 |
| ⑩ | 双向写回（组件→条目方向） | **重开并部分落地（G1，PR #29）**：M3-PR3 评估收口后被 TS 参考实现推翻（TS `Fiber.update` + `internal/update` 写回证明 §5.2.1 "runs in both directions" 是 loader 契约）——core `Fiber::update`（就地重跑、fiber 身份保留、依赖者级联、失败 = L-Raise；**失败态可经 update 复活**——REVIEW-97bb598 major-1 采纳 TS `_error = undefined` 语义）+ `Runtime::set_update_hook`（观察者先于重跑触发、不参与 L-Raise 通道）+ loader `update_entry`/`entry_config`/`register_update_hook`（fiber→条目反查递归写回）。**self-dispose → 条目 `disabled` 写回也已落地（PR #30，TS `internal/plugin` 半段）**：`Fiber::retire` 触发退役观察者，loader 过滤「条目仍在且未 disabled」（= 组件自退役）写回书签；apply 期间 teardown 的 retire 延迟到协调后排空；`loader.fiber(id)` 对退役 fiber 返回 None；desired 显式 `disabled=false` 重新启用（disabled 为协调字段）。退役粘滞语义（无观察者时）不变；同 revision apply 不清除 fiber 层写回（书签回映 desired，协调记录非权威源） |
| ⑪ | 组条目 isolate 注解 | **已收口（G3，PR #31）**：per-key isolate 落地后组条目 isolate 经派生链**拷贝继承**给子条目（组 ctx 重定向被 derive 继承）、子条目注解**覆盖**（最近注解优先）——此前记录的"继承至子树"候选语义自然实现；组 isolate 变更仍整棵重建（保守路径，直证测试）。无需 effective-isolate 穿透（拷贝继承免去 patch 复杂度） |
| ⑫ | cargo metadata / wit 模块图适配器 | **评估完成（M3-PR3，PR #26）**：算法 crate 无 TOML/JSON 解析器依赖（hmr 仅 anyhow 错误处理；serde 为 wasmtime 传递依赖不可用）；`HashMapGraph` 已证明算法数据驱动（适配器仅换数据源）；适配器随 typed world/构建工具 crate 落地（届时允许 serde_json/toml），公开差异关闭 |

### Algorithm 6 适应记录（REVIEW-e8bd96e nit1）

`resolve` 的授权读取经授权 fiber 的**当前** `ρ` 解析 realm（committed 视图
只存 `key → provider`、不含 realm 快照）——Algorithm 7 重指派（isolate
变更）的瞬态窗口内，授权 fiber 的 ρ 已更新而绑定仍在迁移中，`resolve`
可能读到旧 realm 的绑定或 NotBound（映射 `Inactive`）。同步引擎中重指派
与访问不可并发（单线程、调用链内原子），漂移仅影响"重指派进行中的
同步调用链"——记录为已知边界（精确判别留待 typed world / 快照 realm）。

## M3 走查记录（2026-08-17，PR #27）

> 程序（PLAN §7）：重读 §5.3 → 逐条核对实现/测试/bench 证据 → 补查映射 → 走查记录 + 门禁判定。
> 稳定态确认：`cargo test --workspace` 全绿——118 个 `#[test]` 函数（`grep -rc '^\s*#\[test\]' crates examples` 计数）、33 条 `test result: ok` 摘要行（每个测试可执行文件 / doc-test 一条）、0 `FAILED`（REVIEW-567a770 nit1 修正口径）；clippy/fmt 门禁干净；三 PR（#24 im-bot 案例、#25 bench、#26 处置评估）审查闭环。

### §5.3 Case Study: Koishi（案例研究）

| 论文段落 | 实现证据 | 对照 |
|---|---|---|
| 规模与代表性：4000+ 社区插件、IM 适配器/数据库驱动/控制台/终端用户功能 | 无法复现协作规模；形态级验证（三层拓扑 + 运行时操作）+ bench 量化补充（M3-PR2） | **范围说明**（非偏差：论文为存在性结论，本仓库以可复现迷你案例验证其形态） |
| 表达性：一切功能 = §5.1 上下文原语之上的插件，宿主只贡献领域词汇 | im-bot 的 adapter/database/bot 均为 `#[component]` 普通组件（inject/provide 键声明），无宿主特判；broker（§6.2 服务代理）同型 | **对应**（`examples/im-bot/src/main.rs`、`bin/broker.rs`） |
| 通用性：同一模型在**第二运行时**（web console）重现；原语固定、含义留给应用 | 仓库跨语言/载体演示：Rust + Go 双语言 guest 在同一 loader 互通（M1-PR14）、wasm 沙箱载体（M1）——原语语义固定、各应用自行解释 | **对应**（载体级证据：浏览器运行时未复现，以 wasm 载体 + 双语言 guest 作通用性观察） |
| 时间组合①：卸载单插件效果不需要重启宿主 | loader `disabled` 切换 → 整棵卸载级联 + 可逆效应自动撤销（`disabled_toggle_unloads_and_reloads`；broker 场景 3 卸载后备 = 注册逆自动执行、仅撤路由集） | **对应** |
| 时间组合②：上下文中介效果被追踪、逆自动组合 → 插件作者无需手写卸载路径（locality of concern） | im-bot/broker 各组件 `apply_impl` 均无手写 dispose 逻辑——撤销全部经 `ctx.set` 绑定逆 / `ctx.effect` 逆自动执行（broker 后备注册逆、三层绑定逆）；`retired_component_persists_across_unchanged_apply` 钉死退役粘滞与条目权威 | **对应**（直接论据：无卸载路径的前提下效果被自动撤回） |
| 时间组合③：HMR 保存生效、保留缓存状态与存活连接 | cordis-hmr Alg 8/9/10 事务重载 + 双回滚；`hmr_reload_applies_new_version_keeping_other_components`（其他组件状态保留 = 连接/状态不丢）（M2 门禁） | **对应**（M2 已走查） |
| 空间组合拓扑：IM adapter 提供 platform、数据库驱动提供存储、功能插件声明为共效应并访问 | im-bot 三层：`PlatformKey`（adapter）/ `DbKey`（database）/ bot 注入两者、提供 `ReplyKey`；bot 经 `ctx.get` 访问两层 | **对应**（main.rs） |
| 运行时重配置：切换存储后端 / 重连 adapter → 只重激活解析依赖变化的依赖者（§3.2） | main.rs 场景 1/2（bot 重激活、fiber 不变；adapter/database 不受影响）；bench 场景 B ExecCount 直证 bot 恰 1 次、adapter 0 次、与 M 无关；同供给键替换双路径（同条目 revision 重建 + 跨条目替换测试） | **对应**（§3.2 重激活局部性实证） |
| 依赖不可用 → 保持 inactive 直到出现、不报错（"stays inactive until it appears, without erroring"） | main.rs 场景 3：移除 adapter → bot `Inactive`（不 panic/不报错）；adapter 重现 → 自动激活 | **对应** |
| 跨独立作者代码组合一致（只协调共效应键） | im-bot 三组件独立定义（三个 struct/impl，无相互引用），仅经键连接；broker 后备/broker/消费者仅经 `RegKey`/`ServiceKey` 键连接 | **对应** |
| Threats to validity：存在性/采纳性结论；量化测量 = future work | M3-PR2 bench 报告对本实现作量化补充（notify 扫描线性直证 / 传播净成本 / loader 协调 O(N²) 总账），明确论文未主张定量（M3-BENCH.md §1） | **记录**（论文留作 future work 的量化，本报告作出补充） |

### 门禁判定

| 门禁项 | 证据 | 判定 |
|---|---|---|
| 三层拓扑案例全断言（adapter/database/功能插件；切换后端/重连/依赖不可用） | `cargo run -p im-bot`（M3-PR1，REVIEW-d1263fa 闭环） | **通过** |
| §6.2 broker 示例（处置⑦ 落地） | `cargo run -p im-bot --bin broker`（M3-PR1，审查重排依赖方向后闭环） | **通过** |
| bench 报告产出（notify 扇出、切换延迟） | `docs/bench/M3-BENCH.md` + `cargo run -p im-bot --bin bench`（M3-PR2，三层分离测量 + REVIEW-bbb252a 闭环） | **通过** |
| 处置⑩⑪⑫ 评估收尾 | 退役粘滞语义测试 + 已知边界文档 + THEORY-MAP 处置行（M3-PR3，REVIEW-d457b60 闭环） | **通过** |
| 走查 §5.3 无未解释偏差 | 上表逐条对照；范围说明 2 项（协作规模、浏览器第二运行时）均已注明非偏差 | **通过** |

**M3 门禁判定：通过（5/5）**——案例、broker、bench、处置评估、走查 §5.3 五门禁全部达成；处置清单（⑦ 落地、⑩⑪⑫ 评估收口）全部闭环；无未解释偏差（2 项范围说明非偏差）。
