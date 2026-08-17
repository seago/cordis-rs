# PR #29 修复闭环审查报告（b643e34 + b9b55c2）

> 审查员：独立 PR 审查（subagent）
> 范围：commit `b643e34`（code：fiber.rs / runtime.rs / update_binding.rs / lib.rs）+ `b9b55c2`（docs：THEORY-MAP 处置⑩ 行 + REVIEW-97bb598.md 入库）
> 对照物：REVIEW-97bb598.md（major-1 + nit-1/2/3/4）、cordiverse/cordis v4、TS `fiber.ts` assertActive/update
> 验证：`cargo test -p cordis-core --test update_binding`（4 pass）、`cargo test -p cordis-loader`（28 pass），均通过。

## 一、审查范围

- `crates/cordis-core/src/fiber.rs`：`Fiber::update` 断言放宽。
- `crates/cordis-core/src/runtime.rs`：`update_fiber` 双路径 + `set_update_hook` doc + `refresh`/`compute_target`/`reload` 复活链路。
- `crates/cordis-core/tests/update_binding.rs`：`update_revives_failed_fiber` + 负例 `update_on_inactive_fiber_panics` 措辞更新。
- `crates/cordis-loader/src/lib.rs`：模块文档边界① + `inject_change_without_revision_is_ignored` 负例测试。
- `docs/THEORY-MAP.md` 处置⑩ 行、`docs/reviews/REVIEW-97bb598.md`。

## 二、逐条发现（对照 REVIEW-97bb598 的 major-1 / nit-1~4）

### 2.1 major-1 复活语义 ✅ **已如实落实，且直证非巧合**

- **断言放宽**：fiber.rs:204 的 `matches!(Active { .. })` 已改为 `Active { .. } | Inactive(Some(_))`，与 REVIEW 建议 (a) 一致。
- **`update_fiber` 失败路径**：runtime.rs:410-413「`Inactive(Some(ζ))` → 清 ζ（`Inactive(None)`）→ `refresh`」。链路核实：
  - `refresh`（runtime.rs:333-358）调 `compute_target` **从零重算**（542-557：`retired || !satisfied → None`，否则重算 uid 视图），不依赖失败路径留下的旧 `target = None`（⊥），重算结果 `Some` 与记录 `None` 不等 → 更新 `target` → 因 state 为 `Inactive(None)`（非 Active/Reloading/Unloading），且 `target.is_some()` → 走 `reload`（356-357）。
  - `reload` 成功后 `notify` 依赖者（472），consumer 级联恢复。
- **同型性核对（TS）**：TS `assertActive` 判定 `uid !== null`（fiber.ts:225）+ `update` 内 `fiber._error = undefined`（482）→ `restart()`（`_setEpoch(INACTIVE)` + `_refresh`）。Rust「清 ζ + refresh 重算 → reload」与 TS「清 error + 强制 INACTIVE + refresh」语义同型，报告声明准确。
- **退役仍 panic**：`Inactive(None)` 不满足新断言 → 仍 panic；`update_on_inactive_fiber_panics` 的 `should_panic` 措辞更新为「仅 Active/失败态」，与代码实际 panic 文案一致。
- **测试直证**：`update_revives_failed_fiber` 断言了 ① 首次激活失败为 `Inactive(Some(_))` ② consumer 停用 ③ `provider.id()` 复活前后**不变**（身份保留）④ provider/consumer 均 `Active` ⑤ `db_value == "pg"`（新 config 生效）⑥ `is_quiet`。其中 ③⑤⑥ 三重合证直指「复活」而非「重建」，**直接证明而非巧合同过**。
- **结论**：major-1 采纳 TS 语义的方案、实现、测试三者闭环，与 REVIEW 建议 (a) 完全对应。

### 2.2 nit-1（观察者 catch_unwind 之外、不参与 L-Raise）✅ **已如实落实**

- runtime.rs:376-379 新增约束段：「观察者运行于 `reload` 的 `catch_unwind` **之外**，不参与 L-Raise 恢复通道——实现者不得在观察者内 `FiberError::raise`（将被当普通 panic 重抛）」。措辞与 REVIEW 建议修法逐字对应。

