# 产品验证线 P-3 出口判定 —— Await 生产化

**依据**：计划 `docs/cordis-PRODUCTVAL-P3-PLAN.md`（core 授权 §4 已确认）；审查 REVIEW-e8e9b8c（P3-1 PASS）/ REVIEW-7833761（P3-2/P3-3 PASS）。
**判定日期**：2026-08-22。

## 1. 交付与验收

- **P3-1（core，授权额度）**：`Runtime.suspended` 挂起集（登记：激活挂起分支/advance 再挂起；撤销：advance 取走/unload 收账）+ `suspended_fibers()` 查询 + `advance_suspended(judge)` 批量恢复 + 单测（两 fiber 登记/judge 分批/全量清空/挂起中退役撤销/与 is_suspended 一致性）——THEORY-MAP P-3 授权标注。
- **P3-2**：**advance guard 复核**（REVIEW-8589ca2 m-2）——挂起中 `update_fiber` 交互直证（unload 收账 → 惯性 reload 新代 → 挂起集重登记；guard=`target.is_some()` 弱化在更新路径下安全，结论成立）；**判据 v2 评估（结论）**：**不启用**——Await 无载荷 + 外部判据 + `advance_suspended` 显式批量驱动已覆盖现有场景（wasm 回填）；自动轮询（Runtime 侧 poll）会引入"谁在何时 poll"的单线程语义问题，留待真实场景再议。
- **P3-3**：`WasmComponent::poll_and_advance`（poll 回填 → 批量恢复）——agent 插件（P-5）await 驱动底座；端到端 `poll_and_advance_drives_suspend_loop`（挂起 → 回路驱动 → guest 自取完成）。
- **门禁**：core 60/60、wasm 全套绿（a2_e2e 3/3 含回路）、clippy/fmt/doc 0、workspace 无回归、零第三方。

## 2. 记录

- core 改动 = P3-1 授权额度（P3-2 仅测试）；THEORY-MAP P-3 授权行 + B-A1 行重组修复（REVIEW-e8e9b8c Minor-1）。
- Nit 记录：poll_and_advance 多组件未就绪时强制 advance 再挂起（无害，doc 注明）；4000×1ms 环路上限与既有时序模式一致。

## 3. 出口判定

**P-3 完成**：挂起集生产化（枚举/上报/批量恢复）+ advance guard 复核 + wasm 统一驱动回路 + 判据 v2 评估记录 + 全回归绿 + 审查闭环（0 Major 未决）。→ 下一线 **P-4（go ABI 同步自动化）**，计划按纪律起草待用户确认。
