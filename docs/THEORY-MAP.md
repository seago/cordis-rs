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
| 满足谓词 `σ⊧d` | Def 24 | `Store::satisfies` / `InterpState::satisfied` | 单元 | 完成（PR #2） |
| `Γ∞`（统一上下文类型） | Def 32 | `cordis_core::context::Context`（PR #3 最小版：store + 累加器） | 单元 | 最小版完成（PR #3）；realm/拦截投影 PR #4 |
| `𝔈Γ` / `𝔈iter_Γ`（效应函数/迭代器） | Def 8/51 | `cordis_core::effect::{Disposer, EffectIter, Step, once}` | 单元 | 完成（PR #3，同步版） |
| `effectΓ(𝑒)` / `ctx.effect` | Def 12, Alg 1 | `Context::effect` | 单元 | 完成（PR #3：armed + 组合入累加器） |
| `get` / `set`（共效应操作） | Def 23, Alg 2 | 待填（表操作已在 `Store`） | | 效应包装 PR #4 |
| `isolate` / `intercept` | Def 29/31 | 待填 | | PR #4 |
| `notify`（分类通知） | Def 26, Alg 3 | 待填 | | PR #4 |
| 组件 `(d, p, e)` | Def 43 | `interp::Component`（参考实现） | 单元 | 生产版 PR #5 |
| fiber `⟨d, p, e, π, σ, τ, θ⟩` | Def 44 | `interp::Fiber`（参考实现） | 单元 | 生产版 PR #5 |
| `n: 𝔑`（fiber 名） | Def 44/45 | `cordis_core::fiber::FiberId` | —（经 `interp` 间接覆盖） | 完成（PR #2） |
| `dom(𝐹𝛾)`（registry） | Def 45 | `interp::InterpState`（BTreeMap） | 单元 | 参考实现 |
| `target_n(γ)` / 静止判定 | Def 46 | `InterpState::target` / `is_quiet` | 单元 | 参考实现 |
| `ΘΓ`（生命周期状态） | Def 49 | `interp::Lifecycle`（两状态版） | 单元 | 参考实现；扩展版 PR #5 |
| `σγ` / `provider_k(γ)` | Def 45 式 (40) | `InterpState::provided` / `provider_of` | 单元 | 参考实现 |
| 支持集 / Lemma 70 | Def 67–70 | `InterpState::support_set` | 单元 | 参考实现 |
| `O-Insert` / `O-Retire` / `O-Remove` | §4.2 | `InterpState::insert` / `retire` / `remove` | 单元 | 参考实现 |
| `L-Reload` / `L-Unload` | §4.2 | `InterpState::reload` / `unload` | 单元 | 参考实现 |
| `recover` / accumulator `g` | Def 6, Alg 1 | `effect::execute`（LIFO 折叠）/ `Context::dispose_all` / `EffectHandle` | 单元 | 完成（PR #3） |
| `relied_n(γ)`（撤离 guard） | Def 50 | 待填 | | PR #5 |
| `use`（组件实例化） | Alg 4 | 待填 | | PR #5 |
| `refresh` / `reload` / `unload` | Alg 5 | 待填 | | PR #5 |
| 配置 Entry | Def 74 | 待填 | | PR #8 |

## 定理覆盖

| 定理 / 结论 | 测试位置 | 状态 |
|---|---|---|
| Thm 7 / Thm 16：LIFO 恢复、声音不变量 | `effect::tests` / `context::tests`（`execute_runs_inverses_in_lifo`、`thm16_*`、`accumulator_reverts_all_effects_lifo`、**`nested_effect_reverts_in_application_order`**） | 完成（PR #3；嵌套顺序审查后修复） |
| Cor 21：独立效应乱序撤销 | | 未开始（PR #3 后续，§3.3.2 就绪后） |
| Thm 63：依赖者先停、teardown 可读依赖 | | 未开始（PR #5–6） |
| Thm 64：单转换不跨两次解析 | | 未开始（PR #5–6） |
| Thm 66：Progress、guard 不死锁 | `interp::tests::drive_*`（参考解释器自检） | 参考实现已验（PR #2）；真实引擎 PR #6 |
| Thm 73 / Cor 62：Confluence、离场无残留 | `interp::tests::confluence_all_interleavings`（穷举交错） | 参考实现已验（PR #2）；真实引擎 PR #6 |
| Def 26：通知分类正确性 | | 未开始（PR #4） |

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

## 里程碑走查记录

| 里程碑 | 日期 | 覆盖章节 | 结论 | 未决偏差 |
|---|---|---|---|---|