### 2.3 nit-2（loader 模块文档边界① 更新）✅ **已如实落实**

- lib.rs:24-31 已把边界① 从「双向写回未实现…公开差异关闭」改为「组件侧已落地（G1，PR #29）…仍缺席 self-dispose→disabled 写回（TS `internal/plugin` 半段）」。与 THEORY-MAP / TS-REFERENCE-GAP 口径对齐，旧结论残留已清除。

### 2.4 nit-3（负例测试 `inject_change_without_revision_is_ignored`）✅ **已如实落实且直证**

- lib.rs:1951-1990 新增该测试：初始 `with_inject(fs, /a, false)` → 判 `rw:/a`；同 revision 换 `with_inject(fs, /b, true)` 后再次判 `rw:/a` 且 `is_quiet`。直接钉死「同 revision inject 变更被 reconcile 忽略、须随 revision 递增」纪律，非文档兜底。

### 2.5 nit-4（find_loaded_mut 的 contains_key+get_mut 保留 + 注释）✅ **已如实落实**

- lib.rs 保留 `contains_key` → `get_mut` 两段式并附注释说明 if-let 直返触 E0499 借用冲突。与 REVIEW 结论（nit-4 纯风格、可不动）一致，注释已补。

## 三、修复是否引入新问题（borrow / refresh 重算路径）

- **borrow 处理**：runtime.rs:406-407 两次**语句级不可变借用** `state.borrow()` 取 `active`/`failed` 布尔，随后 412 才 `state.borrow_mut()` 清 ζ；三者不重叠、无重入，不触发借用冲突（`unload`/`refresh` 内部自持 `borrow_mut`，因外层借用已释放）。**无问题**。
- **refresh 对失败 fiber（target=⊥）的重算**：失败路径（runtime.rs:461-463）已把 `target = None` 且在失败时 `unload` 恢复已完成步骤，故失败终态是「无残留绑定 + `target=None` + `Inactive(Some ζ)`」。复活时 `compute_target` 从零重算（不读旧 target、不吃 ⊥ 的脏值），`refresh` 以「结果 ≠ 记录 None」驱动重算，链路正确、无 stale-state 泄漏。
- **复活瞬间 `Inactive(None)` 的中间态**：清 ζ 后、`refresh` 重算前，state 短暂为「退役态」形态（`Inactive(None)`）。此为语句级瞬时态，单线程同步、无观察者在其间介入，无实际暴露。**非缺陷**（可记一条 nit-级别观察，不影响结论——见下）。
- **Active 路径不变**：`unload(fiber)`（前述 `Active → 反转效应 → 目标未变 → 链式 reload`）为本 PR 前既有行为，未改动。

## 四、总体结论

**通过（0 major ｜ 0 nit 缺陷；附 1 条非阻断观察）。**

`b643e34 + b9b55c2` 对 REVIEW-97bb598 的 **major-1 + nit-1/2/3/4 五项全部如实闭环**：复活语义采纳 TS `assertActive`/`_error = undefined` 同型方案并附直证测试（`update_revives_failed_fiber` 断言身份保留 + 依赖者恢复 + 新 config 生效三重非巧合证据）；退役仍 panic 的契约被负例测试续钉；nit-1/2 文档落地、nit-3 纪律钉死、nit-4 注释补齐。修复未引入借用冲突或 refresh 重算的 stale-state 问题；复活瞬时 `Inactive(None)` 中间态为语句级、无观察者可介入、非缺陷。

**非阻断观察（可选，建议后续顺手记录而非本 PR 阻塞）**：`update_fiber` 复活分支以 `state = Inactive(None)` 作为「清 ζ」的载体，语义上瞬时与「退役」共享同一状态字（仅因随后立即 `refresh` 翻转而不被观测）。若要更显式，可将失败态独立的 `target = ⊥` 作为唯一失败标记、清 ζ 改为直接置 `Inactive(None)` 前先由 `refresh` 判 target 重算——现写法已正确，此仅为可读性层面的备注，不构成缺陷。

**major：0 ｜ nit：0**
