# 代码审查报告：commit `ef57804`（PR #20 M2-PR4——托管 realm + Algorithm 7 隔离 realm 重指派）

- **审查对象**：`ef57804ea369027d108577781585c7992e23876b`（`crates/cordis-core/src/{context,store,runtime}.rs` + `crates/cordis-loader/src/lib.rs`，+385/−55）及配套 docs 提交 `ad000e143ada46f58b47911e6285ed2c907d83cf`（`docs/PLAN.md` +1/−1、`docs/THEORY-MAP.md` +4/−2）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show ef57804` / `git show ad000e1` 逐行核对 diff；读 `context.rs`（`realm_of`/`isolate_in_place`）、`store.rs`（`move_binding`）、`runtime.rs`（`move_binding`/`provider_of_realm`/`notify_affected`/`compute_target`/`provider_of`/`satisfied`）、`loader/src/lib.rs`（`Entry`/`LoadedEntry`/`apply_into`/`reconcile_into`/`make_loaded`/`entry_ctx`/`annotated_ctx`/`patch_isolation`/`collect_subtree_ids` + 两个新测试）；对照 THEORY-MAP M0 走查 Algorithm 7 行/PR #20 行/PR #20 审查行与 PLAN M2 进度行。实跑 `cargo test --workspace`（**全绿，0 失败**，含 wasm 后端）、`cargo test -p cordis-loader`（**16 passed / 0 failed**）、`cargo build -p hello-plugin`（exit 0）、`cargo fmt -p cordis-core -p cordis-loader -- --check`（exit 0）、`cargo clippy --workspace --all-targets`（exit 0；仅 `cordis-hmr` 的 2 处历史 collapsible_if/unused_import 警告，与本 PR 无关）。

---

## 结论：**有条件通过**

M2-PR4 把 isolate 变更从"重建"正确升级为 Algorithm 7 的 realm 重指派：`Context::isolate_in_place`（ρ 就地改写）与 `realm_of`（公开读）语义精确；`Store::move_binding` 的 `from ∈ dom(σ) ∧ to ∉ dom(σ)` 前置条件与"值 + 提供者整体迁移"忠实论文 `store[s2] ← store[s1]`；`patch_isolation` 的 Δ 计算、ρ patch（条目 ctx + 子树 fiber ctx）、refresh、绑定迁移（own 移动前快照）、affected 通知五步顺序自洽；own 判定以子树成员关系等价替代论文 delimiter（δk）在**叶子条目**场景下与式 (65) 的可观察语义一致；affected 谓词 `resolve ∈ {s1,s2} ∧ own(P)`（外部依赖者 own(D) 恒 false，谓词退化为 own(P)）正确。全局 realm 共享时"绑定提供者属其他条目则不迁移"的关键分支（`subtree.contains(&p)` 为 false）已正确实现。

存在 3 项 nit（均为文档/覆盖口径，不阻塞合入）与 1 项需决策的 major（隔离边界 None↔Some / Local 迁移的测试覆盖缺口，功能上判无 bug 但未被行使）。详见下文。

---

## 🔴 需决策（major）

### major1. isolate 边界迁移（None→Some / Some→None / Local↔）无测试行使，机制正确性仅经推断

**位置**：`crates/cordis-loader/src/lib.rs:705-715`（`realm_of`）、`:616-622`（Δ 计算）、`tests` 两个新测试（`:1344` / `:1420` 均只覆盖 `Global→Global`）

**事实**：Algorithm 7 重指派的新测试只行使 `IsolateAnnotation::Global("db") → Global("db2")` 一种形态。被其**替换**的旧测试 `isolate_change_rebuilds_leaf`（diff 中删除，见 `git show ef57804` 第 377 行起）原先覆盖的是 `Local → Global` 变化（含 Local realm `local:p:val` 的生成与重建），现随"重建→重指派"语义升级被整体改写为 Global→Global，导致以下边界**不再有任何测试行使**：

1. **None ↔ Some**：`realm_of(None, ..) = key`（`:709`）——旧 realm 退化为**原始键符号**（Def 28 `R ⊇ K`），迁移目标/源为 `"val"` 这类裸键；`move_binding("global:db:val", "val")` 与反向 `Some→None` 的 `move_binding("val", "global:db:val")` 路径未验证。裸键 realm 与其它**未隔离条目**的供给键同域，存在 store 键碰撞的语义敏感点（例如另一未隔离条目已绑定裸 `"val"` 时，Some→None 迁移命中 `!store.contains(to)` 守卫而跳过迁移、绑定留在旧 realm——语义上是否可接受未验证）。
2. **Local → X**：Local realm 符号 `local:{id}:{key}` 携带条目 id，其迁移与 Global 命名共享无本质差异，但**从未经 `patch_isolation` 路径**行使（旧测试只走到了重建路径，新的两个测试都不涉及 Local）。

**影响（major，非 blocker）**：实现机制对 None/Local 经 `realm_of` **统一规约**（`None⇒key`、`Some(Local)⇒local:{id}:{key}`、`Some(Global(name))⇒global:{name}:{key}`），Δ 与迁移逻辑与 realm 符号具体形态无关，故从代码路径上**未发现 functional bug**；`diff.is_empty()` 早退（`:623-626`）也正确兜住 None↔None 的退化。但这是 PR 核心语义（realm 重指派）中**唯一由"边界未覆盖"而非"实现缺陷"承载的风险点**——建议补一个 `None→Some` 或 `Some(Local)→Some(Global)` 用例（含裸键 realm 与未隔离供给者共存时的 store 键碰撞分支），否则边界正确性只经推断、未经验证。此项可由审阅者判定为可接受（记录为已知边界）后放行。

---

## ⚪ 细节（nit）

### nit1. 模块头文档与 `apply` doc 的"isolate 变更走重建"旧表述未随 PR 同步更新（审查要点 4）

**位置**：`crates/cordis-loader/src/lib.rs:8-9`、`:24-27`（已知边界②）、`:263-264`（`apply` doc）

**事实**：`patch_isolation` 已落地 Algorithm 7，但三处陈旧表述仍在：
- 模块头 `:8-9`：`isolate`（…）"**变更 = 重建**——Algorithm 7 的 realm 重指派随 M2-PR4"；
- 模块头已知边界 `:26-27`："② isolate 变更走重建而非 Algorithm 7 realm 重指派（M2-PR4）"——此项现已**过时**（PR #20 恰好消除了该边界，应改标为已落地，或将边界②整体删除/改写为"组条目 isolate 变更仍走重建"）；
- `apply` doc `:263-264`："`component` / `revision` / `isolate` 变更 → 重建（M2-PR3：isolate 变更走重建，Algorithm 7 realm 重指派随 M2-PR4）"——与新的实际分派（`reconcile_into` 中 isolate 变更走 `patch_isolation`）矛盾。

**影响（非 block/major）**：纯文档口径滞后，与仓库"表里如一"纪律相悖（THEORY-MAP PR #19 行已把 isolate 变更重指派改为"**PR #20 升级为 Algorithm 7**"、PR #20 行新写，但 loader 源码内联 doc 三处未跟进）。审查要点 4 明确点名此点，建议随本 PR 一并修正。

### nit2. `collect_subtree_ids` 的子树递归对叶子条目恒为单元素集合——"子树 own 判定"的文档/测试命名言过其实

**位置**：`crates/cordis-loader/src/lib.rs:596-597`、`:633`、`:718-735`（`collect_subtree_ids`/`_into`）、`:1422-1423`（测试名与注释 "子树场景/子树成员"）

**事实**：`patch_isolation` 仅在**叶子条目**上被调用——组条目 isolate 变更走整棵重建（`:384-391`，`loaded.isolate != entry.isolate` → `unload_from` + `make_loaded`），且叶子 `make_loaded` 恒 `children: HashMap::new()`（`:471`）。故 `collect_subtree_ids(loaded)` 恒返回 `{ entry 自身 fiber id }`，`:718-735` 的 `children` 递归（`collect_subtree_ids_into` 的 `for child in loaded.children`）是**不可达的防御死代码**。注释 "子树成员（own 判定；论文 delimiter 的树等价）"（`:633`）、模块 doc "条目 fiber + 递归子代"（`:597`）以及测试名/注释 "子树场景 / 覆盖子树成员"（`:1422-1423`）把"子树"语义抬高到实际不存在的层级。

**影响（非 block/major）**：所声明的"以 loader 树子树成员关系等价替代 delimiter own"在**当前叶子-only 的调用面下**退化为"以条目自身 fiber 成员判定 own"，与论文式 (65) `γ′[δk]=d1 ⟺ 派生自条目 ctx` 的等价性**在叶子场景成立**（叶子的"子树"即自身），无正确性缺陷；仅为误导读者的文档过度概括 + 死代码，建议把 `collect_subtree_ids` 简化为单 fiber 判定并修正相关注释/测试名，或将子树递归真正接到"组内 isolate 变更也走重指派"的后续扩展（此时死代码才转正）。

### nit3. `reconcile_into` 组分支的内联注释 "Algorithm 7 随 M2-PR4" 现已成事实矛盾

**位置**：`crates/cordis-loader/src/lib.rs:384-385`

**事实**：组分支注释 "组自身 isolate 变更 → 整棵重建（M2-PR3 边界；Algorithm 7 随 M2-PR4）"——M2-PR4 即本 PR，业已落地，但组 isolate 变更**仍**走整棵重建（且组 isolate 本就不应用，见 `entry_ctx` 组分支 `:479-484` 只 derive + intercept）。注释把"Algorithm 7 补组 isolate"描述为未来事项，未说明 M2-PR4 落地后组 isolate 变更依然重建（且因组 isolate 不应用，该重建本质是**空转**的整树拆除再实例化）。

**影响（非 block/major）**：注释陈旧/表达含混，未点明"组 isolate 不应用（M2-PR3 边界③）→ 其 isolate 变更的重建是浪费而非语义必需"。建议改写为"组 isolate 不应用（M2-PR3 边界③），变更仍走整棵重建（冗余，可后续收敛为 no-op）"。

---

## 正面确认（实现正确的点）

### Algorithm 7 五步顺序自洽性（核心结论）

- **Δ 计算忠实**（`:614-622`）：`Δ = {(k, s1, s2) | s1 ≠ s2}`，`s1 = realm_of(旧 isolate)`、`s2 = realm_of(新 isolate)`，逐 `inject ∪ provide` 键遍历；`diff.is_empty()` 早退（`:623`）兜住 None↔None 退化（尽管该分支实际不可达——`patch_isolation` 仅在 `loaded.isolate != entry.isolate` 时调用，`:419`）。
- **ρ′ 设置时机正确（先 patch）**：`isolate_in_place` 在条目 ctx（`:633`）与子树各 fiber ctx（`:634-638`）上就地改写，**先于** refresh——refresh（`:641-646`）以新 ρ 重算目标，与论文 `entry.ctx[@@isolate] ← ρ′` 先于 reload 一致。
- **refresh 先于绑定迁移、且对自有绑定无影响**（对审查要点 1 时序问题的正面回应）：refresh 子树 fiber 时 `store[s1]` 尚在、`store[s2]` 空，但对**提供者自身**（注入为空或无受迁键注入）`compute_target`（`runtime.rs:486-503`）只依 `inject` 键的 `provider_of`，不读自身 provide 绑定 → target 不变 → 无状态扰动；**消费方**（外部依赖者）的 refresh 在 notify 阶段（`:669-680`）才发生，彼时 `store[s2]` 已就位、`store[s1]` 已空，重算正确。时序自洽。
- **绑定迁移条件正确**（`:648-664`）：`own ∧ store[s1] ∧ ¬store[s2]`；`own` 在移动**之前**快照为 `provider_of_realm(s1) ∈ subtree`（`:650-657`），因为移动后 s1 无绑定无法回查 own——快照时机正确。`move_binding` 的 `unwrap_or_else(panic)`（`:662`）在守卫 `!contains(s2)` + 快照保证下**不可达**，属防御式正确。
- **affected 谓词正确**（`:668-680`）：对外部 fiber，`own(D)=false`（`subtree.contains(&f.id())` 早退排除），谓词退化为 `∃(k,s1,s2,own). f.ctx().realm_of(k) ∈ {s1,s2} ∧ own`；`own(P)` 经 `prov_owns` 快照注入（闭包 `move` 捕获 `diff_clone` + `prov_owns` 引用，:`668-675`）——恰是论文 `resolve ∈ {s1,s2} ∧ own(D) ≠ own(P)` 在外部依赖者情形的正确特化（论文 affected 谓词对"依赖者 own 与提供者 own 不同"取真，外部 own(D)=false、own(P)=true ⇒ 通知）。

### own 语义与 delimiter 等价（审查要点 1 核心）

- **全局 realm 共享正确性已被守卫**（`:650-657`）：`provider_of_realm(s1)` 返回 `store[s1]` 绑定的**实际提供者 fiber**，`subtree.contains(&p)` 判定其是否属于本条目子树——若全局 realm `global:db:val` 的绑定由**其他条目**（非本子树）提供（例：本条目只是 inject 该 realm 的消费者，或提供者在子树外），`own=false`，**不迁移**。这精确对应审查要点 1 的关键约束"全局 realm 共享时绑定提供者属其他条目不得迁移"。`move_binding` 保留提供者字段（`store.rs:121-131` 整体迁移 `Binding` 含 `provider`），迁移后 `σγ` 归属（Def 45）不变。
- **子树成员 vs delimiter δk 的可观察等价**：叶子条目的子树 = 自身 fiber，own 判定退化为"该 realm 绑定的提供者是否为本 fiber"，与式 (65) `γ′[δk]=d1 ⟺ 绑定派生自条目 ctx` 在叶子场景可观察等价（机制不同、语义一致——已如实记入 THEORY-MAP PR #20 审查行 "公开差异声明（机制等价替代）"）。
- **ρ 拷贝继承 → patch 遍历子树的适应记录准确**（`:634-638` + THEORY-MAP PR #20 审查行）：本实现 `derive` 派生 ctx 拷贝 ρ 表（非论文持久化结构共享），故重指派须遍历子树各 fiber ctx 就地改写，逐条 `isolate_in_place`——适应记录与实现一致。

### entry_ctx 重构无回归

- `make_loaded`（`:434-474`）统一经 `entry_ctx`（`:478-504`）构造 `LoadedEntry.ctx`，重建路径与新建路径同源；`entry_ctx` 的叶子/分支分支与 M2-PR3 的 `annotated_ctx`/组 intercept 逻辑一一对应，仅把"派生 ctx"提升为可持久化字段 `LoadedEntry.ctx`（`:225`）供 Algorithm 7 就地 patch——`Clone` 派生（`:216`）与 `Rc<Context>` 共享语义相容（LoadedEntry 的 `Clone` 供 `apply_into` `loaded.get(&id).cloned()` 快照，`:332`/`:347`）。
- **disabled 条目无 fiber 但 isolate 变更只更新记录**（审查要点 2）：`patch_isolation` 的 `expect("patch 仅在已激活条目")`（`:606`）依赖"disabled 条目不进入 reconcile 的 isolate 分支"——`reconcile_into` 在 `:379-381` 对 `entry.disabled` 早退、`:365-377` 对 disabled 切换走 unload/重建，故 **disabled 条目不会与 isolate 变更同时到达 `patch_isolation`**。但存在一条需注意的路径：isolate 变更时若条目恰为 disabled，`apply_into` 阶段 1 的 `rebuilding` 判定（`:336-337`）已**不含** isolate（本 PR 注释 `:338-339` 明确 isolate 不走卸载侧），disabled 条目在阶段 2 落入 `reconcile_into` 的 `:379-381` 早退而 isolate 变更**被忽略**（记录不更新）——这与 M2-PR3 `disabled_period_changes_take_effect_on_reenable` 测试确立的"disabled 期间变更不更新记录、reenable 后以新 entry 实例化"的最终一致语义**自洽**（isolate 变更随下次 enabled 重建生效）。无回归，仅记录该语义（disabled 期间 isolate 变更亦延迟至 reenable）。

### 测试强度

- `isolate_change_reassigns_realms_without_rebuild`（`:1344-1415`）：完整行使绑定迁移（断 `global:db2:val` 在、`global:db:val` 不在）、**fiber id 不变**（不重建）、依赖者 c 停用（affected 通知）→ c 亦迁 realm 重激活（`c_first` 不变）。非假阳性：断言 fiber id 相等可区分"重建"与"重指派"。
- `isolate_change_moves_group_child_binding`（`:1420-1470`）：组内子条目 a 的绑定迁移 + 外部消费者 c 停用，行使 `subtree.contains(&p)` 的 own 判定分支（叶子在组内，own 仍为真）。
- `move_binding` 的前置条件（`AlreadyBound`/`NotBound`）虽无直接单测，但经 `store.rs` 既有 `duplicate_bind_rejected_without_mutation`/`unbind_on_missing_key_is_not_bound` 的姊妹语义 + `patch_isolation` 守卫间接覆盖；`runtime.provider_of_realm`/`notify_affected` 无独立单测，经 loader 集成测试行使。

### 文档回填一致性

- THEORY-MAP PR #20 行（`:145`）与 PR #20 审查行（`:146`）准确概括实现与适应记录；PR #19 行（`:144`）把 "isolate 变更走重建" 括注升级为 "**PR #20 升级为 Algorithm 7**"；PLAN M2 进度行（`:313`）加 "PR #20：托管 realm + Algorithm 7 重指派——HMR 主目标未开始"。三处回填与实现一致（loader 源码内联 doc 的滞后另见 nit1/nit3）。

---

## 总结

- **blocker**：无。
- **major**：major1（isolate 边界 None↔Some / Local 迁移无测试行使——机制正确性仅经推断；可判定为可接受后放行或补测）。
- **nit**：nit1（module 头 + `apply` doc 三处 "isolate 变更走重建" 旧表述未跟进）、nit2（`collect_subtree_ids` 对叶子恒单元素，子树递归为死代码、文档/测试名过度概括）、nit3（组分支内联注释 "Algorithm 7 随 M2-PR4" 成事实矛盾）。

**结论：有条件通过。** 置信度：高——逐行审读全 1471 行 loader + `context.rs`/`store.rs`/`runtime.rs` 语义对照，实跑 `cargo test --workspace` 全绿（0 失败，含 wasm 后端）、16 loader 测试全绿、hello-plugin 构建成功、core/loader fmt 干净、clippy 干净（仅 cordis-hmr 历史警告与本 PR 无关）。Algorithm 7 五步顺序、own 语义（含全局 realm 共享键的"非本子树不迁移"守卫）、affected 谓词、entry_ctx 重构无回归、delimiter→子树成员等价、ρ 拷贝继承→遍历子树的适应记录，均确认无误。3 项 nit 为文档/覆盖精确性（不阻塞合入）；major1 为边界覆盖缺口（功能上判无 bug、未行使），建议补一个 None↔Some 或 Local 迁移用例后合入，或明确记录为该边界"待补测"的公开差异。
