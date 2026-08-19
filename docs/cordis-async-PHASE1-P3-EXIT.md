# cordis-async Phase 1 P1.3 出口判定

**依据**：计划 `docs/cordis-async-PHASE1-P3-PLAN.md`（R0–R3 + 出口与决策）；草案 v1.4（冻结）§2/§4、契约 C-3/O-6。
**判定日期**：2026-08-19（R1–R3 审查闭环后，出口走查）。
**出口标准**（计划 §2 Step R4）：双形态语义完整 + WasmRemote 边界符合 D-2 + 共存文档/示例与实现一致 + 门禁全绿 + 出口走查。

---

## 1. 交付对照（P1.3 §0）

| 交付 | 落地 | 依据 |
|---|---|---|
| Send-future 分池形态 | `RemoteRequest` 内部双变体（Closure \| Future：`Pin<Box<dyn Future+Send>>`）+ `TokioRemote::submit` 双形态调度（`spawn_blocking` / `handle.spawn`）+ `await_remote_join` 回灌；`submit(RemoteRequest) -> RemoteJoin<RemoteValue>` **冻结签名不变**（H3 保持） | R1（REVIEW-281c6ac） |
| WasmRemote 边界 | `WasmRemote` 占位类型 + M1 宿主驱动协议接线 doc（guest 无自发线程、submit=入队+宿主 step 边界驱动+回填；M1 专项接入后 `impl Remote`） | R2（REVIEW-42c1edc），D-2 默认 |
| 双运行时共存收口 | 拓扑文档 `docs/cordis-async-THREADING.md` + 组合示例 `examples/async_combo.rs`（可运行：sync 树 EventsProvider + async use_component + Remote Send-future 回路 hits=7 + retire/settle） | R3（REVIEW-9af13e6），D-3 默认 |

## 2. 单测（累计）

- protocol 21 + spikes 3 = **24/24 全绿**；R1 新增 `send_future_submits_to_worker_pool_and_joins_back`（提交+join 回灌 42）与 `send_future_executes_on_worker_pool_not_combo_thread`（O-6 隔离）。
- 闭包形态（m06 既有）回归不破坏。

## 3. 门禁与回归记录

- `cargo +1.97.0 fmt --check` ✅ / `clippy --workspace --all-targets -- -D warnings` ✅ 0 告警 / `doc -p cordis-async --no-deps` ✅ 0 告警
- `cargo +1.97.0 test --workspace` ✅ 无回归（cordis-async 24 条 + 既有全绿）
- 组合示例 `cargo run -p cordis-async --example async_combo` ✅ 打印通过（三端回路）
- 零第三方 run-deps：cordis-async 仅 `cordis-core` + `tokio`（events/loader 在 dev-deps，示例/测试用）
- 里程碑审查闭环：R1（REVIEW-281c6ac）/ R2（REVIEW-42c1edc）/ R3（REVIEW-9af13e6）全部 PASS，0 Major / 0 Minor 未决

## 4. 决策记录

- D-1（Send-future 双形态）：做——`from_future` + 双形态调度（future=非阻塞异步、闭包=阻塞/CPU，O-6 分工 doc）。
- D-2（WasmRemote 范围）：接入点 + 协议接线 doc；实际 wasm 桥（host step 边界驱动、跨 wasm 值传递）留 M1 wasm 专项。
- D-3（双运行时共存）：拓扑文档 + 组合示例（R3）。

## 5. 出口判定

**P1.3 全部完成**：Remote 双形态语义完整（闭包回归 + future 直证 + O-6 隔离）、WasmRemote 边界符合 D-2、共存文档/示例与草案/实现一致、门禁全绿、审查闭环。

→ **进入 P1.4 决策**（DX 文档：插件作者指南、错误/安静语义、示例插件模板——基于 P1.1 事件订阅、P1.2 门面、P1.3 组合示例；纯文档线，按纪律由用户下达开工）。后续取向：M1 wasm 桥专项（WasmRemote 宿主驱动）、更多产品假设 spike，P1.4 出口评估。
