# PR #30 审查报告（460d8d0 + 1f489e3）：G1 剩余 + G4 最小 hooks——退役写回（self-dispose → 条目 disabled）

> 审查员：独立 PR 审查（subagent）
> 范围：`460d8d0`（code：`crates/cordis-core/src/fiber.rs`、`crates/cordis-core/src/runtime.rs`、`crates/cordis-loader/src/lib.rs`）+ `1f489e3`（docs：`docs/TS-REFERENCE-GAP.md`、`docs/THEORY-MAP.md`）
> 对照物：TS loader/index.ts:88-124（`internal/plugin` 半段，6 个 case 过滤）、论文 §5.2.1、`docs/TS-REFERENCE-GAP.md`、`docs/THEORY-MAP.md`
> 定向验证：`cargo test -p cordis-loader`（31 pass / 0 failed）、`cargo fmt --check`（干净）、`cargo clippy -p cordis-loader -p cordis-core --all-targets`（干净）

## 一、审查范围

- **core**：`Fiber::retire` 新增退役观察者触发点（`retired.set(true)` 后、`refresh` 前）；`Runtime` 新增 `RetireHook` 类型、`retire_hook: RefCell<Option<Rc<RetireHook>>>` 字段（`pub(crate)`）与 `set_retire_hook` 方法。
- **loader**：`Loader` 新增 `retire_pending: RefCell<Vec<FiberId>>`、`in_apply: Cell<bool>` 字段；`apply` 加 `in_apply` 标志分流 + 末尾排空；`register_retire_hook`/`writeback_retire`/`entry_disabled` 三个方法；`find_fiber` 对退役 fiber 返回 `None`；`reconcile_into` disabled 清除分支加退役 fiber 拆除。
- **测试**：3 个新 loader 测试（`self_retire_writes_back_disabled_to_entry` / `loader_driven_operations_do_not_write_back_disabled` / `group_child_self_retire_maps_to_nested_entry`）。
- 对照既有测试 `retired_component_persists_across_unchanged_apply`（无 hook）与核心退役语义。

## 二、逐条发现

### 1. core `set_retire_hook`/`RetireHook` + `Fiber::retire` 触发点 ✅

`Fiber::retire`（fiber.rs:186-196）实现：

```rust
pub fn retire(self: &Rc<Self>) {
    self.retired.set(true);
    if let Some(hook) = &*self.ctx.runtime().retire_hook.borrow() {
        hook(self);
    }
    self.ctx.runtime().refresh(self);
}
```

- **触发点正确**：`retired` 置位后、`refresh` 前触发，与 `update_hook`（`update_fiber` 中先触发观察者再重跑）的时序模式一致。
- **任何 retire 均触发**：无"仅自退役"的判决在调用点；过滤下沉到观察者内部（loader 侧 `entry_of` + `!l.disabled`）——与提交信息及文档声明一致。`retire` 幂等但**每次调用都触发 hook**（`retired.set(true)` 无 already-guard）：重复 `retire()`（如 teardown 对被多次引用的 fiber 再次 retire）会重复触发 hook，但 `writeback_retire` 的 `!l.disabled` 守卫使写回幂等，无副作用。
- **约束声明一致**：`set_retire_hook` 的 doc 明确"观察者运行于同步路径、panic = 宿主 bug（传播）、不得 `FiberError::raise`/不参与 L-Raise 通道"——与 `set_update_hook` 同约束，代码无任何 `raise`/`catch_unwind` 包裹观察者调用，声明与行为一致。
- **字段可见性合理**：`retire_hook` 为 `pub(crate)` 且注释说明由 `Fiber::retire` 同步触发；`retire_hook` 与 `update_hook` 同款 `RefCell<Option<Rc<…>>>`，单线程 host 无重入冲突（hook 体内 borrow 的是 `entries` 而非 `retire_hook`，无自借）。

**结论：无缺陷。**

### 2. loader `register_retire_hook` 过滤 + `in_apply` 分流 + `retire_pending` 延迟排空 ✅（含一处 nit 见 nit-1）

