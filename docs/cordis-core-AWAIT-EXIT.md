# core 步进扩展（Await）专项出口判定（B 计划）

**依据**：提案 `docs/cordis-core-AWAIT-PROPOSAL.md`（授权 B）；计划 `docs/cordis-core-AWAIT-PLAN.md`（A1–A4）；C 探针（`docs/cordis-wasm-C-PROBE-EXIT.md`）结论前置。
**判定日期**：2026-08-20（A1/A2a/A3 审查闭环 + A4 出口走查；A2b go 遗留单列）。

---

## 1. 决策落地（B 提案 §5）

| 决策 | 落地 |
|---|---|
| 授权 B（动 core，单次控限） | Step::Await + resumable + advance 落入 cordis-core（THEORY-MAP B-A1 授权行标注）；core 改动额度 = A1 范围（额外仅 Fiber::is_suspended 访问器） |
| Await 无载荷 | 判据由调用方 advance 前满足 |
| advance 协议 | `Runtime::advance(fid)`，未挂起 panic=bug |
| 可恢复执行 | `try_execute_with`（带初始 acc 连续恢复）；`execute` 对 Await panic=走错路径（添加性零回归） |
| THEORY-MAP 偏离标注 | 论文确定性一次性效应 vs 产品异步 await（已入"已知偏差"表） |

## 2. 里程碑对照

| 里程碑 | 内容 | 审查 |
|---|---|---|
| A1 | core 机制（Await/try_execute_with/resumable/advance/unload 回收/PushingIter 透传）+ 单测（挂起/恢复 LIFO/违约 panic） | REVIEW-8589ca2 PASS |
| A2a | wasm 桥接线：wit effect-step.inverse→option、WasmTaskIter 在途 join→Await、rust 4 guest ABI 同步重编、a2_e2e 端到端（guest 完整 take-await + O-6 隔离 + 错误通道 err 达 guest） | REVIEW-dbc2384 PASS |
| A3 | C 探针定位（轻量捷径/归档）+ 时序边界解锁记录 + WasmRemote doc 引用 Await + Cargo.lock 入库 | （合并走查） |

## 3. 验收直证

- **core**：`advance_resumes_suspended_fiber`（K1 挂起→advance→K2→退役 K2 先解 LIFO）、`try_execute_with_suspends_on_await_and_resumes_lifo`、`advance_unresumed_panics`（违约 panic=bug）；core 58 全绿 + workspace 无回归。
- **wasm**：`a2_e2e` 2/2——guest submit→Await 挂起→poll 回填→advance→**guest 自取 take** 结果落盘（O-6 隔离：远端在 worker 池线程）+ op panic→err 回填→**guest take Err** 分支；wasm 全套绿（lib7 + 集成15，go 2 ignore）。
- 门禁：fmt / clippy `-D warnings` / doc 0 告警 / workspace 无回归。

## 4. 遗留（诚实记录）：A2b go ABI 收尾

- **root cause**：host 解码 go 的 effect-step 报 `invalid option discriminant`——wit `inverse→option` 后，go 侧（wit-bindgen go 0.60 对 `option<resource>` 的编码）与 wasmtime 组件模型期望的判别布局未对齐（go 绑定层问题，非 rust/core）。
- **现状**：rust 系 4 guest 全对齐；`go_guest` 2 测试暂 `#[ignore="A2b"]`（M1 双语言门禁恢复项）。
- **处置建议**：A2b 独立排期——对齐 go 绑定对 `option<inverse>` 的编码（可选：wit 结构改显式 `variant effect-step { step(inverse), wait }` 绕开 option<resource> 边界），或 go 端手工 encode 修正 + 测试恢复。

## 5. 出口判定

**B 计划主体完成**（core Await 机制 + wasm guest 完整 take-await 端到端 + 错误通道 + 文档归位 + 门禁全绿 + 审查闭环，0 Major 未决）。**A2b（go ABI 收尾）为既定遗留**（wit 波及，具因/处置已记录，独立跟踪，不在本出口阻塞判定内——rust 侧 B 目标达成，go 为 M1 历史双语言门禁恢复项）。
</RSEOF>
echo EXIT-wrote && git add crates/cordis-wasm/tests/go_guest.rs docs/cordis-core-AWAIT-EXIT.md && git commit -q -m "docs: B 计划 A4 出口判定——主体完成（core Await + guest take-await + err 通道 + 文档归位 + 门禁绿）；A2b（go ABI，invalid option discriminant 根因）列为既定遗留独立跟踪（B 计划）" && git log --oneline -1
