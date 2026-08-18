# Phase 1（P1.1）出口走查报告：REVIEW-PHASE1-EXIT

- **走查对象**：`docs/cordis-events-PHASE1-EXIT.md`（提交 ae81266）——事件验收 #1–#9 + 集成 + 门禁 + 出口判定。
- **走查日期**：2026-08-19
- **走查人**：independent-review-agent（出口走查专职）
- **范围**：EXIT 文档 ↔ 代码真实一致（`crates/cordis-events/src/lib.rs` 642 行 + `tests/{events.rs, m15.rs}`）逐条核验；测试数核准；零依赖复核。
- **前置**：里程碑 M1.1–M1.5 各自独立审查已 PASS（REVIEW-85d2379 / -a0963ab / -f8541f1 / -866407c / -b6ebd25）。

---

## 总体结论

✅ **PASS（出口成立）** —— EXIT 文档的验收映射**无夸大、无遗漏**：测试数与测试名与代码一一对应，17/17 实际全绿；语义真实性核验通过（无掩耳盗铃）。发现 3 项 Nit（均不阻塞，1 项措辞精化建议 + 2 项可选增强）。

- **Major**：0
- **Minor**：0
- **Nit**：3

## 验收映射逐条核实（EXIT §1 ↔ 代码）

| # | EXIT 声称 | 实测 | 判定 |
|---|---|---|---|
| 1 | emit 序（emit_order_and_payload） | 存在且 ok | ✅ |
| 2 | disposer 幂等 + 双路径 armed（disposer_idempotent_double_dispose + m13::manually_dispose_and_ctx_dispose_all_share_armed） | 双测试存在且 ok；实现：`on`/`subscribe` 的 disposer 与 ctx.effect 逆**共享同一 `Arc<AtomicBool>` fence**（lib.rs:175-196, 511-525, 569-573）——双路径至多一次，语义真实 | ✅ |
| 3 | 卸载自动退订（m13::subscribe_auto_unsubscribes_on_fiber_retire） | Subscriber 组件 apply 内 `subscribe(&ctx,…)` 落 fiber ctx 累加器（events.rs:264-303）；退役后 emit 不达（done:2 缺席）——真实直证 | ✅ |
| 4 | waterfall 链/短路（waterfall_basic_link_order + m14::waterfall_short_circuit） | 基础链 A→B→terminal + A around（110/序断言）；短路 A 不调 next → B/terminal 缺席（log=只有 A）——真实 | ✅ |
| 5 | serial/bail（serial_collects_all_and_bail_stops_on_first_some） | serial 收集 [5,7]；bail 首个 Some(9) 停 b3 未调；全 None → None —— 真实 | ✅ |
| 6 | 四冲突 should_panic（conflicts::*） | same_name_different_payload / same_mode_different_r / cross_mode_different_payload / dispatch_r_mismatch——各带 `expected` 消息匹配（符号冲突 / R 单一性 / 跨模式载荷冲突 / 派发 R 不符）；实现 register lib.rs:303-344 四规则齐全 + check_dispatch_r lib.rs:465-477 | ✅ |
| 7 | 重入快照：注册本轮跳过（dispatch_register…）+ 退订跳过 | 注册重入直证（本轮 A2 缺席、下一轮 A1:2+A2:2，注册序）；**退订跳过** = 实现四派发 alive 调用期检查（emit lib.rs:378-381 / waterfall waterfall_link lib.rs:500-512 / serial lib.rs:434 / bail lib.rs:450）+ #2 直证——诚实（无专门单测，见 Nit-3） | ✅ |
| 8 | 空集 E-2（empty_listeners_e2） | emit no-op / waterfall 仅 terminal（v=99）/ serial 空 vec / bail None —— 四断言真实 | ✅ |
| 9 | Send+Sync 编译断言（m15::send_sync_compile_assert） | `assert_send_sync::<EventBus/Arc<EventBus>/EmitListener<u32>>()` —— 真实编译期断言（Send+Sync 闭包上界下通过） | ✅ |

**测试数核准**：`cargo test -p cordis-events` 实际 = `events.rs` 14 + `m15.rs` 3 = **17/17**，与 EXIT「17/17」**精确一致**（events.rs 的 `grep '#[test]'` 计数 18 系 conflicts 4 例各带 `#[test]+#[should_panic]` 两行标注，非测试数——测试数已用 cargo 输出核准）。

## 集成与衔接（EXIT §2）

