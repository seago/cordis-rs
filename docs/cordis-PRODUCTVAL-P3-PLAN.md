# 产品验证线 P-3 详细计划 —— Await 生产化

**依据**：B 计划（core `Step::Await` + `Runtime::advance`，REVIEW-8589ca2 / REVIEW-dbc2384）；REVIEW-dbc2384 n-1（`is_suspended` 仅测试消费——宿主统一挂起集/批量 advance/状态上报留 A3/后续）；B 提案 §2（判据形态 v2 泛化）；REVIEW-8589ca2 m-2（advance guard `target.is_some()` 与 `update_fiber` 更新路径交互复核）；P-5 依赖（agent 插件 await 形态底座——先 P-3 更顺，REVIEW-PHASE2-PROPOSAL m-2）。
**状态**：**草案——待开工指令（含 core 授权点 §4）**。
**保证**：Gate A/B 同前；commit 分 code/docs；**core 改动 = 本线一次性授权额度（§4 确认后）**；全回归绿（core/loader/events/hmr/async/wasm）。

---

## 0. 目标

把 Await 机制从"测试可用的原语"升级为**宿主可编排的生产能力**：
1. **挂起集**：Runtime 登记所有挂起于 Await 的 fiber（可枚举、可上报）；
2. **批量恢复**：宿主对挂起集统一驱动（判据满足 → 批量 advance）；
3. **判据 v2 评估**：Await 无载荷 + 外部判据（当前）是否需要泛化登记判据；
4. **advance guard 复核**（REVIEW-8589ca2 m-2）与 `update_fiber` 更新路径交互。

## 1. 设计

### 1.1 挂起登记与查询（core，P3-1）
- `Runtime` 增 `suspended: RefCell<HashSet<FiberId>>`：
  - **登记**：激活/恢复路径遇 `Step::Await` 挂起（runtime 475 挂起分支 + `advance` 再次挂起分支）→ 插入；
  - **撤销**：`advance` 恢复完成 / `unload` 收账残留 / `update` 前置收账 → 移除；
  - **查询**：`Runtime::suspended_fibers() -> Vec<FiberId>`（快照；宿主轮询/上报用）。
- 现有 `Fiber::is_suspended` 保持（单 fiber 查询）。

### 1.2 批量恢复辅助（core，P3-1）
- `Runtime::advance_suspended(judge: impl Fn(FiberId) -> bool)`：对挂起集逐 fiber 判据检查 → 满足则 `advance`（组合线程调用；judge 由宿主提供——wasm 桥侧即"该组件的 remote 结果就绪"）。
- 语义：单线程 push 保持（ADR-0002）；advance 仍 panic=bug（未挂起不适用——批量版只对挂起集成员）。

### 1.3 判据 v2 评估（P3-2）
- 现状：Await 无载荷 + 判据在调用方（advance 前外部满足）——**已覆盖 wasm 回填场景**（poll_remotes 后 advance）。
- v2 候选：挂起时登记"就绪谓词"（Runtime 侧 poll 自动恢复）——**评估**：引入自动轮询会改变单线程语义（谁在何时 poll？）——倾向**不启用**（保持外部判据 + `advance_suspended` 显式驱动）；评估结论入 EXIT（记录"判据 v2 不启用：外部判据 + 显式批量驱动已覆盖；自动轮询留待真实场景"）。

### 1.4 advance guard 复核（P3-2）
- 复核点：advance guard = `target.is_some()`（弱于激活 `==guard_target`）与 `update_fiber`（reload/update 路径）交互——更新路径经 `unload` 先收账 resumable（已静态确认安全）；**补测试直证**：挂起中 `update_fiber` → 残留逆收账 + 新代激活正确 + 挂起集清理；guard 结论写入 EXIT（保持 `is_some()` 或收紧——按测试结果定）。

### 1.5 wasm 侧统一驱动（P3-3，可选）
- `WasmComponent` 提供宿主辅助：对"挂起中的本组件 fiber"统一 `poll_remotes` → `runtime.advance_suspended(就绪判据)`（封装"回填→恢复"回路）——agent 插件（P-5）直接可用；评估后在 P3-3 落地（量小）。

## 2. 分步

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| P3-1 | 挂起集登记/查询 + `advance_suspended` 批量恢复（core）+ 单测（登记/撤销/批量） | 开工 + §4 授权 | 1–2 天 |
| P3-2 | 判据 v2 评估（结论记录）+ advance guard 复核（update 交互测试 + 结论） | P3-1 | 1 天 |
| P3-3 | wasm 侧统一驱动辅助（poll→advance 回路封装）+ 端到端（挂起中恢复回路） | P3-2 | 0.5–1 天 |
| P3-4 | 出口（门禁全绿 + EXIT + 走查） | P3-3 | 0.5 天 |

全程约 3–4.5 天（含审查）。

## 3. 验收

- 挂起登记/撤销正确（挂起→登记；advance 完成→撤销；unload/update→收账+撤销）；
- `suspended_fibers()` 快照与 `is_suspended` 一致；
- `advance_suspended` 批量恢复直证（多 fiber 挂起 + 判据分批满足 → 逐批恢复）；
- update 挂起交互测试绿（残留逆收账、新代激活、挂起集无残留）；
- wasm 统一驱动回路端到端（复用 a2_e2e 形态扩展）；
- 全回归（core/loader/events/hmr/async/wasm）+ workspace 无回归。

## 4. 决策点（开工前确认）

1. **core 授权**（REVIEW-PHASE2-PROPOSAL nit-2 单独立案）：本线允许 `cordis-core` 改动 = §1.1/§1.2 范围（suspended 集 + 查询 + 批量 advance）——**一次性授权、不扩面**；THEORY-MAP 标注（P-3 授权行）；
2. **判据 v2**：不启用（外部判据 + 显式批量驱动；评估记录）——确认；
3. **wasm 统一驱动**（P3-3）：做（agent 插件底座）——确认。

## 5. 风险

- core 挂起集与既有路径（L-Raise 失败分支、guard 中断、update）的登记/撤销遗漏 → 测试全覆盖 + 断言（挂起集与 resumable 一致性）。
- `advance_suspended` 的 judge 误用（非组合线程调用）→ panic=bug 同既有纪律。
- 零第三方保持。

## 6. 纪律

Gate A/B 同前；commit 分 code/docs；core 改动额度=§4 授权；THEORY-MAP 记录；全回归绿。
