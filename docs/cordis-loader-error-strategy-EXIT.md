# cordis-loader 错误策略线出口判定

**依据**：草案 `docs/cordis-rs-error-strategy-draft.md` v0.2（冻结）；计划 `docs/cordis-loader-error-strategy-PLAN.md`（E0–E3）；评审闭环 `docs/CORDIS-ERROR-STRATEGY-REVIEW.md`（v0.1→v0.2）。
**判定日期**：2026-08-19（E0–E2 审查闭环 + E3 出口走查）。
**出口标准**（计划 §4 / 草案 §9）：验收 #1–#9 全过（含既有 should_panic 迁移、panic 边界护栏 #8）+ 报告面/事件衔接绿 + workspace 无回归 + 出口走查（无未解释偏差）。

---

## 1. 验收测试清单（草案 §9）对照

| # | 条目 | 测试 | 状态 |
|---|---|---|---|
| 1 | 单条目校验失败不中断（叶子） | `config_validate_failure_reports_not_panic`（失败 Failed、其余 Activated、不崩） | ✅ |
| 2 | 组条目校验失败（整组未挂载、子不实例化） | `group_config_validation_failure_reports` | ✅ |
| 3 | 重试与复活（每次 apply 重试；修配置 bump 复活 Activated） | `failed_entry_retries_each_apply_and_revives` | ✅ |
| 4 | ProvisionClash first-wins（keys 全列、owner） | `provision_clash_first_wins_reports` | ✅ |
| 5 | 未知组件（报告、继续、每次重试） | `unknown_component_reports_not_panic` | ✅ |
| 6 | Display 契约（EntryError/ApplyReport 含 id+键/组件名） | `display_carries_three_elements` / `apply_report_display_snapshot` | ✅ |
| 7 | 同键替换不误报 Clash | `same_key_replace_reports_no_clash` | ✅ |
| 8 | panic 边界回归（供给纪律越界仍 panic） | `supply_discipline_panic_kept` | ✅ |
| 9 | 既有 should_panic 迁移 | `unknown_component`/`config_validate` 迁移为报告断言；HMR Clash 回滚测试迁移（hmr 9/9） | ✅ |

## 2. 交付物与语义落地

- **类型面（E0）**：`EntryError`/`EntryErrorKind`（四变体）+ Display 三要素；`EntryOutcome`/`ApplyReport`（failed()/ok()）/`EntryState`。
- **迁移（E1）**：`validate_config → Result`；instantiate_leaf/group → `Result<Rc<Fiber>, EntryError>`（ConfigValidation/UnknownComponent/ProvisionClash{keys,owner}/UnknownParent 不 panic）；`make_loaded → Result<Option<LoadedEntry>, EntryError>`（disabled→不推）；apply_into/reconcile_into 逐层 outcomes（新增 Activated / 未变 Unchanged / 失败 Failed / 重建 Activated）；`apply → ApplyReport` + `report()` 快照 + `entry_state(id)`；组失败整组未挂载子不实例化；未挂载失败每次 apply 重试（v0.2 决议）。
- **报告面 + events（E2）**：`EntryFailedHook` 注入（loader 零依赖 events）+ `loader/entry-failed` 桥接测试（error_bridge）。
- **HMR 下游适配（E1）**：reload 检查 apply 报告 `Failed`（OrchestrationError）→ 回滚（与 L-Raise 检查互补）；测试迁移。
- **核心零改动**（goed：`git diff` 不触 `crates/cordis-core`）；loader run-deps 仅 `cordis-core`（events 在 dev/tests）。

## 3. 门禁与回归记录

- `cargo +1.97.0 fmt --check` ✅ / `clippy --workspace --all-targets -- -D warnings` ✅ 0 告警 / `doc` ✅ 0 告警
- `cargo +1.97.0 test --workspace` ✅ 无回归（loader 58、events 14、hmr 9，既有 core/wasm 全绿）
- 里程碑审查闭环：E0（REVIEW-1f9d5e8）/ E1（REVIEW-9b61d82）/ E2（REVIEW-c0fb7c1）全部 PASS，0 Major / 0 Minor 未决

## 4. 出口判定

**错误策略线（loader OrchestrationError 迁移）全部完成**：验收 #1–#9 全过、报告面/事件衔接绿、panic 边界护栏、门禁全绿、审查闭环、core 零改动保持。

→ 后续取向：M1 wasm 桥专项（WasmRemote 宿主驱动）/ 更多产品假设 spike / Phase 2 决策——按纪律由用户下达。
