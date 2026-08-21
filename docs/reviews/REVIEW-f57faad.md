# 代码审查报告：commit `f57faad`（P-1 wasm 逆表回收，产品验证线）

- **审查对象**：`f57faad` — `feat(wasm): P-1 P1-1+P1-2 wasm 逆表回收——Host.inverse_free 复用池（run_inverse 执行后入池，drop 保持 no-op）+ set 分配优先复用 + 有界性验收测试`
- **审查日期**：2026-08-22
- **审查人**：independent-review-agent
- **审查范围**：`crates/cordis-wasm/src/lib.rs`（Host.inverse_free / set 分配 / run_inverse 入池 / 测试），对照 `docs/cordis-PRODUCTVAL-P1-PLAN.md` P-1 计划 & REVIEW-2a7a686 m3。
- **验证**：静态阅读 + `cargo +1.97.0 test -p cordis-wasm`（lib 8 + 集成 14 全绿，含 go；新增测试过）。

---

## 总体结论

✅ **PASS WITH NITS**（0 Major / 1 Minor / 3 Nit）→ 放行 P-1 出口。

核心回收方案正确、安全（仅"逆已执行"入池、take 幂等防重复入池、跨边界 move 语义保证句柄已撤销），回归无破坏。1 项 Minor 为**测试路径覆盖缺口**（新增测试用手动 push 模拟释放，未走真实 `run_inverse` 入池路径）。

---

## 发现

### Minor

#### m-1（建议）：新增测试未覆盖真实 `run_inverse` 入池路径

- **位置**：`host_inverse_free_reuse_bounds_rep_allocation`——用 `host.inverse_free.push(first)` **手动模拟**了"逆已执行"释放，而非走真实的 `run_inverse`（核心 Disposer 执行 + 入池）。
- **影响**：已验证"复用池机制"（pop 复用、`next_rep` 有界），但**未验证** `run_inverse` 真正把 rep 入池的完整链路（真实路径涉及 wasm guest step + 核心逆执行 + borrow 顺序）。真实路径的 borrow/配对正确性是**静态推断**（core_inverses 与 store 两个 RefCell 独立、take 后入池），缺实机佐证。
- **建议**：补一个走真实路径的测试——宿主层直接调用 `InstanceState::run_inverse`（若可见）或经 wasm 集成（长驻组件 set→迭代器逆执行→复用断言）；至少可加一个"run_inverse 后 inverse_free 含该 rep"的最小断言。

### Nit

- **n-1**：测试里 `submit("nope")` 调用与测试意图无关（注释已说明"用 remote rep 空间"），但引发 `Host::submit` 的资源分配——建议移除（测试不涉及 remote），减少混淆。
- **n-2**：`run_inverse` 的 `task()`（核心逆执行）在 `host.inverse_free.push(rep)` 之前——若 `task()` panic（逆执行失败）则 rep 不入池（本可复用）——属防御性留白，可接受；建议注释注明"task panic 时 rep 不入池（保守）"。
- **n-3**：模块 doc 把 `drop 保持 no-op` 明确为"句柄销毁 ≠ 逆执行"——语义边界清晰，但**与计划 §1.1 的 free list 回收信号（drop/run 双信号）不一致**：实现收敛为"仅 run_inverse 入池"，doc 已如实表述（单信号更保守、更安全）——记录该差异（实现比计划更保守，无碍）。

---

## 通过项（逐条确认）

- **回收安全性**：仅 `run_inverse`（逆已执行、槽位空、句柄已撤销）入池；`drop` 不入池（句柄销毁 ≠ 逆执行，绑定仍待撤销——语义边界正确）。**复用安全**：旧句柄违规调用 run = panic=bug（协议 m4 保持）；跨边界 `effect-step.inverse` 为 owned 句柄 move 给宿主 → guest 不再持有 → run_inverse 后 rep 复用安全 ✅
- **分配/配对正确性**：set 分配优先从 `inverse_free.pop()`（空则 `next_rep++`）；复用 rep 后 `forward_pending` 的 `core_inverses[len<=idx 则 push None]` 重写新逆、镜像 bindings 正确——同 rep 旧槽位已 take 为空，配对正确 ✅
- **防重复入池**：`run_inverse` 用 `core_inverses[rep].take()`（取走 Some → None）→ `if let Some(...)` 才 push ——take 幂等，同一 rep 不会二次入池 ✅
- **借用无冲突**：`core_inverses.borrow_mut()` 借用结束于语句，之后 `store.borrow_mut()` 拿 host + push ——两个独立 RefCell（core_inverses / store）无冲突 ✅
- **有界性**：复用 rep → `next_rep` 恒定（断言 =1）+ `core_inverses` 表长度不增长（复用索引）——分配量 ≈ 峰值并发逆数，非操作次数 ✅
- **不改 core**（仅 cordis-wasm）；**回归**：wasm 全套 lib8+集成14 全绿（含 go），既有逆撤销路径不破坏 ✅

---

## 结论

**P-1（wasm 逆表回收）达成**：free list 复用方案安全（仅逆执行入池、take 幂等、跨边界 move 保证句柄已撤）、分配有界直证、回归绿。m-1（真实路径测试覆盖缺口）与 n-1..3 记录；**放行 P-1 出口**（EXIT 文档：REVIEW-2a7a686 m3 已知边界 → 已回收；m-1 建议随出口一并补覆盖或记入待办）。
