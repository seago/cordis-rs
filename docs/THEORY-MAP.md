# THEORY-MAP：论文符号 ↔ 代码映射与偏差记录

> 活文档。每个 PR 合入时回填映射、记录偏差；里程碑走查时逐条处置（PLAN §7）。
> 论文：`paper/paper.pdf` — *A Programming Paradigm for Spatiotemporal Composability*（Shi / Zhang / Cui，2026）

## 符号 ↔ 代码映射

| 论文符号 / 术语 | 章节 | 代码（crate::path） | 测试 | 备注 |
|---|---|---|---|---|
| `Γ∞`（统一上下文类型） | Def 32 | 待填 | | |
| `γ ∈ Γ`（上下文状态） | Def 32 | 待填 | | |
| `𝔈Γ` / `𝔈iter_Γ`（效应函数/迭代器） | Def 8/51 | 待填 | | |
| `effectΓ(𝑒)` / `ctx.effect` | Def 12, Alg 1 | 待填 | | |
| `Σ` / `Σiso` / `Σinter`（共效应上下文） | Def 22/28/30 | 待填 | | |
| `get` / `set` | Def 23, Alg 2 | 待填 | | |
| `isolate` / `intercept` | Def 29/31 | 待填 | | |
| `notify`（分类通知） | Def 26, Alg 3 | 待填 | | |
| 组件 `(d, p, e)` | Def 43 | 待填 | | |
| fiber `⟨d, p, e, π, σ, τ, θ⟩` | Def 44 | 待填 | | |
| `dom(𝐹𝛾)`（registry） | Def 45 | 待填 | | |
| `𝜏`（retirement）/ `𝜋`（parent） | Def 44 | 待填 | | |
| `𝜔`（committed view） | Def 44/46 | 待填 | | |
| `target_n(γ)` / 静止判定 | Def 46 | 待填 | | |
| `ΘΓ`（生命周期状态） | Def 49 | 待填 | | |
| `recover` / accumulator `g` | Def 6, Alg 1 | 待填 | | |
| `relied_n(γ)`（撤离 guard） | Def 50 | 待填 | | |
| `O-Insert` / `O-Retire` / `O-Remove` | Def 47, §4.2 | 待填 | | |
| `L-*` 生命周期规则 | §4.2–4.3 | 待填 | | |
| `use`（组件实例化） | Alg 4 | 待填 | | |
| `refresh` / `reload` / `unload` | Alg 5 | 待填 | | |
| 配置 Entry | Def 74 | 待填 | | |

## 定理覆盖

| 定理 / 结论 | 测试位置 | 状态 |
|---|---|---|
| Thm 7 / Thm 16：LIFO 恢复、声音不变量 | | 未开始 |
| Cor 21：独立效应乱序撤销 | | 未开始 |
| Thm 63：依赖者先停、teardown 可读依赖 | | 未开始 |
| Thm 64：单转换不跨两次解析 | | 未开始 |
| Thm 66：Progress、guard 不死锁 | | 未开始 |
| Thm 73 / Cor 62：Confluence、离场无残留 | | 未开始 |
| Def 26：通知分类正确性 | | 未开始 |

## 已知偏差

> 每 PR 合入时追加；里程碑走查逐条处置：**修正 / ADR 保留 / 公开差异声明**。

| 日期 / PR | 偏差描述 | 论文依据 | 处置 | 状态 |

## 里程碑走查记录

| 里程碑 | 日期 | 覆盖章节 | 结论 | 未决偏差 |
|---|---|---|---|---|
