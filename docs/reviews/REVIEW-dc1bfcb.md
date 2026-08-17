# REVIEW-dc1bfcb —— PR #31（G3：per-key isolate 粒度）

## 审查范围

- **提交**：`dc1bfcb`（code：`crates/cordis-loader/src/lib.rs`）+ `656700e`（docs：`THEORY-MAP.md`、`TS-REFERENCE-GAP.md`）。
- **参照**：`/tmp/cordis-ts/packages/loader/src/config/isolate.ts`（`isolate?: Dict<true | string>`，逐键 Realm access）。
- **核对方式**：`git show dc1bfcb --stat` / 全文 diff，通读当前 `lib.rs`（entry_ctx / annotated_ctx / patch_isolation / realm_of / reconcile_into / make_loaded / tests）与 `cordis-core/src/context.rs`（`derive` / `isolate` / regions 传播），运行 `cargo test -p cordis-loader`、`cargo fmt --check`、`cargo clippy -p cordis-loader --all-targets -- -D warnings`。
- **验证结果**：loader 34 测试全绿；fmt/clippy 干净；零第三方依赖（仅 `lib.rs` 改动，`Cargo.toml` 无 diff）。

## 逐条发现

### 1. 类型与 API —— 通过

- `Entry.isolate: BTreeMap<Symbol, IsolateAnnotation>` 与旧 `Option<IsolateAnnotation>` 的语义迁移正确：`with_isolate(key, iso)` 逐键 `insert`，同名键后写覆盖（最近注解优先的自然载体）。
- `realm_of(&BTreeMap, id, key)` 逐键查表，`None => key`（裸键 realm）与 TS `access` 的 `if (!label) return`（无注解键不隔离）同型。
- realm 命名不变（`local:{id}:{key}` / `global:{name}:{key}`），既有断言（`local:p:val`、`global:db:val` 等）全部保持。
- `patch_isolation` Δ 键域 = `inject ∪ provide ∪ entry.isolate.keys ∪ loaded.isolate.keys`（`sort_unstable + dedup`），与声明键域一致扩展。

### 2. 应用路径 —— 通过

- `annotated_ctx`（叶子）：`parent.derive()` 后逐键 `ctx = ctx.isolate(*key, realm)`，其内部 `derive`（context.rs L393-401）拷贝 `ρ`（realms）表，故父（组）realm 经派生链继承给子条目确认成立。
- `entry_ctx` 组分支：同样逐键应用后返回派生 ctx；`make_loaded` 组路径经 `instantiate_group(entry, &ctx)` → `annotated_ctx(entry, ctx)`（再次派生+幂等应用）→ `holder.ctx()` 递归子条目。子条目在 `annotated_ctx` 中先继承组 realms、再 `isolate(*key, ...)` 覆盖（`insert` 后写胜出）= 最近注解优先。**⑪ 收口论断诚实**：拷贝继承替代了 effective-isolate 穿透，无 realm 脱同步风险。

### 3. Algorithm 7 适配 —— 通过

- Δ 键域扩展正确覆盖"isolate 映射键非组件声明键"：`realm_of` 对未声明但被 isolate 映射的键返回隔离 realm，新旧映射删增均触发 diff。`isolate_change_boundaries_none_to_global`（bare→Global）直证该路径。
- 逐键 `isolate_in_place`（条目 ctx + 子树各 fiber ctx）、`move_binding`（own 判定）、`notify_affected`（resolve ∈ {s1,s2} ∧ own）逻辑未被破坏——均为逐键循环，随 diff 键集驱动。
- 无声明键时组件 `inject()/provide()` 为空集，Δ 键域退化到「新旧 isolate 映射键」本身，语义自洽。

### 4. 协调语义 —— 通过

- 组分支 `loaded.isolate != entry.isolate`（`BTreeMap` 派生 `PartialEq`，键+注解全量比较）→ `unload_from` + `make_loaded` 整棵重建。**保守路径如实**：测试 `group_isolate_change_rebuilds_subtree` 直证子条目 fiber id 变化、绑定迁 realm。
- 叶子 isolate 变更走 `patch_isolation`（不重建），fiber id 不变——`isolate_change_reassigns_realms_without_rebuild` 直证。
- 组 isolate 变更与子列表 keyed diff 无交集：组 isolate 差异在进入子列表 diff 前即整体重建返回。

