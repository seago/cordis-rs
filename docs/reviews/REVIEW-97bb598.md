# PR #29 审查报告（97bb598 + b0b8eff）：G1 双向绑定 + G2 每键注入配置

> 审查员：独立 PR 审查（subagent）
> 范围：`97bb598`（code）+ `b0b8eff`（docs）
> 对照物：cordiverse/cordis v4（`/tmp/cordis-ts`）、论文 §5.2.1 / Def 30/31、`docs/TS-REFERENCE-GAP.md`

## 一、审查范围

- **代码**：`crates/cordis-core/src/fiber.rs`、`crates/cordis-core/src/runtime.rs`、`crates/cordis-core/tests/update_binding.rs`、`crates/cordis-loader/src/lib.rs`
- **文档**：`docs/TS-REFERENCE-GAP.md`、`docs/THEORY-MAP.md`
- **定向验证**：`cargo test -p cordis-core --test update_binding`（3 pass）、`cargo test -p cordis-loader`（27 pass）、`cargo +1.97.0 fmt --all -- --check`（干净）、`cargo +1.97.0 clippy -p cordis-core -p cordis-loader -- -D warnings`（干净）
- **逐条核实**了 `Fiber::update`/`update_fiber`/`unload`/`reload`、`update_entry`/`entry_of`/`find_loaded_mut`、`Entry.inject`/`with_inject`/`annotated_ctx`/`entry_ctx`、`Context::get_meta`/`intercept_set_boxed`，并对照 TS `fiber.ts:137-144/476` 与 `loader/index.ts:60-124` 的 `internal/update`/`internal/plugin` 瀑布。

## 二、逐条发现

### 与 TS 语义的对照核实（无缺陷，逐条记录事实）

1. **`Fiber::update` 就地重跑语义（对照 TS `fiber.ts:476`）** ✅
   - TS 序：`assertActive()` → `resolveConfig` → `waterfall('internal/update', config, …, () => { fiber.config = config; fiber._error = undefined; return fiber.restart() })`。即**观察者先以新 config 触发，再换 config，再 restart**（restart = `_setEpoch(INACTIVE)` 强制卸载 + `_refresh` 重算重载）。
   - Rust 序：`Fiber::update` 断言 `Active` → `Runtime::update_fiber` 先触发 `update_hook(fiber, config)` → 构造新 `apply` 闭包（闭包内捕获 `component` + `ctx`，以 `config.as_ref()` 惰性绑定）→ `unload(fiber)`。
   - `unload` 逆转当前全部效应（`dispose` LIFO + `ctx.dispose_all`）→ 依赖者先级联停用（Thm 63 序）→ `target` 未变（换闭包不动 `inject`/`retired`，`compute_target` 结果不变）→ 链式 `reload` 依赖者级联恢复。**fiber 身份全程保留（同一 `Rc<Fiber>` 未重建）**。与 TS `restart()` 的"先 INACTIVE 再 refresh"行为语义等价。
2. **失败路径同 `reload`（L-Raise → `Inactive(ζ)`）** ✅
   `update_fiber` 只是换 `apply` 后调用 `unload`，重跑经 `reload` 的同一条 `catch_unwind`/`FiberError::raise` 路径——失败置 `target ⊥` + `unload` 恢复已完成步骤 + `Inactive(Some(ζ))`。与声明一致。
3. **`assertActive` 同型性** ⚠️ **存在一处语义级差异（见 major-1）**：Rust 断言 `matches!(Active {..})`，TS `assertActive` 判定 `uid !== null`——TS 允许对"已失败但仍注册"（`_error` 置位、`uid` 非空）的 fiber 调用 `update` 以**清除 `_error` 并重启恢复**；Rust 用 `Inactive(Some(ζ))` 表示失败终态，被 `update` 的断言排除在外，即 **Rust 无法经 `update` 复活一个失败 fiber**。
4. **`apply` 字段 `RefCell` 化的波及面** ✅
   `apply` 仅在两处读写：`register`（构造时 `RefCell::new`）与 `reload`（`(fiber.apply.borrow())()`）。`update_fiber` 新写入点 `*fiber.apply.borrow_mut() = …`。单线程 `Rc` 宿主、无重入路径，`borrow`/`borrow_mut` 不冲突；`use std::any::Any` import 补充正确。
5. **loader `update_entry` revision 语义诚实性** ✅
   - 步骤 1 先写条目书签（`find_loaded_mut` 递归整棵树），步骤 2 仅当 `Active` 且未退役时 `fiber.update`。`fiber(id)` 对"组 = 持有者 fiber"的语义由现有 `find_fiber` 承接。
   - **不递增 revision**，并经测试 `update_entry_replaces_config_in_place` 直证：同 revision 的后续 `apply` 走 no-op 分支、**不清除 fiber 层写回**（值保持 `pg2`），而书签被 `reconcile` 的 no-op 分支回映为 desired 的 `pg`——"书签 = 协调记录、非权威源"这一诚实语义已直接钉死，非巧合。