- **过滤等价性**：`writeback_retire` 用 `entry_of(fid)`（fiber→条目 id 反查）+ `find_loaded_mut(&id)`（条目仍在）+ `!l.disabled`（未 disabled）三重条件，正好等价 TS `internal/plugin` 半段的"条目仍在且未 disabled 才写回"= 组件自退役。loader 驱动的退役（teardown）发生时：revision 重建前 `unload_from` 已 `map.remove`（条目已移除 → `entry_of`/`find_loaded_mut` 返回 `None`）；disabled 置位路径先 `unload_from` 再 `l.disabled = true`（条目已置位 → `!l.disabled` 为假）。两者自然过滤，与提交信息"条目已移除或已置位 → 自然忽略"一致。
- **`in_apply` 分流 + `retire_pending` 解决 RefCell 重借**：`apply` 在 `apply_into` 阶段持有 `&mut entries`（`self.entries.borrow_mut()` 贯穿整个递归协调），此期间 teardown 触发的 `retire()` → hook 若直写 `writeback_retire` 会二次 `borrow_mut` entries → panic。`in_apply.get()` 为真时改 push `fiber.id()` 到 `retire_pending`，apply 末尾 `std::mem::take` 排空——`apply` 函数中 `in_apply.set(false)` 先于排空，排空时 `writeback_retire` 用 `borrow_mut`（此时 `apply_into` 的 `&mut` 已 drop），无冲突。设计正确。
- **排空与重实例化的无害性**：apply 期间 teardown 的已退役 fiber 在排空时其条目已被重建/替换（`make_loaded` + `map.insert`），`entry_of(old_fid)` 反查不到（新 fiber id 不同）→ 无写回。若 disabled-clear 分支先 `unload_from`（teardown 触发 retire → pending 又 push 一次旧 fid）再 `make_loaded`，排空时旧 fid 依然反查不到 → 无害。幂等且安全。
- **`in_apply` 未在 panic 时复位（nit-1，可接受）**：`apply` 无 `Drop`/`catch_unwind` 兜底——若 `apply_into` 内部 panic（未注册组件、供给冲突、remove_fiber 失败等均 `panic!`），`in_apply` 永久滞留 `true`，此后任何组件自退役将被恒定 push 到 `retire_pending` 而永不被排空（因不再有完整 `apply` 走到排空行），写回静默丢失。**评估为可接受**：其一，`apply` doc 明确 panic = 配置错误 = 宿主 bug，loader 在此后本即处于不一致状态；其二，与 `register_update_hook`（无 `in_apply` 分流、teardown 也不触发 update hook）不同，此路径只在"apply 已 panic 后仍继续使用同 loader 且期待写回"这种双重违约场景才暴露，属防御性范畴。建议记录为已知边界而非阻断。

**结论：正确，附 nit-1。**

### 3. reconcile disabled 清除分支拆除已退役 fiber + `loader.fiber(id)` 查询语义 ✅

- **ProvisionClash 前提核实（正确）**：自退役的 fiber **只置 `retired` 不 `remove_fiber`**，仍留在 `Runtime.fibers` registry（`Fiber::retire` 只 `refresh`，不清理 registry；`remove_fiber` 仅由 loader `teardown` 调用）。因此若 disabled-clear 分支直接 `make_loaded` → `instantiate_leaf` → `ctx.use_component`，而旧 fiber 的供给名仍被 registry 持有 → `ProvisionClash`（runtime.rs:214 供给名冲突检查）。新增的"退役 fiber 先 `unload_from` 拆除"代码（lib.rs:545-549）确实释放供给名（`teardown` 末尾 `remove_fiber`），消除 clash。前提判断**准确**。
- **`loader.fiber(id)` 对退役 fiber 返回 `None`**：`find_fiber` 对命中条目返回 `loaded.fiber.clone().filter(|f| !f.retired())`——退役即视为"已卸载"查询语义。LoadedEntry 的 `fiber` 字段仍持引用，供 disabled-clear 分支的 `map.get(&entry.id).and_then(|l| l.fiber.clone())` 读取并判定 `retired()` 后拆除——引用保留与查询返回 `None` 二者并存，注释（lib.rs:445-447）已明确说明用途，设计自洽。
- **与既有测试 `retired_component_persists_across_unchanged_apply` 无冲突（结论正确）**：该测试**未注册 retire hook**，且退役后**从不经 `loader.fiber("provider")` 查询**（而是直接持有 `provider: Rc<Fiber>` 调用 `.retired()` / `.state()`），故 `find_fiber` 语义变更与 disabled-clear 拆除路径均不触及该测试的断言路径。其"退役粘滞跨未变 apply"的核心断言（`provider.retired()` 为真、consumer 仍 Inactive）依然成立：无 hook 时退役不写回条目书签，未变 apply 走 no-op 分支，`l.disabled` 保持 false，不触发 disabled-clear 重实例化。**退役粘滞路径仍成立，无回归。** 文档头（lib.rs:32-35）已把"粘滞"表述修正为"无观察者时跨未变 apply 保持"，与代码一致。
- 边界验证：desired 显式 `disabled=false` 的 apply（有 hook）会因 `l.disabled==false`（写回后）与 `entry.disabled==false` 走 disabled-clear 分支 → 拆除退役 fiber → 重实例化 → 重新启用。此路径由测试 1 直接覆盖（详见 §4）。

**结论：无缺陷。**

### 4. 测试质量 ✅（3 个 loader 测试直证声明语义，非巧合）