### 5. 测试质量 —— 通过（附一处语义收窄说明）

- 3 个新测试直证：`isolate_per_key_mixed_granularity`（同一条目 `a→Local`、`b→Global`）、`group_isolate_inherits_to_children_and_child_overrides`（继承 + 覆盖两段）、`group_isolate_change_rebuilds_subtree`（整棵重建语义）。
- 15 处既有 `with_isolate` 调用 per-key 化（键 = `val`）。断言均围绕 `val`（共享/私有依赖键），迭代后仍覆盖原意图。
- **说明（非缺陷，语义收窄）**：旧「全声明键等值隔离」下，`sum_consumer` 的提供键 `sum` 也被隔离（`local:c:sum` / `global:db:sum`）；per-key 化后仅 `val` 被隔离，`sum` 回到裸键 realm。既有测试对 `sum` 的 realm 无任何断言（消费者在相关场景要么 Inactive 不绑 `sum`、要么断言只看 `val`），故不可观测、断言不失真。但这构成 API 契约的**收窄**（从「全部声明键」到「仅映射键」），已由 `Entry.isolate` 字段文档（L145-150「只隔离映射中的键」）明示，属 PR 意图内变更，而非断言失真。

### 6. docs 一致性 —— **major**

`THEORY-MAP.md` 与 `TS-REFERENCE-GAP.md` 的改动正确（G3 完成标记、⑪ 收口行、PR #31 行均与代码一致）。但 **`crates/cordis-loader/src/lib.rs` 内多处 doc 注释仍保留旧的 M3-PR3 结论，与新代码及同 PR 的 docs 改动直接矛盾**：

- **模块级 doc（L37-44）**：仍写「组条目自身的 isolate 注解不应用（组无声明键……）——**M3-PR3 评估结案**……实现中组 isolate 因 GroupHolder 空键自然 no-op……effective-isolate 穿透……记录为公开差异」。这与本 PR 实现的「组 per-key isolate 经派生链继承给子条目」以及 THEORY-MAP ⑪ 的「已收口」**正面冲突**。
- **`entry_ctx` 组分支注释（L669）**：仍写「isolate 无声明键可应用，M2-PR3 边界」——现已逐键应用。
- **`instantiate_group` 注释（L747-748）**：仍写「组 isolate 因 GroupHolder 空键自然 no-op」——已非如此（组 isolate 经派生链继承）。
- **`IsolateAnnotation` 枚举 doc（L74）**：仍写「应用于条目组件**全部**声明键（`inject ∪ provide`）」——已非「全部」，而是「仅映射键」。

上述为**语义错误/文档矛盾**，属 major。

### 7. 纪律 —— 通过

- `cargo fmt --check -p cordis-loader` 退出码 0。
- `cargo clippy -p cordis-loader --all-targets -- -D warnings` 退出码 0。
- 零第三方依赖：本 PR 仅改 `lib.rs`（`BTreeMap` 为标准库），无 `Cargo.toml` diff。

## 总体结论

**条件通过（1 major 文档矛盾 + 0 nit）**。功能实现与测试正确、诚实：per-key 粒度与 TS `Dict<true|string>` 同型、realm 命名与既有断言不变、Algorithm 7 Δ 键域扩展正确、⑪ 收口经派生链拷贝继承成立、组变更保守整棵重建如实、fmt/clippy/零依赖干净。

唯一 major 为 `lib.rs` 内四处残留的旧 doc 注释（模块级 L37-44、`entry_ctx` L669、`instantiate_group` L747-748、`IsolateAnnotation` L74）仍断言「组 isolate no-op / 全声明键应用 / 公开差异」，与本 PR 代码及同 PR 的 `THEORY-MAP`/`TS-REFERENCE-GAP` 收口记录矛盾。建议同步改旧注释为「组 per-key isolate 经派生链继承、子条目覆盖；无注解键 = 裸键 realm」后合入。

- **major：1**（残留 stale doc 注释与代码矛盾）
- **nit：0**
- **结论**：条件通过（修复 4 处 stale doc 注释后合入）
