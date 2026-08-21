# 产品验证线 P-6 详细计划 —— §6.6 版本化依赖 + 小项 ②③

**依据**：用户拍板（P-6 = ① §6.6 版本化依赖，顺带 ② Progress 定量上界补测、③ loader O(N²) 索引优化）；论文 §6.6（Dependency Typing and Versioning：interface drift / key collision 两问题 + 三方法方向）与 §4.4.4（Thm 66：`S(n) ≤ (K+4)(V(n)+1)`，K = 每 fiber 效应迭代长度上界，B(n) = (K+4)(2+Σ_{m≺n}B(m)) 递归）；THEORY-MAP 46 行（Progress 定量上界未断言缺口）；bench M3-BENCH 已知边界①（loader 阶段一 `desired.iter().rev().find()` 逆扫 → O(N²)）。
**状态**：**草案——待开工指令**（按顺序在 P-7 之后执行；内容随用户可调）。
**保证**：Gate A/B 同前；commit 分 code/docs；**零 core 改动**（版本化在 loader 层键编码；Progress 补测仅测试；索引优化在 loader）。

---

## 0. 目标

论文 §6.6 指出 coeffect 链接只有名义链接（按键名），无版本/结构链接 → **接口漂移**（提供者改接口、消费者旧声明仍匹配）与**键碰撞**（无关提供者同键名）两个问题。P-6 落地**最语言无关的组合**：键命名空间化 + 版本化链接（运行时校验），把"静默错/不可诊断"转为可诊断报告；并顺带完成 Progress 定量上界断言与 loader 索引优化。

## 1. 子项 P6-1：版本化依赖（§6.6 落地）

### 设计
- **键命名空间化**：依赖/提供声明支持 `name@namespace`（如 `db@cordis/db`）——键碰撞隔离（提供者显式声明命名空间，消费者按命名空间匹配）；命名空间作为 Symbol 编码的一部分（`name@ns` 即一个键，core 无感）。
- **版本化链接**：声明 `key@version`（提供者提供版本、消费者声明约束）——loader 解析时校验（v1：精确版本匹配；可选约束区间留 v2）；**不匹配 → `OrchestrationError` 报告**（复用错误策略线通道：`EntryErrorKind` 增 `VersionMismatch` 或复用 ConfigValidation 类），不静默绑定。
- **范围**：Entry 的 inject/provide 元数据带版本字段（`Entry` 增可选 `version` 注释或键内编码——优先键内编码 `key@1.2`，避免 Entry API 大改）；loader 解析校验 + 报告；插件指南文档更新。
- **论文对齐**：验收动机 = §6.6 两问题（drift → 版本校验报错；collision → 命名空间隔离）。

### 任务
1. 键内编码 `name@ns@ver`（或 `name@ns` + 独立版本字段——实现时定稿）；
2. loader 解析校验（提供者声明 vs 消费者约束）+ 报告通道（错误策略 `EntryErrorKind` 扩展）；
3. 测试：版本匹配成功 / 漂移报错（drift 场景）/ 命名空间碰撞隔离（collision 场景）；
4. 文档：`cordis-PLUGIN-GUIDE.md` 版本化链接章节。

## 2. 子项 P6-2：Progress 定量上界补测（Thm 66）

- **现状**：THEORY-MAP 46 行——progress（到达静止）已验，定量上界 `(K+4)(V+1)` **未断言**（覆盖缺口）。
- **补测**：property 测试——构造满足假设的拓扑（≺ 无环、每组件迭代长度 ≤ K、依赖链深 d）→ 驱动到静止 → 断言总步数 ≤ Σ B(n)（B(n) = (K+4)(2+Σ_{m≺n}B(m))，按链深展开）；小拓扑（链深 0/1/2 + K=1/2）逐例断言精确界。
- **文档**：THEORY-MAP 46 行更新（缺口 → 已补测）。

## 3. 子项 P6-3：loader desired-diff 索引化

- **现状**：`apply_into` 阶段一 `desired.iter().rev().find(|e| e.id == id)` 逐条目逆扫 → O(N²)（bench 已知边界①）。
- **优化**：apply 前构建 `HashMap<id, &Entry>`（或按 id 索引 desired）→ 阶段一查表 O(1)/条目 → 整体 O(N)；**语义零变化**（last-wins 语义保持——索引按"desired 中该 id 的最后一项"预计算，与逆扫 find 等值）。
- **验证**：既有 loader 测试全绿 + bench 场景 B 重测（可选）+ 新增"同 id 多出现 last-wins 保持"断言。

## 4. 分步与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| P6-1 | 版本化依赖（编码+校验+报告+测试+文档） | 开工指令 | 2–3 天 |
| P6-2 | Progress 上界补测 + THEORY-MAP 更新 | P6-1 | 0.5–1 天 |
| P6-3 | loader 索引优化 + 回归 | P6-1 | 0.5–1 天 |
| 出口 | 门禁全绿 + EXIT（`cordis-PRODUCTVAL-P6-EXIT.md`）+ 走查 | 以上 | 0.5 天 |

全程约 3.5–5 天（含审查）。

## 5. 风险

- **键编码形态**（`name@ns@ver` 内编码 vs Entry 字段）：实现时定稿——内编码零 API 破坏（键即 Symbol）；若选 Entry 字段则 loader API 扩展（backward 兼容）。
- **版本校验语义**：v1 精确匹配（避免 semver 范围复杂度）；语义不匹配 → 报告（错误策略通道，不 panic、不静默）——与判定公理一致。
- 零 core 改动（版本校验在 loader 层；Symbol 编码 core 无感）。

## 6. 纪律

Gate A/B 同前；commit 分 code/docs；零 core；loader 语义零变化（P6-3 以既有测试全绿为护栏）。
