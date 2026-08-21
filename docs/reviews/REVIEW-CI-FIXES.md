# CI 修复回溯审查（异步测试时序加固补审）

- **审查对象**：`a427252`（spike_s2 轮询 64→512）、`4971466`（m06 spawn_remote 轮询补 1ms）、`edae960`（spike_s2 轮询补 1ms）
- **审查人**：independent-review-agent（回溯补审——原 3 个 CI 紧急修复直接推送）
- **日期**：2026-08-21

## 总体结论：✅ PASS（3 个修复合格；无未加固的同族遗留）

**发现：Major 0 / Minor 0 / Nit 1**（`spike_s2` 循环注释只降级为"轮询预算放宽"、未点明 1ms 的必要性——细微，非阻塞）。

## 逐 commit 核查

| commit | 内容 | 修复正确性 |
|---|---|---|
| `a427252` | spike_s2 64→512 | ✅ 预算放宽方向正确（初版只扩预算，见下条） |
| `4971466` | m06 spawn_remote 循环加 1ms sleep | ✅ 与 m06 `send_future` 既有先例（512×1ms）一致 |
| `edae960` | spike_s2 循环加 1ms sleep | ✅ 补齐 a427252 的缺口——**纯 yield 在 CI 0.00s 耗尽预算是根因**，1ms 给 worker 真实执行窗口；512ms 上限合理，无死锁 |

**正确性要点**：
- 模式统一为「512 × 1ms」；断言语义未变（`llm:ok:` / `["submit","joined"]` 日志序/回灌仍直证）；
- 单测内 1ms 线程 sleep 可接受（非组合线程之外副作用、单测私有；上限可预估）；无死锁（有界）；
- 修复链条清晰：先扩预算（a427252）耗尽仍不等 → 补 1ms（edae960）——两步到位，逻辑闭环。

## 同族未加固点扫描（重点）

`grep` 全部 `for _ in 0..64` / `0..50` / `0..8` 循环（cordis-async 测试），逐一判定**是否 await 真实外部时间（worker/远端回灌）**：

- **await worker 真实回灌的循环 → 已全部加固**：
  - m06 `spawn_remote_submits_to_worker_and_joins_back`（512×1ms ✅）
  - m06 `send_future_submits...`（先例 512×1ms ✅）
  - `spike_s2`（512×1ms ✅）
- **属"同步调度/组合线程内事件"的就绪条件轮询 → 非负载敏感，维持小预算合理**（头寸充裕，注释均写明"固定轮数 yield 头寸充裕、无 flaky 风险"）：
  - `0..64`：协议 380/710/977/1115/1639/1793、spikes 392（s3 agent loop）——等 fiber `Active`、`entry_disabled`、`log` 标志（同步 drive/自退役写回，非真实时间）；
  - `0..50`（180）、`0..8`（396/507/610/783/830）——幂等/同步小幅驱动。
- **边界件（良性）**：m06 `unload_during_pending_remote_join`（1433）用 512 **纯 yield**——但它等的是"submit"（join 在途的**同步落盘**）而非 worker 完成，预算足够；不等真实回灌，不属同族（若将来该测试要等 worker 完成再断言，才需补 1ms——当前无此需求）。

**同族未加固清单：无**（authenticates await-worker 回灌的循环均已 512×1ms；其余循环为同步事件，不受 CI 负载影响）。

## 结论

3 个 CI 修复合格、模式统一、无死锁、语义未变；同族（await worker 真实回灌）已全部加固，其余小预算循环均为同步调度非负载敏感。Nit-1（spike_s2 注释未明示 1ms必要性）可选改。
