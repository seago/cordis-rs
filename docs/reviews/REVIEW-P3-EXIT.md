# P-3 出口走查报告

- **审查对象**：`docs/cordis-PRODUCTVAL-P3-EXIT.md`（Await 生产化出口判定）
- **审查日期**：2026-08-22
- **审查人**：independent-review-agent（对照仓库现行状态 + 抽测）
- **范围**：EXIT §1 交付/验收、§2 记录、§3 出口判定 ↔ 实现（P3-1 `e8e9b8c` / P3-2+P3-3 `7833761` / 审查落地 `3f1ccc9` / EXIT `bccbcc6`）

---

## 总体结论

✅ **PASS** —— P-3 出口成立（0 Major / 0 Minor / 0 Nit 阻塞）。EXIT 声明与实现逐点命中、门禁数字实测一致、判据 v2 结论诚实。

## 核实要点（EXIT ↔ 实现 ↔ 实测）

| EXIT §1 项 | 实现核实 | 实测 |
|---|---|---|
| P3-1 挂起集 | `Runtime.suspended: RefCell<HashSet<FiberId>>`（runtime.rs:87/106 初始化）；登记点：激活挂起分支 `insert(fiber.id())`（:565）+ advance 再挂起（:346）；撤销点：advance 取走 `remove(&fid)`（:326）+ unload 收账 | 抽测 `cargo +1.97.0 test -p cordis-core --lib` = **60/60** ✓ |
| `suspended_fibers()` | :353-354 快照（`iter().copied().collect`）| ✓ |
| `advance_suspended(judge)` | :363-370（对挂起快照逐 fiber judge→advance；doc 注明"advance 对未挂起 panic=bug 纪律不变，本方法只驱动快照中挂起成员"）| ✓ |
| 单测 | `advance_resumes_suspended_fiber`（:1191）/ `suspended_set_tracks_and_batch_advances`（:1243 登记+批量）/ `update_during_suspend_reclaims_and_restarts`（:1292）| 实测 60/60 含三测试 ✓ |
| P3-2 guard 复核 | `update_during_suspend_reclaims_and_restarts`（:1292）——unload 收账 → 惯性 reload 新代 → 挂起集重登记；guard=`target.is_some()` 弱化在更新路径安全结论成立（与 REVIEW-8589ca2 m-2 对齐）| ✓ |
| P3-2 判据 v2 | EXIT §1 记录**不启用**：外部判据 + `advance_suspended` 显式批量驱动已覆盖；自动轮询引入"谁在何时 poll"单线程语义问题——理由诚实、留真实场景再议 | 无多余代码（无 v2 机制）✓ |
| P3-3 统一驱动 | `WasmComponent::poll_and_advance`（lib.rs:440：poll_remotes + advance_suspended 驱动）；端到端 `poll_and_advance_drives_suspend_loop`（a2_e2e:147）| 抽测 a2_e2e = **3/3**（含回路）✓ |
| THEORY-MAP | P-3 授权行 + B-A1 行重组修复（REVIEW-e8e9b8c Minor-1 落地）| 存在 ✓ |

## 门禁数字

- EXIT 声明：core 60/60、wasm 全套绿（a2_e2e 3/3）、clippy/fmt/doc 0、workspace 无回归、零第三方、core 额度合规（P3-2 仅测试）。
- 抽测复核：**core 60/60**、**a2_e2e 3/3** 与声明一致；clippy/fmt/doc/workspace 由父会话已验（本走查未重跑全量，属父会话既有验证）。

## 发现

无阻塞发现。次要观察（可选）：`advance_suspended` 对"judge 对未挂起成员调用"的安全已由"只对快照挂起成员驱动"保证（:364 for 循环遍历 suspended_fibers 快照——若 advance 内再次挂起/完成，快照成员在循环期间被 advance 正确处理，无重复/遗漏——EXIT Nit"多组件未就绪强制 advance 再挂起（无害）"已记录）。

## 出口判定

**P-3 出口成立**——挂起集生产化（枚举/上报/批量恢复）+ advance guard 复核（含 update 交互直证）+ wasm 统一驱动回路 + 判据 v2 评估记录 + 全回归绿 + 审查闭环（REVIEW-e8e9b8c / REVIEW-7833761 均 PASS，0 Major 未决）。→ 下一线 P-4（go ABI 同步自动化）按纪律待用户确认。

> 说明：作为被委派子代理仅读核查 + 写报告（`docs/reviews/REVIEW-P3-EXIT.md`），未做 commit/文件修改——入库由父会话（主导 agent）处理。
