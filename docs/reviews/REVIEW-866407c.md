# 代码审查报告：commit `866407c`（M1.4 waterfall 短路 + 重入快照，Phase 1）

- **审查对象**：`866407c65842f6c7b4a513c2fd01c35980e5ec6e` — `test(events): M1.4 验收 #4 短路 + #7 派发中注册重入快照（Phase 1）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 866407c`（`crates/cordis-events/tests/events.rs` +75：`m14` 模块 2 个测试），对照冻结草案 `docs/cordis-events-protocol-draft.md` v0.3.1 §2.2 E-1/E-2、§6 验收 #4/#7；关联实现 `crates/cordis-events/src/lib.rs`（emit/waterfall/serial/bail 派发 + `waterfall_link` + 快照 + alive）与 `tests/events.rs` m12/m13 既有测试。
- **上下文**：M1.1–M1.3 已审查闭环（REVIEW-85d2379 / -a0963ab / -f8541f1，均 PASS）。E-1「派发中退订本轮跳过」为 M1.2 审查 Minor-1 确立要求、已落地（74ca052，waterfall/serial/bail 调用期 alive 检查）。
- **验证手段**：静态阅读 + 实际运行（见「验证记录」）。

---

## 总体结论

✅ **PASS WITH NITS**

- **major**：0
- **minor**：1（emit 派发缺调用期 alive 检查，与 waterfall/serial/bail 及 E-1 一致性要求不一致——当前因 `Disposer` 非 Send 不可达，属防御缺口、非客户可见缺陷）
- **nit**：0

M1.4 的两条验收测试直证充分、无 flaky、与既有测试互补不冲突；短路（#4）与派发中注册重入（#7）语义与草案 v0.3.1 及实现完全吻合；#8 空集（E-2）不回归；门禁全绿。建议放行 M1.5（Send+Sync 断言 + async/loader 集成，验收 #9），Minor-1 在 M1.5 或后续小修落地（不阻塞）。

---

## 发现

### Major：无

### Minor

### Minor-1（建议）：`emit` 派发缺调用期 alive 检查——与 waterfall/serial/bail 及 E-1 一致性要求不一致（当前不可达，防御缺口）

- **位置**：`crates/cordis-events/src/lib.rs` `EventBus::emit`（快照为 `Vec<EmitAnyFn>`，仅 collect 期 `.filter(|e| e.alive())` 一次性过滤，调用期 `for f in snap { f(any) }` **不检查 alive**）；对照 `waterfall_link`（每层调用期 `fs[i].0.load` 检查，退订者本轮跳过、不短路）与 `serial`/`bail`（`snapshot_reply` 带 alive + 调用期 `filter`/`continue`）。
- **问题**：E-1 的「派发中**退订**的监听器本轮不再调用」是 M1.2 审查 Minor-1 确立、已落实到 waterfall/serial/bail 的仓库立场；`emit` 未跟随——若派发中某 emit 监听器被退订，其后序监听器本轮**仍会被调用**（快照已含其闭包、无 alive 门）。当前 `Disposer`（core `Box<dyn FnOnce()`）**非 `Send`**，监听器闭包强制 `Send+Sync` → 客户代码无法在监听器内捕获/调用退订句柄 → 「emit 派发中退订」**不可构造**，故非客户可见缺陷。但①与兄弟派发及已确立一致性要求不一致；②防御缺口——若未来引入 Send 退订句柄、或内部可达路径（订阅失效通知等），emit 将失守而 waterfall/serial/bail 已防。
- **草案/依据**：草案 v0.3.1 §2.2 E-1（派发中退订者本轮不触发）；REVIEW-a0963ab Minor-1（「派发中途退订 = 本轮跳过」的落地要求）；M1.4 任务声明的边界 ——「pub API 下监听器内无 Send 退订句柄无法构造派发中退订」**诚实成立**（Disposer 非 Send 是类型强制），但「实现含调用期 alive 检查」的表述对 `emit` **不完全成立**（emit 只有 collect 期过滤）。
- **建议**：`emit` 快照改 `Vec<(Arc<AtomicBool>, EmitAnyFn)>` + 调用期 alive 检查（与 waterfall/serial/bail 对齐；成本一行 filter），或至少 doc 明示「emit 快照为 collect 期一次性过滤，在途退订场景依赖 `Disposer` 非 Send 保证不可达」——消除一致性歧义。不阻塞 M1.5。