6. **`register_update_hook` 反查（`entry_of`/`find_loaded_mut`）** ✅
   `entry_of_in` 对每个 `LoadedEntry` 先比 `l.fiber.is_some_and(|f| f.id() == fid)` 再递归 `children`；组持有者 fiber 与子条目 fiber 均被命中（`group_child_self_update_maps_to_nested_entry` 直证嵌套命中）。`find_loaded_mut` 用 `contains_key` → `get_mut` 两段，绕开 `map.get_mut(id)` 与递归 `values_mut()` 的借用冲突（若 `get_mut` 直接命中则无需再借 `values_mut`），borrow 处理正确。
7. **`Entry.inject` 应用位置与遮蔽序（对照 `fiber.ts:137-144`、Def 30/31）** ✅
   - TS：`this.ctx[Context.intercept][name] = config`（原型链拷贝后**覆写**同键 intercept）——inject 遮蔽 intercept。
   - Rust：`annotated_ctx` 先 `intercept_set_boxed`（intercept）后 `intercept_set_boxed`（inject），`intercept_set_boxed` 为 replace 语义 → 同键后应用遮蔽。`entry_ctx` 组分支同样先 intercept 后 inject。`intercept_shadows`（实际是 inject 遮蔽 intercept）测试直证。
   - 组条目经 `derive` 拷贝 `ι` 表传给子条目（`context.rs` `derive`/`clone_intercept` 扁平拷贝）→ 组 inject 继承给子条目（`group_inject_inherits_to_children` 直证）。
   - 读取方 `get_meta` 右偏合并：`declared μ` 与 `carried ι` 经 `M::merge(&mu, &iota)`（`ι` 优先），与 Def 30/31 `get(k, μ) = σ(k)(μ ⊕ₖ ι(k))` 一致（`context.rs:443-451` 既有实现，未改动）。
8. **`update_fiber` 观察者序与 panic 处理** ✅
   观察者（`update_hook`）在换闭包**之前**以新 config 触发（`update_fiber` 首行），与 TS `waterfall('internal/update', …)` 先于 `fiber.config = config` 一致。观察者 panic 未捕获（`set_update_hook` 文档注明"回调 panic = 宿主 bug（传播）"）——但**未注明"宿主 bug"而非"组件失败"这一约束**（见 nit-1）。

### major

- **major-1（语义差异：失败 fiber 不可经 `update` 复活，未文档化、未经测试）**
  - **位置**：`crates/cordis-core/src/fiber.rs:201-207`（`Fiber::update` 的 `Active` 断言）
  - **理由**：TS `assertActive()`（`fiber.ts:224-227`）判定 `uid !== null`，且 `Fiber::update` 在 restart 回调中显式 `fiber._error = undefined`——即 TS 允许对**已失败（`_error` 置位）但已注册**的 fiber 调 `update` 以清除错误并重启；Rust 以 `Inactive(Some(ζ))` 为失败**终态**，`update` 的 `matches!(Active {..})` 断言将其排除，用户无法"换 config 复活"失败组件（必须先换出/重建条目）。这与 TS 参照实现的行为是一个**真实**（虽小）的语义差，而 PR #28 的 G1 声明口径是"TS 有完整参照"，此处宜文档化公开差异。同时存在一个合理的反向论点：Rust 把失败建模为 `Inactive(ζ)` 终态、`ζ` 为协变静态度量（`is_quiet` 恒静止），"不让 update 复活失败态"可能是更干净的抉择——但这一抉择**既未在文档（TS-REFERENCE-GAP / THEORY-MAP / fiber.rs doc）中声明为公开差异，也无测试钉死**，属"语义差异 + 文档与代码口径不一致"的双重缺口。
  - **建议修法**（择一）：(a) 若采纳 TS 语义——将断言放宽为 `Active | Inactive(Some(ζ))`，并在 `update_fiber` 换闭包启动重跑前清除失败 outcome 的途径（Rust 的失败态由 `target = ⊥` 承载，需同时把 `target` 从 ⊥ 恢复为可计算值，即换闭包后 `refresh` 而非直接 `unload`），并补一条"失败 fiber 经 update 复活"测试；(b) 若坚持当前语义——在 `Fiber::update` doc 与 `docs/TS-REFERENCE-GAP.md` 显式记录"失败 fiber 不可经 update 复活（与 TS `_error = undefined` 恢复行为不同），复活须重建条目"，并补一条 `update_on_failed_fiber_panics` 测试把该差异钉死为契约。

### nit

- **nit-1（观察者 panic 语义注明不精确）**
  - **位置**：`crates/cordis-core/src/runtime.rs:371-377`（`set_update_hook` doc）
  - **理由**：doc 写"回调 panic = 宿主 bug（传播）"，但未点明**为何**是宿主 bug 而非可恢复的 `FiberError`：观察者在 `reload` 的 `catch_unwind` 之外同步执行，且无 `FiberError::raise` 语义约定——若未来实现者误在观察者内 `raise`，会被当作普通 panic `resume_unwind`，不符合"组件失败可恢复"的心智。属表述层面可补一句澄清，非功能缺陷。
  - **建议修法**：doc 补一句"观察者运行于 `reload` 的 `catch_unwind` 之外，不参与 L-Raise 恢复通道；实现者不得在观察者内 `FiberError::raise`"。
