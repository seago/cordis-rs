# 产品验证线 P-7 详细计划 —— 错误策略 O-1/O-4 联动

**依据**：错误策略草案 v0.2 §10 开放问题（O-1：native 供给纪律越界写 → ComponentFailure 升级——"wasm guest 已 raise，native 保持 bug；待插件生态真实场景再定"；O-4：HMR 失败经报告面还是事件通道——"待 HMR 首个真实失败场景"）；REVIEW-PHASE2-PROPOSAL 遗漏扫描（P-7 与 P-5 产品 spike 联动后具实质价值——**P-5 已提供真实插件生态形态**，两个"待场景"前提已满足）。
**状态**：**草案——待开工指令（含 core 授权点 §3）**。
**保证**：Gate A/B 同前；commit 分 code/docs；全回归绿；**O-1 触 core（授权额度）**；零第三方。

---

## 0. 目标

把错误策略线两个"待真实场景"的开放项闭环（P-5 agent 插件生态 = 真实场景）：
1. **O-1 升级**：native 组件供给纪律越界写从 `panic=bug` 升级为 **ComponentFailure**（L-Raise → `Inactive(ζ)` + 可复活）——与 wasm guest 对齐（第三方插件安全深度）；
2. **O-4 定案**：HMR 失败呈现 = **报告面为主（`Loader::report()`）+ `entry-failed` 事件通道为辅**（两通道并存，定案记录）。

## 1. 子项 P7-1：O-1 native 越界写升级（core 授权）

### 现状
- core `context.rs` 供给纪律越界写 4 处 `panic!`（Def 43/48 纪律——作者义务 = bug）；wasm 桥在 `forward_pending` 侧已把越界转 `FiberError::raise`（失败模型）——**同一纪律、两个后端不同处置**（安全深度不对称）。
- 错误策略验收 #8（`supply_discipline_panic_kept`）断言 native 越界 panic——O-1 升级后该断言语义变化。

### 设计
- core `context.rs` 越界写 4 处：`panic!` → `FiberError::raise(...)`（组件失败通道——reload 的 catch_unwind 识别 → `Inactive(ζ)` + 已完成步骤恢复 + 可复活）。
- **语义变化**：native 组件越界 = 组件失败（可诊断、可复活、不拖垮宿主）——与 wasm 对齐；"作者义务"收窄为其余纪律（元数据冲突、调用方违约等保持 panic）。
- **回归影响**：错误策略验收 #8 改写（越界 → FailedFiber/Inactive(ζ) 断言）；既有 core/wasm 测试中依赖越界 panic 的断言适配。
- **测试**：native 组件越界 → fiber `Inactive(Some(ζ))` + 依赖者级联停用 + 可复活（update/重启用）；wasm 路径回归不变。

## 2. 子项 P7-2：O-4 HMR 报告面定案

### 设计
- **定案**：HMR reload 失败经 **报告面（`Loader::report()` 逐条目）+ `entry-failed` 事件通道** 双呈现——报告面为准（结构化诊断），事件通道为通知（HMR 场景的 entry-failed 发射）。
- 现状已备：reload 失败回滚（错误策略线）+ `register_entry_failed_hook`（E2）+ `entry-failed` 事件桥（error_bridge）——P7-2 补 **HMR 场景直证**（reload 失败 → report().failed() 呈现 + entry-failed 事件收到）。
- **测试**：HMR reload 失败（Clash 场景）→ 回滚 + report 呈现 + entry-failed 事件断言。
- **文档**：`cordis-ERRORS-QUIET.md`/HMR doc 补 O-4 定案记录。

## 3. 决策点（开工前确认）

1. **O-1 升级（core 授权）**：core `context.rs` 越界写 panic→raise（授权额度 = 4 处越界写；其余纪律 panic 保持）；THEORY-MAP 标注——确认升级（默认：P-5 真实场景已满足 O-1 前提）；
2. **验收 #8 语义**：越界写归 ComponentFailure 后 #8 改写（"panic 边界"测试改为"组件失败"断言）——确认；
3. **O-4 双通道定案**：报告面 + 事件通道并存——确认。

## 4. 分步与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| P7-1 | O-1 升级（core 4 处 raise + 测试迁移 + 新验收） | 开工 + §3 授权 | 1–2 天 |
| P7-2 | O-4 定案（HMR 报告面+事件直证 + 文档） | P7-1 | 0.5–1 天 |
| 出口 | 门禁全绿 + EXIT（O-1/O-4 闭环记录）+ 走查 | 以上 | 0.5 天 |

全程约 2–3.5 天（含审查）。

## 5. 风险

- O-1 升级波及面：core 纪律语义变化（越界写从"宿主不变式"变"组件失败"）——既有依赖 panic 的测试全量扫描（grep should_panic 越界断言）；loader/events/hmr/async 回归。
- 复活路径：越界失败后复活（重启用）不再越界则恢复——直证。
- 零第三方；wasm 路径零变化（已 raise）。

## 6. 纪律

Gate A/B 同前；commit 分 code/docs；core 改动=授权额度；THEORY-MAP 标注；全回归绿。
