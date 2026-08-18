# cordis-async Phase 0 出口判定

**依据**：`docs/cordis-async-protocol-draft.md` v1.4（冻结）§9；执行计划 `docs/cordis-async-PHASE0-PLAN.md`。
**判定日期**：2026-08-18（里程碑 M0.1–M0.6 全部审查关闭后）。
**出口标准**（草案 §9）：11 条协议单测 + 3 项 spike 全部通过 → 进入 Phase 1 决策；任一失败 → 回架构决策表重审 C。

---

## 1. 协议单测清单（草案 §9）对照

| # | §9 条目 | 测试（tests/protocol.rs） | 状态 |
|---|---|---|---|
| 1 | I-1：三步效应 + 复合逆 LIFO await 序 | `i1_composite_disposer_runs_lifo` | ✅ |
| 2 | I-2：长 await 中途目标变更（retire），在途步完成、逆入账 | `i2_guard_false_at_step_boundary_keeps_inflight_step` / `i2_guard_false_immediately_yields_empty_composite` / `i2_guard_flips_while_inflight_step_pending` | ✅ |
| 3 | I-3：退役提供者，消费者 async 逆 settle 先于提供者（日志序直证） | `m03::i3_dependent_async_inverse_settles_first` | ✅ |
| 4 | I-4：Failed 后 settle 恒完成、is_quiet 真；disabled 写回；重启用复活 | `m04::i4_failed_settles_quiet_writeback_and_revive` | ✅ |
| 5 | drain 重入：逆中注册新效应入下一代被排空；自再生触发守卫 panic | `m03::drain_reentry_next_generation_is_drained` / `m03::drain_self_regeneration_triggers_guard_panic` | ✅ |
| 6 | panic 隔离：async 效应 panic 不进失败通道、不级联、进程存活 | `m07::async_panic_is_isolated_not_failed_not_cascaded` | ✅ |
| 7 | 快照纪律（评审点 C）：提供者卸载后，依赖者尾巴从步创建时捕获的 Arc 读值 | `m07::dependent_tail_reads_captured_snapshot_after_provider_gone` | ✅ |
| 8 | 代次与更新（评审点 E）：update → 旧代 cancel + 新代 spawn，旧尾巴 settle | `m05::update_bumps_generation_and_settles_old_tail` | ✅ |
| 9 | 无环关停（评审点 B）：shutdown 后 AsyncRuntime 可 drop（Weak 计数） | `m05::retired_settled_runtime_releases_no_cycle` | ✅ |
| 10 | H 竞态（评审点 H）：drive 恰在 cancel 后、settle 前完成 Ok → 共享槽恰一次 take；Failed slot 留空；shutdown 补 enqueue 收账 | `m05::h_race_slot_taken_exactly_once_and_failed_slot_empty` / `m05::shutdown_settles_inflight_tail` | ✅ |
| 11 | shutdown 一致性（C-7）：编排方退役 → 双真；未退役 → 违约捕获；退役零配置污染 | `m04::shutdown_after_orchestrator_retire_is_double_quiet` / `m04::shutdown_without_orchestrator_retire_panics` | ✅ |

**协议单测合计：11/11 全过**（另含 M0.6 Remote 桥 2 条与 M0.3 附加用例，protocol.rs 18 条全绿）。

## 2. Spike 清单（草案 §9）对照

| spike | 假设 | 测试（tests/spikes.rs） | 通过标准 | 状态 |
|---|---|---|---|---|
| S1 | 事件总线 DX 税可承受（订阅随 fiber 卸载自动退订） | `spike_s1_event_bus_subscription_auto_unsubscribes_on_unload` | 订阅/退订原型跑通；DX 形态 = 一行 `cx.effect`（注：spike 直证单 fiber 订阅/退订；级联退订共享同一 `dispose_all` teardown 路径，其级联序已由测试 3/I-3 覆盖——REVIEW-68f0c80 nit-1） | ✅ |
| S2 | 组合线程二分不别扭（tokio 服务 sync 壳） | `spike_s2_tokio_service_sync_shell_via_spawn_remote` | 同步壳 + 远端调用回路跑通（mock LLM 往返 + 卸载收账） | ✅ |
| S3 | agent loop 注册器模式三端协作完整 | `spike_s3_agent_loop_flushes_session_on_unload` | mock SSE 流 + 工具调用 + 卸载 cancel → 检查点退出 → flush session（await 收尾）、无泄漏 | ✅ |

**Spike 合计：3/3 全过**。

## 3. 门禁与回归记录

- `cargo +1.97.0 fmt --check` ✅（workspace）
- `cargo +1.97.0 clippy --workspace --all-targets -- -D warnings` ✅ 0 告警
- `cargo +1.97.0 doc -p cordis-async --no-deps` ✅ 0 告警
- `cargo +1.97.0 test --workspace` ✅ 全绿无回归（cordis-async 21 条 = protocol 18 + spikes 3；core 55+28、loader 49、hmr 9、wasm 全套）
- 里程碑审查闭环：M0.1–M0.6 全部独立审查 PASS（REVIEW-1005c8b / -91254a9 / -83c254a / -596125d / -23383f3 / -4f1e555），无未决 Major/Minor

## 4. 出口判定

**Phase 0 全部完成**：11 条协议单测 + 3 项 spike 全过，工程门禁全绿，里程碑间审查门禁全部关闭。

→ **进入 Phase 1 决策**（计划 §0/§6 预备路线）：`cordis-events` crate（类型化事件名 + 四种派发，S1 原型已跑通订阅/退订形态）、AsyncRuntime 完善（block_on 入口、AsyncFiberHandle 门面收口）、Remote 桥扩展（Send future 分池形态、WasmRemote 接入 M1 协议）、DX 文档与示例。S1 的 `AsyncCx::effect` 订阅入口、S3 的注册器模式与检查点退出语义已获原型验证，可直接作为 Phase 1 组件的规格输入。