| 项 | 实测 | 判定 |
|---|---|---|
| async 监听器投递（m15::async_listener_delivery_via_spawn_local） | sync 闭包内 `spawn_local` 投递、同步段立即执行 + async 任务完成断言（async:7）——投递模式真实演示（C-5 可追溯措辞见 Nit-1） | ✅ |
| loader 集成（m15::events_provider_mounts_via_loader） | `Loader` 挂 `EventsProvider` 根条目 → `ctx.get::<EventsKey>` 可达 → 订阅/emit 回路（loader:3）→ teardown —— 真实 | ✅ |
| 活变化通道（§4.1） | 订阅即效应（M1.3）= 通道落地形态；async 消费方留 Phase 1 后 —— 按草案范围 | ✅ |
| scope 模式（§4.3） | 草案仅给接入面，无代码 —— 按草案范围 | ✅ |

## 零依赖与门禁（EXIT §3）

- `cargo tree -p cordis-events`：run 分支仅 `cordis-core` ✅；dev-deps = cordis-loader + tokio（M1.5 集成测试用）——符合计划 §5「零第三方（tokio/async 只进 dev-deps）」。
- `cargo test -p cordis-events` 17/17 全绿（本次走查实测）。
- fmt/clippy/doc 由各里程碑审查已备查（本走查未重跑，抽查 test/tree 足够）。

## 发现

### Nit-1（建议精化）：EXIT「async 监听器任务可追溯（C-5）」措辞略强于实测模式

- **位置**：EXIT §2 async 行「任务可追溯」；`m15.rs` 的 `tokio::task::spawn_local(...)` 的 JoinHandle 未保留（仅演示投递，disposer 亦演示性保留，测试自注 REVIEW-b6ebd25 nit-1）。
- **问题**：async 草案 C-5「任务可从 fiber 尾巴/注册表句柄到达、无裸 spawn」是 **async 层契约**；events 层零依赖、async 投递只是**消费方使用模式**。严格说本节演示丢弃了 JoinHandle（不满足 C-5 字面可追溯）。**不影响任何功能**（events 只保证 sync 派发 + 订阅管理；async 侧任务可追溯性由消费方 async 层/async crate 落实），但 EXIT 措辞可更精确。
- **建议**：EXIT §2 async 行措辞改为「异步投递模式演示（spawn_local 投递、不阻塞派发）；C-5 可追溯性由消费方 async 层落实（events 零依赖不强制）」。

### Nit-2（可选增强）：无「派发中途退订 · 已入快照者本轮跳过」专门单测

- **位置**：#7（EXIT 记为「退订跳过 = #2 直证 + 四派发 alive 调用期检查实现」）。
- **问题**：注册重入有专属测试；「派发中退订已入快照者本轮跳过」依赖实现（emit/serial/bail 调用期 alive 检查 + waterfall_link 跳过）与 #2 推理共同支撑——**诚实但无单一专属断言**（预期的「本轮跳过、后续不再触发」中"后续不再触发"由 #2 直证，"本轮跳过"由实现保证）。
- **建议**（可选）：补一条用例——监听器 A 在执行器第一次 emit 后置其 disposer（`d()`），同轮 emit 中后续监听器 B 对 A 的触发……（注：单线程 sync 派发中 A 在**自己的调用**期间无法退订**自己之后**的段落，实际可测形态为 waterfall/串行中退订下一项）——当前证据链已充分，此增强非必须。

### Nit-3（git 卫生）：`0fc7d93` 与 `de0a9d7` 两 commit 同主题重复（疑似未 squash 的中间态）

- **位置**：git log M1.3 段——两 commit 描述皆为「M1.3 审查 nit-2/3 落地（REVIEW-f8541f1）」，前者多 "dir disposer"、后者多 "（clippy 干净）"；diff `0fc7d93..de0a9d7` 显示 lib.rs -14 行 + REVIEW-f8541f1 入库——说明前者为中间态、后者覆盖。
- **问题**：历史冗余（同一变动两个 commit，本可用 `--amend`/`squash` 合并），不影响内容正确性（当前 HEAD 状态已由 de0a9d7 定稿且全绿）。
- **建议**（可选）：后续类似探索性中间提交优先 `--amend`；当前历史已固化，可留档或未来整理时说明，**不阻塞出口**。

## 结论

**P1.1（cordis-events）出口成立。** EXIT 文档与代码真实一致：验收 #1–#9 映射逐条命中、17/17 测试全绿、四派发语义/双路径 armed/冲突四规则/E-2/重入快照无掩耳盗铃；async 投递与 loader 集成真实演示；零第三方（run 仅 cordis-core）符合纪律；里程碑审查闭环（M1.1–M1.5 全 PASS，0 Major/0 Minor 未决）。3 项 Nit（C-5 措辞精化建议、可选单测增强、git 卫生）不阻塞。

**照准进入 P1.2–P1.4 决策**（按计划 §3 后续线，各线开工由用户下达）。