---

## 核查要点（逐条确认）

- **#4 短路（验收 #4）— 直证成立**：`waterfall_link` 为递归链（`i` 达 `len` → terminal；中途 alive 失活 → 跳过但不短路——「退订 ≠ 拒绝」）。A 不调 `next` → 不递归下游 → B 与 terminal 均不执行；A 的 `&mut` 修改 `*p=5` 沿链保留。测试双断言 `v==5`（载荷修改保留）+ `log==["A"]`（下游/terminal 跳过）精确直证，与草案 §2.2 短路语义一致。
- **#7 派发中注册（验收 #7 前半）— 直证成立**：`emit` 快照（`Vec<EmitAnyFn>`）锁内 collect、锁外调用（release-then-invoke）；A1 调用期 `bus.on`（写锁）注册 A2 → A2 不在本轮 snap → 本轮 `log==["A1:1"]`；下一轮 snap 含 A1+A2（注册序）→ `log==["A1:1","A1:2","A2:2"]`。断言精确直证 E-1「派发中注册本轮不触发、下一轮触发」。
- **E-1 退订者本轮跳过（验收 #7 后半）— 边界诚实、见 Minor-1**：burn 测试不可构造（Disposer 非 Send），M1.4 明确不attempt；可达的退订语义已由 m12 `disposer_idempotent_double_dispose`（派发前退订 → collect 期过滤、彻底不触发）与 m13 `manually_dispose_and_ctx_dispose_all_share_armed`（双路径 armed）覆盖。声明诚实合理——唯一缺口是 emit 缺调用期 alive 检查（Minor-1）。
- **#8 空集（E-2）— 不回归**：`empty_listeners_e2`（emit=no-op、waterfall=仅 terminal、serial=空 vec、bail=None 四断言)存在且绿（14/14 含），M1.4 未触碰。
- **测试质量**：两测试均单线程、无 timer/sleep（无 flaky）；断言为 log 序 + 载荷值（精确直证）；与既有测试无重叠冲突 —— #4 与 m12 `waterfall_basic_link_order`（全链序 + around）**互补**（后者测链贯通、前者测短路断链）；#7 注册重入与 #2（collect 期过滤退订）、m13（armed 双路径）互补。监听器捕获 `Arc<EventBus>`/`Arc<RwLock<Vec>>`（Send+Sync 上界满足 §0 核心义务）；`_d2` 用命名绑定（非 `_`）保证订阅在闭包内不提前 drop，正确。
- **工程门禁 — 核实通过**：见「验证记录」。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `cargo test -p cordis-events` — **PASS**，14/14：
   - `m14::waterfall_short_circuit_skips_downstream_and_terminal` / `m14::dispatch_register_this_round_skipped_next_round_fires`（新增）ok
   - m12：`emit_order_and_payload` / `disposer_idempotent_double_dispose` / `serial_collects_all_and_bail_stops_on_first_some` / `waterfall_basic_link_order` / `empty_listeners_e2` / `listener_captures_arc_not_rc` / conflicts 4 条（should panic）ok
   - m13：`subscribe_auto_unsubscribes_on_fiber_retire` / `manually_dispose_and_ctx_dispose_all_share_armed` ok
2. `cargo clippy -p cordis-events --all-targets -- -D warnings` — **PASS**，exit 0，0 告警。
3. `cargo fmt --check` — **PASS**，干净。

---

## 结论

M1.4（验收 #4 waterfall 短路 + #7 派发中注册重入快照）实现与草案 v0.3.1 §2.2 E-1/E-2 完全一致：短路断链语义、快照 collect + release-then-invoke 注册序重入语义均直证充分、无 flaky；E-1 退订在途边界声明诚实（Disposer 非 Send 不可构造）；#8 空集不回归；门禁全部绿。

**建议放行进入 M1.5**（Send+Sync 编译断言 + async 监听器衔接 + loader 集成，验收 #9）。通过前无必须修复项（Major 0 / Minor 1 为 emit 调用期 alive 检查的防御缺口，当前不可达、不阻塞，可在 M1.5 或后续小修一并处理）。
