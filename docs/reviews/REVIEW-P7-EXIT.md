# P-7（错误策略 O-1/O-4 联动）审查 + 出口走查

**审查对象**：63140ca（O-1 升级）/ 1decd3d + a3201bf（O-4 定案）/ 5d58144（EXIT）——产品验证线 P-7。
**审查日期**：2026-08-22。**手段**：静态阅读 + 抽测。

---

## 总体结论：✅ PASS —— P-7 达成、出口成立（0 Major / 0 Minor / 2 Nit）

### 核查命中（逐点）

**O-1 升级（63140ca）**：
- core `context.rs` 越界写 **4 处** `panic!` → `FiberError::new(...).raise()`（组件失败通道）——reload 的 catch_unwind 识别 FiberError 载荷 → `Inactive(ζ)` + 已完成步骤恢复 + 可复活（与 wasm 桥 `forward_pending` 侧已 raise 的处置**对齐**，安全深度对称）✓
- 既有测试迁移语义正确：`set_outside_provision_panics` → `set_outside_provision_fails_component`（should_panic → `Inactive(Some(ζ))` 断言 + `is_quiet`）；loader 验收 #8 `supply_discipline_panic_kept` → `supply_discipline_oob_fails_component`（越界写 → 组件失败态断言 + 宿主不崩 + 静止）✓
- THEORY-MAP P-7 授权行标注 ✓；其余纪律（元数据冲突/调用方违约/内部一致性）panic 保持（未波及）✓

**O-4 定案（1decd3d）**：
- 双通道分工**诚实且直证**：`hmr_failure_reports_and_emits_entry_failed`——HMR reload 失败（Clash）→ 回滚（既有语义）+ **报告面 = 最近一次 apply 状态**（`report.ok()`——失败尝试报告被回滚后的成功 apply 覆盖）+ **失败通知 = 事件通道**（`register_entry_failed_hook` → `loader/entry-failed` → 订阅收到"供给冲突"）——两通道分工在回滚场景下定案为"事件通道呈现失败、报告面为最近干净状态"（`cordis-ERRORS-QUIET.md` §3bis 记录）✓
- `cordis-events` 为 hmr dev-dep（零第三方 run-deps 保持）✓

**门禁与抽测**：
- `cordis-core --lib` **60/60**、`cordis-loader` **58/58**、`cordis-hmr` **10/10**（含新 O-4 直证）——与任务声明的门禁数字精确一致 ✓
- 父会话已验 workspace 无回归 + clippy/fmt/doc 0（抽测三 crate 全绿佐证）

### Nit（记录，不阻塞）
- n-1：`hmr_failure_reports_and_emits_entry_failed` 内联定义 `EntryFailed` 事件（与 error_bridge 的 `EntryFailed` 重复——各测试独立定义，可抽公共但无碍）；
- n-2：O-4 定案"报告面=最近状态"的语义（失败尝试报告被覆盖）依赖"回滚后 apply 是干净的成功状态"——该前提在 HMR 回滚语义下成立（回滚恢复旧版本并成功 apply），测试直证。

## 出口判定

**P-7 达成且出口成立**：O-1（native 越界写升级 ComponentFailure，与 wasm 对齐——第三方插件安全深度闭环）+ O-4（HMR 失败双通道定案，诚实分工 + 直证）全部落地、测试迁移语义正确、门禁全绿、审查闭环（0 Major/0 Minor 未决）。**产品验证线 P-1..P-5+P-7 全部收官，仅剩 P-6（§6.6 版本化依赖）**。
