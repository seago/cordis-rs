# 代码审查报告：commit `24bfab5`（PR #19 / M2-PR3——loader 全字段 + 配置树 + group/include）

- **审查对象**：`24bfab5e8195edba58eeaba688b1cff30ef8fcda`（`crates/cordis-core/src/context.rs` +54/−0、`crates/cordis-loader/src/lib.rs` +748/−129）及配套 docs 提交 `b062f792059704695b948a471201c113736c271d`（`docs/THEORY-MAP.md` +4/−0、`docs/PLAN.md` +2/−1、`docs/reviews/REVIEW-32a913d.md`、`docs/reviews/REVIEW-6e0fd1e.md` 入库）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show 24bfab5` / `git show b062f79` 逐行核对 diff；读 `context.rs`（`derive` / `intercept_set` / `intercept_clear` / `intercept_set_boxed`）、`loader/src/lib.rs`（`Entry` / `LoadedEntry` / `Loader::{apply,apply_into,reconcile_into,make_loaded,instantiate_leaf,instantiate_group,annotated_ctx,apply_intercept,unload_from,teardown}`）、`runtime.rs`（`register` / `remove_fiber` / `unload` / `retire`）、`fiber.rs`（`retire` / `ctx` / `parent`）；对照 THEORY-MAP「PR #19 行 · 处置清单 · §5.2.1」与 PLAN M2 行。**实跑**：在干净 worktree（`git worktree add --detach /tmp/cordis-review-19 b062f79`，避开工作树并发 M2-PR4/PR #20 中间态）先行构建 4 个 Rust wasm guest + 复用原仓库 Go `guest.wasm`，`cargo test -p cordis-loader`（**15 passed / 0 failed**）、`cargo test --workspace`（除 go_guest/bridge_core 因 guest 产物缺失外全绿——见「回归」节，非 PR #19 缺陷）、`cargo fmt --all -- --check`（exit 0）、`RUSTFLAGS="-D warnings" cargo clippy -p cordis-loader -p cordis-core --all-targets`（干净）。

---

## 结论：**有条件通过**

PR #19 实质落地了论文 §5.2.1 的 loader 全字段（`Entry` 补 Def 74 的 `isolate`/`intercept`）与配置树（`group`/`include` 分支条目 + keyed diff + 级联拆除），核心语义正确：两阶段协调（先卸载侧再实例化侧）保证同供给替换单次 `apply` 完成、组持有者 fiber 承载子条目（`π = 组 fiber`，Def 47 注册正确）、拆除顺序（先 retire 级联、再自底向上 `remove_fiber`）满足 O-Remove 的 HasChildren 前提、keyed diff 幸存子条目 fiber id 不变、`intercept_set_boxed` 就地替换 + `intercept_clear` 移除不触发 reload、Local/Global realm 派生（`annotated_ctx` keys = inject ∪ provide）与满足判定（`provider_of` 经 fiber ctx ρ）正确。发现 **1 项 major**（intercept 移除回退语义缺口 + 文档过度声称，见下）与若干 nit，均不阻塞合入但 major 应随本 PR 或紧随 PR 修正。

> **范围说明**：审查基准为 PR #19 的**已提交状态**（`24bfab5` + `b062f79`，loader 1192 行）。审查期间发现**主仓库工作树存在并发开发**的未提交 M2-PR4（Algorithm 7 realm 重指派：`patch_isolation`/`LoadedEntry.ctx` 字段，loader 已增到 1403 行）且其一度因编辑中间态导致 `cargo test --workspace` 报 `missing field ctx` 编译失败——**此非 PR #19 缺陷**。为隔离，在干净 worktree（`b062f79`）复现全绿（见「验证手段」），结论不受并发工作干扰。

---

## 🔴 blocker / 🟠 major

### major1. `intercept_clear` 的「回退到继承值」语义缺口——移除条目 intercept 注解不会恢复父（组）的继承拦截值，直接落 `None` → 仅剩组件声明；与 `intercept_clear` 文档声称冲突，且未列入已知边界、无测试覆盖
**位置**：`crates/cordis-core/src/context.rs:422-427`（`intercept_clear` + 其 doc）；`crates/cordis-loader/src/lib.rs:559-568`（`apply_intercept` 的清除分支 `intercept_clear`）；模块文档已知边界 `crates/cordis-loader/src/lib.rs:24-32`（4 条，未含此项）

**事实**：
1. `Context` 的派生族（`derive`/`derive_for_fiber`/`isolate`/`intercept`）对 `ι` 均采用 `clone_intercept` **深拷贝快照**（`context.rs:556-564`），**不维护父链**。`annotated_ctx`（`lib.rs:536-555`）先 `parent.derive()`（拷贝父 ctx 的 ι，含组拦截注解），再对条目每个 intercept 键 `intercept_set_boxed` **覆写**。
2. 因此「条目某键既有继承值（组拦截）又有条目自身覆写」时，子 ctx 的 ι 中该键只存**条目值**（覆写掉了继承副本）。当条目移除该拦截注解，`apply_intercept` 的清除分支（`lib.rs:563-567`）对 prev 有、desired 无的键执行 `ctx.intercept_clear(key)` → `self.intercept.borrow_mut().remove(&key)` → 该键**彻底消失**（`None`），`get_meta` 落入「仅组件声明」分支——**而非继承的组拦截值**。
3. `intercept_clear` 的 doc（`context.rs:422-424`）明示「移除条目注解时**回退到继承值**（继承值在派生时已拷贝——清除后该键回到无元数据状态）」——前半句「回退到继承值」与后半句「回到无元数据状态」**自相矛盾**，且前半句在 flat-copy 模型下**不可实现**（派生是拷贝非链接，被覆写后继承副本已丢失，`remove` 后无值可回退）。

**为何是 major 而非 nit**：(1) 这是本 PR 的**招牌特性**（§5.2.1 "intercept — updated in place" 的就地移除路径）；(2) 模块文档确立了「子条目继承组拦截注解」的语义（子 ctx 经 `holder.ctx().derive()` 继承组 ι，`make_loaded`/`instantiate_group` 均如此），而「子条目先覆写组拦截、再移除自身覆写」正是该继承语义下的自然操作，当前实现会**静默丢失组的拦截值**；(3) 文档**明确声称**了正确行为却未实现，属「表里不一」；(4) 4 条已知边界**未列入**此项；(5) 唯一测试 `intercept_annotation_applied_in_place_without_rebuild`（`lib.rs:1037-1046`）用**顶层条目**（无父拦截）验证「移除 → 回退到组件声明」，其断言 `Some(path_meta(&["/declared"], false))` 恰好是「remove → None → declared」的产物，**未行使**「子覆写组拦截再移除 → 应回退组值」的违规路径——测试无意中固化了 None 语义，却与 doc 的「回退到继承值」声称方向相反。

**影响（非 block）**：当前实现语义自洽（「条目 intercept 权威，移除键 = 该键在 ctx.ι 缺席 → get_meta 回退组件声明」），且与 flat-copy `Context` 架构（`isolate`/`intercept`/`derive` 均拷贝、无父链）一致。**二选一处置**：(a) 接受「remove → None」为定义语义，则修正 `intercept_clear` doc 去掉「回退到继承值」、只保留「清除后该键回到无元数据状态」，并在 loader 模块文档已知边界补第 ⑤ 条「移除条目拦截注解不回退父（组）继承拦截值」（因派生拷贝无父链）；(b) 实现真正的回退语义，需在 clear 时重派生自父并仅清除目标键——成本高（需在 `LoadedEntry` 记录父 ctx 或注解码），建议按 (a) 处理并顺带为组子场景补一个负向直证测试。**倾向 (a)**。

---

## ⚪ 细节（nit）

### nit1. `Loader::apply` doc 把就地更新 API 写成 `intercept_set`，实际代码用的是 `intercept_set_boxed`
**位置**：`crates/cordis-loader/src/lib.rs:262`（doc「`intercept_set`/`intercept_clear`」）；实际调用 `lib.rs:517/552/561` 均为 `intercept_set_boxed`

**事实**：`apply` 的 doc 注释在描述「intercept 变更 → 就地更新」时写 `intercept_set`/`intercept_clear`，但实现统一走**类型擦除版** `intercept_set_boxed`（条目注解为权威、无类型冲突检查、replace 语义）。两 API 语义不同（`intercept_set` 带类型冲突检查 + 替换，`intercept_set_boxed` 直接覆写），doc 指向了未使用的 API，易误导读者。改 doc 为 `intercept_set_boxed`/`intercept_clear` 即可。

### nit2. `Context::intercept_set`（typed）为公开 API 但全仓库无调用点、无测试——死 API
**位置**：`crates/cordis-core/src/context.rs:412-420`（`intercept_set`）

**事实**：`grep intercept_set` 全仓库仅 3 处命中——`context.rs:424`（定义）、`:443`（`intercept_set_boxed` doc 里的对照引述）、`loader/src/lib.rs:262`（nit1 的 doc 误引）。无任何 `intercept_set::<M>(...)` 实际调用，亦无单测。其 doc 自称「与 `intercept_set_boxed` 互补（typed，检查 + 替换）」，但目前只是暴露而未用的公开 API。非缺陷（公共 API 可保留供外部编排方用），但「互补」的定位缺少消费点佐证，建议要么补一个单测固化妆义（类型冲突 panic + 重放幂等替换），要么在 doc 注明「预留 API，loader 分派走 erased 版」。

### nit3. `reconcile_into` 的两处重建分支（叶子 component/revision/isolate、组 isolate）为**不可达死代码**——阶段一已卸载，防御性冗余
**位置**：`crates/cordis-loader/src/lib.rs:409-424`（叶子重建）与 `383-387`（组 isolate 变更整棵重建）

**事实**：`apply_into` 阶段一（`lib.rs:322-340`）已对「非 disabled 且 component/revision/isolate 变化」的条目执行 `unload_from`（`map.remove`），阶段二（`lib.rs:343-351`）随即按 `None` 走 `make_loaded` 重建。因此 `reconcile_into` 内的两个重建分支（叶子 `409-424`、组 isolate `383-387`）在**任何层级**（顶层经 `apply_into`、组内递归同样经 `apply_into`）都不可达——`reconcile_into` 只在「未变化 / disabled 切换 / 纯 intercept 变化」时被调，而「disabled 切换」由 `362-374` 处理、「disabled 未变」由 `376-378` 早退，重建条件永不满足。属**防御性冗余**：行为正确（阶段一 + `make_loaded` 已正确重建），但增加了认知负担，且若未来有人改阶段一条件导致二者漂移，死分支会成为隐蔽陷阱。建议删除这两个死分支（保留 `reconcile_into` 仅处理 disabled 切换 + intercept 就地 + 组子 diff），或加注释标明「由阶段一兜底，此处不可达」。

### nit4. `instantiate_group` 手工复刻 `annotated_ctx` 的 intercept 应用循环——本可复用
**位置**：`crates/cordis-loader/src/lib.rs:514-519`（`instantiate_group` 的 `let ctx = parent_ctx.derive(); for (key, meta) in entry.intercept.iter() { ctx.intercept_set_boxed(...) }`）对比 `536-555`（`annotated_ctx`）

**事实**：`instantiate_group` 的动作 = `parent_ctx.derive()` + 逐个 `intercept_set_boxed`，恰是 `annotated_ctx(parent, entry, &KeySet::new())` 的组场景特例（组 isolate 因 GroupHolder 空键而 no-op，拦截注解照常应用）。手工展开与 `annotated_ctx` 的拦截循环重复，两点风险：未来改 `annotated_ctx` 的拦截应用方式（如改为合并而非替换）时组路径会被遗漏；且「组拦截在哪应用」出现两处实现。建议 `instantiate_group` 直接调 `self.annotated_ctx(parent_ctx, entry, &KeySet::new())`（isolate 因空键自然 no-op），删除重复循环。

### nit5. 组内子条目的 retire 被注册两次（`register` 在派生 ctx 上的逆 + `instantiate_leaf` 在父 ctx 上的显式逆）——派生 ctx 累加器孤儿化，靠显式父效应补级联
**位置**：`crates/cordis-loader/src/lib.rs:500-507`（`instantiate_leaf` 的显式 `parent_ctx.effect(... retire ...)`）对比 `crates/cordis-core/src/runtime.rs:225-233`（`register` 内 `ctx.effect(... retire ...)`，`ctx` = `use_component` 传入的 annotated 派生 ctx）

**事实**：`instantiate_leaf` 里 `ctx = annotated_ctx(parent_ctx, ...)` 是一个**派生** ctx（`fiber = parent_ctx.fiber`），`ctx.use_component` → `register(ctx, ...)` 把 retire 逆注册在**派生 `ctx` 的累加器**上；随后 `instantiate_leaf` 又显式在 `parent_ctx` 上注册 retire 逆。前者（派生 ctx 累加器）在 `instantiate_leaf` 返回后随 `ctx`（`Rc`）一起 drop，**从不执行**（孤儿逆，仅持有一个 `Rc<Fiber>`，随 drop 释放）；后者（父 ctx 累加器）才是组退役级联（`holder.retire → holder.ctx.dispose_all → 逆 retire 子`）的真正通道。二者 retire 均幂等，故**无功能缺陷**，但「派生 ctx 累加器白注册一条永不执行的 dispose 闭包」是 `register` 架构与 loader 显式级联的**结构性冗余**，值得在 `instantiate_leaf`/`instantiate_group` 的级联注释里点明「register 的逆落在派生 ctx（孤儿），此处的显式父效应才是组级联通道」，避免后人误以为重复或漏删其一。

### nit6. 无「组内同供给替换」测试——两阶段协调在组内层面的正确性仅有顶层直证
**位置**：`crates/cordis-loader/src/lib.rs:841-877`（`same_supply_replacement_in_single_apply`，顶层 root 场景）；组内测试 `group_keyed_diff_preserves_surviving_children`（`1090-1115`）供给不相交（db=val、sum=sum）

**事实**：审查点「同供给替换在组内是否仍成立」由 `apply_into` 的两阶段**递归**保证（组内子列表 diff 走同一 `apply_into`，阶段一卸载旧供给、阶段二实例化新供给），代码路径正确，但**无组内负向直证**——组内两个子条目先后用不同组件提供同一键（如 `val`）替换，单次 `apply` 不应 `ProvisionClash`。现有 `same_supply_replacement_in_single_apply` 只在顶层验证。补一个组内同供给替换测试（组内 child X 换 child Y）能固化「两阶段递归」这一本 PR 的核心不变量，防未来阶段逻辑重构破坏组内场景。

### nit7. `reconcile_into` 组分支的 `l.config`/`l.revision` 更新实为无操作（阶段一已兜底 revision 变更）
**位置**：`crates/cordis-loader/src/lib.rs:398-399`（组拦截就地更新后的 `l.config = ...; l.revision = ...`）

**事实**：组 `revision` 变更已在 `apply_into` 阶段一触发整棵重建（`lib.rs:333-339` 的 `rebuilding` 含 `l.revision != entry.revision`），故 `reconcile_into` 组分支执行到 `398-399` 时 `l.revision == entry.revision`（相等赋值 = no-op）；`l.config` 虽可能被调用方换了新 `Rc` 值但未递增 revision，赋值只是记录不生效（`GroupHolder.apply` 忽略 config）。与 `reconcile_into` 叶子拦截分支（`431-432`）的 `l.config` 更新同型（防呆记录）。无害，但与 nit3 同源（防御性冗余、认知负担），建议随 nit3 一并审视。

---

## ✅ 正面确认

1. **两阶段协调递归正确**（`apply_into` `lib.rs:322-351`）：阶段一先枚举 `loaded` 键、对消失/禁用置位/需重建条目 `unload_from`（释放供给名），阶段二再 `make_loaded`/`reconcile_into`。同供给替换（desired 用 Y 替换提供同一键的 X）在单次 `apply` 内先释放 X 的供给再实例化 Y，规避 `ProvisionClash`——`same_supply_replacement_in_single_apply` 直证，且该两阶段**递归**（阶段一/二在 `apply_into` 内，组内子列表 diff 复用同一入口）保证组内同供给替换同样成立。
2. **组持有者 + π 正确**（`instantiate_group` `lib.rs:514-532` + `instantiate_leaf` `478-509`）：组 = 空组件 `GroupHolder`（无注入/供给/效应）fiber；子条目经 `annotated_ctx` 派生自 `holder.ctx()`（`derive` 保留 `fiber = Some(holder_id)`，`context.rs:310-318`），`register` 取 `parent = ctx.fiber = Some(holder_id)`——子条目 `π = 组 fiber`（Def 47 精确落地）。组内子条目额外在 `holder_ctx` 注册退役逆（`instantiate_leaf` 的 `parent_ctx.fiber().is_some()` 分支），组退役时经 `holder.ctx.dispose_all()` 级联 `O-Retire` 子（`runtime.rs:407-429` 的 `unload` 调用 `fiber.ctx.dispose_all()`）。
3. **拆除顺序正确**（`teardown` `lib.rs:579-594`）：先 `fiber.retire()`（同步级联退役子、释放供给），再自底向上递归 `teardown` 子（后序），最后 `remove_fiber(父 fid)`——满足 O-Remove 的 `HasChildren` 前提（`runtime.rs:257-259` 检查 `∃m. parent == Some(id)`）。`group_teardown_removes_subtree` 断言 registry 无残留 + 绑定全清直证。
4. **keyed diff 幸存子条目不重建**（`group_keyed_diff_preserves_surviving_children` `lib.rs:1090-1115`）：子列表按 `id` diff，幸存项 `reconcile_into` 时 component/revision/isolate 未变 → 仅拦截就地更新 + 递归子 diff，`fiber.id()` 不变（FiberId 绝不复用，`fiber.rs:21-25`）。借用管理正确：`holder` 先 `Rc` clone、`holder_ctx` 再 clone，`get_mut` 分语句取 `children`，`apply_intercept(fiber.ctx(), &l.intercept, ...)` 的共享借用与后续 `l.intercept = ...` 的可变借用被 NLL 正确分隔。
5. **intercept 就地替换语义正确**（`intercept_set_boxed` `context.rs:433-435` + `apply_intercept` `lib.rs:559-568`）：desired 键 `intercept_set_boxed`（replace、重放幂等，不做合并避免集合元数据重复增长——与 `intercept_in_place` 的合并语义互补）；消失键 `intercept_clear`。读路径 `get_meta` 经 `intercept_of`（携带侧类型不符 → `None`）一致；`intercept_annotation_applied_in_place_without_rebuild` 直证「就地更新 fiber id 不变 + 右偏合并（声明 ⊕ 条目注解，ι 优先）+ 移除回退声明」。
6. **isolate Local/Global realm 正确**（`annotated_ctx` `lib.rs:536-555`）：keys = `component.inject() ∪ provide()`（`instantiate_leaf` 组装 `lib.rs:490-494`），Local → `local:{entry_id}:{key}`、Global → `global:{name}:{key}` 逐键 `ctx.isolate(key, realm)`；满足判定经 `provider_of` 的 `ctx.resolve_realm(key)`（`runtime.rs:436-443`）——`isolate_local_creates_private_realms` 直证 Local 跨条目不可见（consumer Inactive）、Global 命名共享（consumer 激活）。
7. **`Context::derive` 原语正确**（`context.rs:308-318`）：继承 `ρ`/`ι`（`clone_intercept` 深拷贝）与 fiber 归属、空累加器——与 `derive_for_fiber`/`isolate`/`intercept` 同构，「隔离派生不污染父上下文」语义落地。
8. **`Entry::new` 签名保持**（`lib.rs:156-173`，5 参 `id/component/config/revision/disabled`）：`examples` 与 `crates/cordis-wasm/tests/{go_guest,dual_backend}.rs` 的既有 5 参调用零破坏（仅内部补 `isolate/None`、`intercept/Intercepts::new()`、`children/Vec::new()` 字段）。
9. **失败 fiber 静默加载边界已记录**（模块文档第 ④ 条，`lib.rs:28-32`，引 REVIEW-32a913d nit7）——与 PR #17 后 `use_component` 对组件失败返回 `Ok(fiber)`（`Inactive(Some(ζ))`）的语义衔接准确。

---

## 回归与卫生

- **测试**：`cargo test -p cordis-loader` **15 passed / 0 failed**（原 8 + 新增 7）；`cargo test --workspace` 在干净 worktree 中，除 `go_guest`（2）/`bridge_core`（2）外全绿，该 4 项失败**纯环境性**——`examples/wasm-plugin-rust/target/wasm32-wasip2/...` 与 `examples/wasm-plugin-go/guest.wasm` 均未被 git 跟踪（`.gitignore` 的 `target`、guest.wasm 未入库）、干净 worktree 无产物，测试 `.expect("先构建 guest...")` 即 panic；补齐产物（`cargo build --manifest-path ... --target wasm32-wasip2` + 复用原仓库 `guest.wasm`）后 `bridge_core`（2）、`go_guest`（2）、`isolated_wasm`（2）、`load_guest`（1）、`sandbox_isolation`（3）**全部 ok**。PR #19 仅改 `cordis-core/src/context.rs` + `cordis-loader/src/lib.rs`，不触碰任何 wasm 产物/测试，故**非回归**；原 8 个 loader 测试 + hello-plugin/dual_backend 等外部调用（`Entry::new` 5 参）不受影响。
- **fmt/clippy**：`cargo fmt --all -- --check` exit 0；`RUSTFLAGS="-D warnings" cargo clippy -p cordis-loader -p cordis-core --all-targets` 干净。
- **文档一致性**：THEORY-MAP PR #19 行（`Def 74, §5.2.1`，完成）、PLAN M2 行「进行中（首批任务：失败模型/拦截前置）…… HMR 主目标未开始」、loader 模块文档已知边界 4 条（双向写回未实现 / isolate 变更走重建 / 组条目 isolate 不应用 / 失败 fiber 静默加载）与实现（`derive` 变更是重建、组 isolate 不应用、`panic!` 于 `RegistryError` 而静默失败 fiber）逐条对应——除 major1 所述「移除 intercept 不回退继承值」**未列入**边界。
- **命名/注释**：`IsolateAnnotation::{Local,Global}`、`Intercepts`、`apply_into`/`reconcile_into`/`make_loaded`/`teardown` 与论文 §5.2.1/Def 74 术语对应清晰；`Debug for Intercepts`（`debug_set().entries(keys)`）、`Intercepts` 手写 `Clone`（`clone_box` 深拷贝）正确。

---

## 总结

- **blocker**：无。
- **major**：major1（`intercept_clear`「回退到继承值」语义缺口——移除条目拦截注解不回退父（组）继承值、doc 过度声称、未入边界、无负向测试）。
- **nit**：nit1（`apply` doc 误引 `intercept_set` 应为 `intercept_set_boxed`）、nit2（`intercept_set` typed 版死 API）、nit3（`reconcile_into` 两处重建分支不可达死代码）、nit4（`instantiate_group` 复刻 `annotated_ctx` 拦截循环）、nit5（组内子条目 retire 双重注册、派生 ctx 累加器孤儿化）、nit6（缺组内同供给替换测试）、nit7（`reconcile_into` 组分支 `config`/`revision` 更新为 no-op）。

**结论：有条件通过。** 置信度：高——逐行审读 `context.rs` 全 1049 行与 `loader/src/lib.rs` 全 1192 行语义对照，干净 worktree 上实跑 15 loader 测试全绿、`cargo test --workspace` 除 guest 产物缺失外全绿（补齐后全 ok）、fmt/clippy 干净。核心正确性（两阶段递归协调、组持有者 + π、拆除顺序、keyed diff、拦截就地替换、isolate realm 派生与满足判定、`Entry::new` 签名保持、已知边界记录）均确认无误；7 新测试覆盖拦截就地不重建/移除回退/Local/Global realm/组 keyed diff/组拆除/禁用重启用/include 嫁接/isolate 变更重建且非假阳性（Local/Global 的 consumer Active/Inactive 断言、keyed diff 的 fiber id 不变断言、isolate 重建的新 fiber id 断言均为实质判定）。唯一 major1 属「文档过度声称 + 未覆盖的继承回退语义」——不阻塞功能合入，但应随本 PR 或紧随 PR 以「改 doc + 补边界第 ⑤ 条 + 组子负向测试」落地（见 major1 处置 (a)）。