- **测试 1 `self_retire_writes_back_disabled_to_entry`**：完整走「自退役 → 观察者写回 `entry_disabled==Some(true)` + consumer 级联停用 + `is_quiet` → desired `disabled=false` 重新启用（新 fiber id ≠ old、`entry_disabled==Some(false)`、Active）+ 自退役后 desired `disabled=true` 保持禁用（`fiber==None`）」。直证三条关键声明：**disabled 是协调字段**（可被 desired 重新启用）、**重新启用 = 新 fiber**（退役已卸载旧 fiber）、**disabled=true 保持禁用**。覆盖了题目点名的"retire 后 apply disabled=true 的保持禁用"场景。
- **测试 2 `loader_driven_operations_do_not_write_back_disabled`**：revision 重建（`entry_disabled==Some(false)` 且 Active）→ disabled 切换（`Some(true)` + fiber None，且注释注明这是 desired 置位产物非写回）→ 条目移除（`None`）。直证 loader 驱动的三条路径均不写回，非巧合。
- **测试 3 `group_child_self_retire_maps_to_nested_entry`**：组内子条目自退役 → `entry_disabled("child")==Some(true)`，验证 `entry_of` 递归反查命中嵌套条目书签。直证组内映射。
- **缺失场景评估**：题目要求的核心场景均已覆盖。唯一未显式覆盖的是 apply 中途 teardown 触发的 retire 经 `retire_pending` 排空后的**无副作用**（即排空时旧 fiber 不误写回），但该路径由测试 2 的 revision 重建（`Some(false)`）间接约束——若排空误写回，重建后 `entry_disabled` 会变 `true`，断言即失败。故该关键分支实为隐式覆盖。无实质缺失。

**结论：测试充分、直证语义，无缺陷。**

### 5. docs 一致性 ✅

- **TS-REFERENCE-GAP G1 完成标记**：G1 条目（TS-REFERENCE-GAP.md:123）改为"✅ **已落地（PR #29 + PR #30）**"，明确列出 `set_retire_hook`/`register_retire_hook`、过滤语义、pending 队列、`fiber(id)` 返回 None、desired `disabled=false` 重新启用，并注明"G4 hooks 最小集 = `update_hook` + `retire_hook` 两观察者"。与代码一致。
- **THEORY-MAP 处置⑩**：⑩ 行补全 self-dispose → disabled 写回已落地（PR #30）及实现细节，并重申退役粘滞（无观察者时）不变、同 revision apply 不清除 fiber 层写回——与代码语义一致。新增 PR #30 记录行在"审查记录"表，处置对 §5.2.1/Alg 5 映射正确。
- **loader 模块头边界①**：lib.rs:29-35 把"仍未缺席"改写为"已补齐（G1 剩余，PR #30）"，逐项列出过滤/延迟排空/`fiber(id)` 返回 None，并保留"退役粘滞（无观察者时…）+ disabled 为协调字段 + 同 revision 不清除写回"——与实现及测试语义一致，无矛盾。

**结论：无缺陷。**

### 6. 纪律 ✅

- `cargo fmt --check`：干净（exit 0）。
- `cargo clippy -p cordis-loader -p cordis-core --all-targets`：无 warnings。
- 零第三方依赖：本次 diff 新增 `use std::cell::{Cell, RefCell}` 与既有 `std` 类型，无新外部 crate。
- 未跑 wasm、未跑全 workspace（仅定位 `-p cordis-loader` + `-p cordis-core` clippy），符合审查约束。

**结论：无缺陷。**

## 三、总体结论

**通过（approve）**。PR #30 正确落地 G1 剩余（self-dispose → 条目 `disabled` 写回）与 G4 最小 hooks 的退役半段，语义与 TS `internal/plugin` 半段（条目仍在且未 disabled 才写回）等价；`in_apply` 分流 + `retire_pending` 延后排空正确规避了 apply 期间的 entries RefCell 重借；reconcile disabled 清除分支对退役 fiber 的拆除前提（ProvisionClash）判断准确；`loader.fiber(id)` 对退役 fiber 返回 `None` 的查询语义与既有粘滞测试无冲突。3 个测试直证声明语义而非巧合，docs 三处变更与代码一致，fmt/clippy 干净、零第三方依赖。

### 发现汇总

| # | 严重度 | 位置 | 发现 |
|---|---|---|---|
| nit-1 | nit | `crates/cordis-loader/src/lib.rs` `apply`（327-343） | `in_apply` 无 panic 兜底复位：`apply_into` 内部 panic（未注册组件/供给冲突/remove_fiber 失败）后 `in_apply` 恒为 `true`，此后自退役写回被永久延后且永不排空。可接受（panic = 宿主 bug、loader 已处不一致态），建议文档记录为已知边界，不阻断。 |

**major：0　nit：1**

---

**报告路径**：`docs/reviews/REVIEW-460d8d0.md`