- **nit-2（loader 模块级文档边界① 已过时，未随 G1 更新）**
  - **位置**：`crates/cordis-loader/src/lib.rs:24-30`（模块文档「已知边界①」）
  - **理由**：该段仍写"① 双向写回未实现——条目为权威记录，组件不能自行改配置…M3-PR3 评估结案…写回方向属编排责任…公开差异关闭"，与本 PR 新落的 `update_entry`/`register_update_hook`（组件→条目写回已实现）直接矛盾——`Fiber::update` + 观察者已让组件侧改配置并写回条目书签（`fiber_self_update_writes_back_through_loader_hook` 测试钉死）。THEORY-MAP 处置⑩ 已重开并标记部分落地，但 loader 模块文档的边界① 残留旧结论，属文档与代码不一致。
  - **建议修法**：把边界① 改写为"G1 已落地（PR #29）：`update_entry`/`register_update_hook` 提供组件→条目写回通道；**剩余** self-dispose → `disabled` 写回（TS `internal/plugin` 半段）尚未实现"，与 THEORY-MAP / TS-REFERENCE-GAP 口径对齐。
- **nit-3（`LoadedEntry` 未存 `inject`，变更纪律仅靠文档约定、无结构兜底）**
  - **位置**：`crates/cordis-loader/src/lib.rs:251-266`（`LoadedEntry`）+ `with_inject` doc（第 229-236 行）
  - **理由**：`LoadedEntry` 只存 `intercept`（reconcile 可用 `apply_intercept` 就地差分），而 `inject` **只在 `make_loaded`/`entry_ctx`/`annotated_ctx` 实例化期消费一次、未存入记录**；因此同一个 `revision` 下改变 desired 的 `inject` 会被 `reconcile` 的 no-op 分支静默忽略（revision 未变 → 不重建 → 不重跑 `annotated_ctx`）。PR 用 doc（`with_inject` 注明"变更须随 revision 递增触发重建"）陈述了这一纪律，语义上诚实、也与 config 同纪律一致——但相较 `intercept` 有结构 + 就地分派，`inject` 的"变更即丢失"更容易踩坑，且无负例测试掩护（现有 6 条 loader 测试全走初始 apply，无"同 revision 改 inject 被忽略"的钉死）。
  - **建议修法**：补一条负例测试（同 revision 改 `with_inject` 元数据 → 断言读到的仍是旧值/不生效），把"revision 代行"纪律钉死为可执行契约；若未来要给 inject 就地分派，再为 `LoadedEntry` 增 `inject` 字段即可（非本 PR 必需）。
- **nit-4（`finder_load_mut` 先 `contains_key` 后 `get_mut` 的二次哈希，纯风格）**
  - **位置**：`crates/cordis-loader/src/lib.rs:905-909`
  - **理由**：`contains_key` + `get_mut` 做两次哈希查找，可用"先尝试 `get_mut`、未命中再 `values_mut()` 递归"的一段式写出等价的正确借用（逻辑无误，仅微优化/可读性）。非缺陷，可不动。

## 三、总体结论

**需修复 major（1 项 major-1）**。

G1 双向绑定与 G2 每键注入配置的主体语义**正确且诚实**：`Fiber::update` 就地重跑保持身份、依赖者级联、失败 L-Raise 同路径；观察者序符合 TS `internal/update` 瀑布（先写回后重启）；`apply` 改 `RefCell` 波及面干净；`update_entry` 的"不递增 revision / 书签回映 desired / 协调记录非权威源"语义被测试直证而非巧合；`Entry.inject` 应用位置（intercept 之后遮蔽同键）、组派生链继承、`get_meta` 右偏合并均与 TS 与 Def 30/31 一致。测试质量实打实（core 3 + loader 6 均直证声明语义，含"同 revision apply 不清除写回"这一最易巧合通过的关键场景）。fmt/clippy（1.97，`-D warnings`）干净、零第三方依赖新增、docs 主文档（TS-REFERENCE-GAP / THEORY-MAP）已同步。

唯一 major 是 **失败 fiber 无法经 `update` 复活**这一与 TS `assertActive`/`_error = undefined` 的语义差既未文档化也未测试钉死——属"语义差异 + 文档口径不一致"，需作者明确抉择（采纳 TS 语义并补复活测试，或将差异声明为公开差异并补钉死测试）。其余 4 项 nit 均非功能缺陷，建议一并修订（其中 nit-2 的 loader 模块文档边界① 残留"写回未实现"旧结论，与代码直接矛盾，宜优先在 major 修订时同改）。

在 major-1 明确处置（并同步 nit-2 文档）前不建议直接合入；修复后可作为「通过」。
