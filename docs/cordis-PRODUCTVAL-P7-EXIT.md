# 产品验证线 P-7 出口判定 —— 错误策略 O-1/O-4 联动

**依据**：计划 `docs/cordis-PRODUCTVAL-P7-PLAN.md`（O-1 core 授权 + O-4 定案）；错误策略草案 v0.2 §10。
**判定日期**：2026-08-22。

## 1. 交付与验收

- **P7-1（O-1 升级，core 授权额度）**：`context.rs` 供给纪律越界写 4 处 `panic` → `FiberError::raise`——native 组件越界写归 **ComponentFailure**（`Inactive(ζ)` + 可复活，与 wasm 对齐；错误策略 O-1 闭环，P-5 插件生态真实场景前提满足）；其余纪律 panic 保持。测试迁移：core `set_outside_provision_panics` / loader 验收 #8 → 组件失败断言；THEORY-MAP P-7 授权标注。
- **P7-2（O-4 定案）**：HMR 失败呈现**双通道分工**——报告面 = 最近 apply 状态（回滚场景失败尝试报告被回滚覆盖）、事件通道 = 失败通知（`loader/entry-failed` 经 hook 发射）——`hmr_failure_reports_and_emits_entry_failed` 直证（回滚 + 报告面干净 + 事件收到 Clash）；`cordis-ERRORS-QUIET.md` 定案记录。
- **门禁**：core 60、loader 58、hmr 10（含新测试）、workspace 无回归、clippy/fmt/doc 0、零第三方。

## 2. 记录

- O-1 升级的语义变化：越界写从"宿主不变式（panic）"变"组件失败（可诊断/可复活）"——wasm 路径零变化（已 raise）。
- O-4 定案与计划措辞对齐：双通道分工（计划"报告面为主"修正为"事件通道承载回滚失败"——以直证为准）。

## 3. 出口判定

**P-7 完成**：O-1（native 越界写 ComponentFailure 升级）+ O-4（HMR 双通道定案）闭环 + 全回归绿 + 审查闭环。**产品验证线只剩 P-6（§6.6 版本化依赖）**——计划已起草（`docs/cordis-PRODUCTVAL-P6-PLAN.md`），按纪律待用户确认开工。
