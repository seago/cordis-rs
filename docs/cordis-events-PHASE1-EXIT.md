# cordis-events Phase 1（P1.1）出口判定

**依据**：草案 `docs/cordis-events-protocol-draft.md` v0.3.1（冻结）；计划 `docs/cordis-events-PHASE1-PLAN.md`（P1.1 cordis-events 主交付线，Step 0–5）。
**判定日期**：2026-08-19（里程碑 M1.1–M1.5 全部审查闭环后，出口走查）。
**出口标准**（计划 §0/§4）：events 验收 #1–#9 全过 + async/loader 集成全绿 + workspace 无回归 + 出口走查（无未解释偏差）→ 进入 P1.2–P1.4 决策。

---

## 1. 验收测试清单（草案 §6）对照

| # | §6 条目 | 测试（crates/cordis-events/tests/） | 状态 |
|---|---|---|---|
| 1 | emit 序与载荷 | `events.rs::emit_order_and_payload` | ✅ |
| 2 | disposer 幂等（自研 armed 同款；双路径 Rust 落地） | `events.rs::disposer_idempotent_double_dispose` + `m13::manually_dispose_and_ctx_dispose_all_share_armed` | ✅ |
| 3 | 卸载自动退订（spike S1 固化） | `m13::subscribe_auto_unsubscribes_on_fiber_retire` | ✅ |
| 4 | waterfall 链（A→B→terminal 序 + around + 短路） | `waterfall_basic_link_order` + `m14::waterfall_short_circuit_skips_downstream_and_terminal` | ✅ |
| 5 | serial / bail（收集序 / 首个 Some 即停 / 全 None） | `serial_collects_all_and_bail_stops_on_first_some` | ✅ |
| 6 | 符号冲突：同名异载荷 / 同模式异 R / 跨模式异载荷 / 派发 R 不符 | `conflicts::*`（4 用例，各 `#[should_panic]` + 类型名诊断） | ✅ |
| 7 | 重入快照：派发中注册本轮不触发、下一轮触发；退订者本轮跳过 | `m14::dispatch_register_this_round_skipped_next_round_fires`（退订跳过 = #2 直证 + 四派发 alive 调用期检查实现，REVIEW-a0963ab/866407c） | ✅ |
| 8 | 空集语义（E-2 四断言） | `empty_listeners_e2` | ✅ |
| 9 | Send+Sync 编译断言 | `m15.rs::send_sync_compile_assert` | ✅ |

**验收 #1–#9 全过**（另有 listener Arc 捕获义务直证、waterfall 基础链、async/loader 集成——共 17/17 测试）。

## 2. 集成与衔接（草案 §4）

| 项 | 落地 | 状态 |
|---|---|---|
| async 监听器投递（§4.1 / async C-5） | `m15::async_listener_delivery_via_spawn_local`——sync 闭包内 `spawn_local` 投递、不阻塞派发、任务可追溯 | ✅ |
| 活变化通道（§4.1，与 C-1' 快照互补） | 订阅即效应（M1.3）即该通道的落地形态；async 层消费方（Phase 1 后） | ✅（语义对齐） |
| loader 集成（§4.2） | `m15::events_provider_mounts_via_loader`——`EventsProvider` 根条目挂载、总线可达、订阅/emit 回路、teardown 零污染 | ✅ |
| scope 模式（§4.3） | 草案仅给接入面（realm 隔离实例），无代码；app 层后续 | ✅（按草案范围） |

## 3. 门禁与回归记录

- `cargo +1.97.0 fmt --check` ✅（workspace）
- `cargo +1.97.0 clippy --workspace --all-targets -- -D warnings` ✅ 0 告警
- `cargo +1.97.0 doc -p cordis-events --no-deps` ✅ 0 告警
- `cargo +1.97.0 test --workspace` ✅ 无回归（events 17 条 + 既有 cordis-async 21 / core / loader / hmr / wasm 全绿）
- 零第三方：`cargo tree -p cordis-events` run 分支仅 `cordis-core`；tokio/cordis-loader 在 dev-dependencies（计划 §5）
- 里程碑审查闭环：M1.1–M1.5 全部独立审查 PASS（REVIEW-85d2379 / -a0963ab / -f8541f1 / -866407c / -b6ebd25），0 Masajor / 0 Minor 未决

## 4. 出口判定

**P1.1 cordis-events 全部完成**：验收 #1–#9 全过、集成全绿、门禁全绿、里程碑审查闭环。

→ **进入 P1.2–P1.4 决策**（计划 §3 后续线）：P1.2 AsyncRuntime 完善（auto-settle O-2 / AsyncFiberHandle 门面收口 / observer hook O-3 / Failed 富化 O-4）｜P1.3 Remote 扩展 + 双运行时收口｜P1.4 DX 文档。各线开工按纪律由用户下达。
