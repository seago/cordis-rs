# 代码审查报告：commit `6e0fd1e`（PR #18 处置①——interception 求值形态·元数据侧落地）

- **审查对象**：`6e0fd1efd853dcd1f58e060f5d7eeba542d36d15`（`crates/cordis-core/src/{component,context,fiber,runtime}.rs` + `crates/cordis-core/tests/interception.rs`，+348/−4）及配套 docs 提交 `247fc67`（`docs/THEORY-MAP.md` +5/−2、`docs/PLAN.md` +1/−1）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show 6e0fd1e` / `git show 247fc67` 逐行核对 diff；读 `context.rs`（`InterceptMeta`/`intercept`/`intercept_of`/`get_meta`/`declared_metadata_of`/`intercept_in_place`/`clone_intercept`）、`component.rs`（trait 全量）、`fiber.rs`（Fiber 结构 + 访问器）、`runtime.rs`（register 构造点）；对照 `cordis-wasm/src/lib.rs`（WasmComponent 实现）与 `cordis-macro/src/lib.rs`（derive 展开）确认新增默认方法不影响既有 Component 实现；对照 THEORY-MAP M0 走查 interception 行/PR #18 行/处置清单①/§6.3 行。实跑 `cargo test -p cordis-core --test interception`（**5 passed / 0 failed**）、`cargo test --workspace`（**全绿，0 失败**，含 wasm 后端 go_guest/sandbox_isolation 等）、`cargo fmt --all -- --check`（exit 0）、`cargo clippy --workspace --all-targets`（干净，exit 0）。

---

## 结论：**通过**

处置①（Def 30/31 interception 求值的元数据侧）实质落地：`get_meta` 的右偏合并方向与论文 Def 31「`ι(k)` 优先覆盖组件 `μ`」一致、与既有 `intercept` 操作式的「new 优先」经由 `merge(existing, new)` 契约自洽；`declared_metadata`（𝔇inter 默认 ε）+ `intercept_in_place`（§5.2.1 就地分派）语义正确；借用纪律、root 回退、Fiber 无环构造均无缺陷；provider 函数形态的公开差异记录准确；文档回填与实现一致。发现 4 项 nit（均不阻塞合入，见下）。

---

## ⚪ 细节（nit）

### nit1. `dom(d) ⊆ inject` 约束仅作文档声明、未强制，且未在 THEORY-MAP 记录为偏差
**位置**：`crates/cordis-core/src/component.rs:32`

**事实**：`declared_metadata` 的 doc 注释断言「`dom(d) ⊆ inject`（声明的 是依赖键的元数据）」——这是 Def 30 `𝔇inter` 的定义性约束，但实现仅为 trait 文档陈述：方法签名 `fn declared_metadata(&self, _key: Symbol) -> Option<...>` 接受**任意** `Symbol`，运行时对「组件为未注入键声明元数据」**无任何检查**（`get_meta` 走 `declared_metadata_of` 时也不校验 `key ∈ self.inject`）。测试组件 `MetaDeclaring` 恰好声明了注入键 `fs`（`inject()` 返回 `[fs()]`），故未行使到「声明非注入键」的违规路径。

**影响（非 block/major）**：`declared_metadata` 是**读时咨询**（`get_meta` 查询），非规格强制，语义上更接近「组件自述的元数据注释」。这本身是合理的设计取舍（强制 `dom(d) ⊆ inject` 需在每次 `get_meta` 时反查 `inject`，且元数据侧求值本就不消费 `inject` 语料），但**文档把约束当不变量陈述、却既不强制也不在 THEORY-MAP 记为偏差**，与仓库「表里如一」的纪律存在轻微口径落差。建议任选其一：(a) 在 doc 注释把「`dom(d) ⊆ inject`」改为「建议（声明的是依赖键的元数据），本实现不强校验」；(b) 或在 THEORY-MAP PR #18 行补一句「`dom(d) ⊆ inject` 未强制，为读时咨询的文档级约定」。

### nit2. `get_meta` 两条读路径的类型冲突纪律不对称：组件声明（panic）vs 上下文携带（None）
**位置**：`crates/cordis-core/src/context.rs:362-369`（`declared_metadata_of` 的 `panic!`）vs `:324-330`（`intercept_of` 返回 `None`）

**事实**：`get_meta` 组合两条读路径，但二者对「读取类型与实际元数据类型不符」的处置不同——`declared_metadata_of` 读**组件声明**时 `downcast_ref::<M>()` 失败 → `panic!`（错误即 bug 策略）；`intercept_of` 读**上下文携带** `ι` 时 `and_then(|downcast|)` 失败 → 静默 `None`。`get_meta` 的 doc（`:340-342`）称「值类型不符 → panic（与 interrupt 的类型纪律一致）」——这只描述了组件声明一侧，未点明携带侧的软 `None` 语义。调用方读一个「组件声明为 PathMeta、但 `get_meta::<PathMeta>` 正常、`get_meta::<OtherMeta>` panic」的键时，若同时又用 `intercept_of::<OtherMeta>` 读携带侧，会得到 `None` 而非 panic——两路径行为不一致。

**影响（非 block/major）**：该不对称**在语义上可辩护**——组件声明元数据是组件作者固定类型（类型不符 = 调用方编程错误，panic 有理），而携带 `ι` 是运行时动态注入（读错类型属「未安装该类型」，软 `None` 合理，与 `intercept_in_place` 写入侧「多次拦截同 key 须同类型 → panic」区分）。但这份「读路径不定向的 `None` vs 定向的 `panic`」的纪律未被文档显式区分，仅在 panic 文案里暗示。建议在 `get_meta` doc 补一句：携带侧类型不符返回 `None`（可逆读）、声明侧类型不符 panic（组件固定类型、bug）。

### nit3. `register` 对 `component` 的 `Rc` 双重克隆（`apply` 闭包 + `component` 字段）
**位置**：`crates/cordis-core/src/runtime.rs:202`（`apply` 闭包 `Rc::clone(&component)`）与 `:214`（`component: Rc::clone(&component)`）

**事实**：`Fiber` 新增 `component: Rc<dyn Component>` 字段后，同一 `Rc<dyn Component>` 在构造 fiber 时被克隆两次——一次进 `apply` 闭包（组件效应函数 `component.apply`），一次进 `component` 字段（声明元数据读取）。二者各自合法、无循环（`Component` 不持有 `Fiber`，见「正面确认」），仅多一个引用计数与一份指针的冗余。

**影响（非 block/major）**：纯结构性微小冗余，无正确性或内存缺陷。若在意，可让 `apply` 闭包改为捕获 `Rc<dyn Weak>` 或复用 `component` 字段（但会引入借用顺序复杂性，得不偿失）。记录供知情，不建议改动。

### nit4. `get_meta` / `intercept_in_place` 的 `self: &Rc<Self>` 而非 `&self`
**位置**：`crates/cordis-core/src/context.rs:343`、`:377`

**事实**：`get_meta`（`declared_metadata_of` 仅 `&self`、`intercept_of` 仅 `&self`）与 `intercept_in_place`（仅 `self.intercept.borrow_mut()`）都**不需要** `Rc`（均不构造新 `Context`、不 `Rc::clone` 自我），却采用了 `&Rc<Self>` 接收者；对比真正需要 `Rc` 的是派生实现 `intercept`（`:287`，构造 `Rc::new(Context)`）。

**影响（非 block/major）**：一致性考虑（所有 `InterceptMeta` 相关 API 统一 `&Rc<Self>`）可接受，且为将来「就地拦截后需派生/克隆」留了余地。纯 API 风格小事，非缺陷，仅记录。

---

## 正面确认（实现正确的点）

### Def 31 右偏合并方向的忠实性（核心结论）

- **`get_meta` 的合并方向正确、与论文一致**：`get_meta`（`context.rs:346-350`）在 `(Some(mu), Some(iota))` 时调 `M::merge(&mu, &iota)`——按 `InterceptMeta` 契约（`context.rs:39-40`「`merge(existing, new)` 中 `new` 优先」），`existing=μ`、`new=ι`，**ι 优先级高于 μ**，恰是 Def 31「`get(k, μ) = σ(k)(μ ⊕ₖ ι(k))` 中 `ι(k)` 覆盖组件 `μ`」的落地。与论文「ι(k) takes priority」逐字一致。
- **与 `intercept` 操作式 new 优先自洽**：派生 `intercept`（`:289-297`）用 `M::merge(existing, &meta)`（`meta`=新拦截=`ν`，`new` 优先），`intercept_in_place`（`:379-386`）同样 `M::merge(existing, &meta)`。三处经同一 `merge(existing, new)` 契约统一为「后到者优先」，即 M0 已记录的「实现采纳 intercept 操作侧语义（new 优先）」，get 侧放 `ι` 到 `new` 位、组件 `μ` 到 `existing` 位——get 与 intercept 两侧的「右偏张力」在实现层面被**同一定向消解**，与 THEORY-MAP L108 的张力记录相符。
- **常量 provider 函数的公开差异记录准确**：`get` 签名（`context.rs:125` `pub fn get<K: Key>(&self)`）**无 `μ` 参数**、不咨询 `ι`，`get_meta` 仅暴露**合并后的元数据**而非经 `σ(k)` 求值的绑定值——doc（`:338-342`）与 THEORY-MAP L108「`get` 签名仍无 μ 参数（常量 provider 函数）」、处置清单①「provider 函数形态（σ(k): ℳ→𝒱）随⑨ typed world 评估」三处口径一致，公开差异声明准确无误导。

### 借用纪律与回退正确性

- **`declared_metadata_of` 的借用纪律正确**（`context.rs:354-370`）：先 `let fid = self.fiber?` 取 fiber id（`Option` 早退），再 **块作用域** `{ let fibers = self.runtime.fibers.borrow(); ... Rc::clone(fiber.component()) }` 在 `Ref` 释放前克隆出 `Rc<dyn Component>`，随后（作用域外）调用 `component.declared_metadata(key)`——`fibers` 的 `RefCell` 借用已在块末 drop，不跨过 `declared_metadata` 调用（后者返回 `Box<dyn InterceptMeta>`，无 `fibers` 借用）。规避了本仓库一贯的 `RefCell` 双重借用陷阱（对照 `fiber.rs:158-163` 的 m4 借用警告纪律）。
- **root ctx（fiber=None）回退正确**：`declared_metadata_of` 首行 `let fid = self.fiber?;` → root/编排器上下文 `declared=None`，`get_meta` 落入 `(None, iota) => iota` 分支，仅返回 `ι`——与「root 无组件、无声明」语义一致。`derived_intercept_is_isolated_in_place_is_shared` 测试正是用 `Context::new()`（fiber=None）验证了 `intercept_of`/`intercept_in_place` 在 root 下的行为。
- **`intercept_in_place` 与派生 `intercept` 的类型冲突纪律一致**：二者在「同 key 已有值且类型不符」时均 `downcast_ref::<M>()` + `expect("拦截元数据类型冲突…")` panic，文案近似、策略统一（panic = bug，与 `InterceptMeta` 模块文档一致）。

### Fiber 构造与内存

- **`component` 字段无环**：`Fiber → component: Rc<dyn Component>`，而 `Component` trait 对象（含 `WasmComponent`/宏派生/测试组件）不持有 `Fiber`（`register` 只把 `Rc<Fiber>` 存进 `runtime.fibers` 与父上下文注册回调闭包，不回流给 component）——单向引用，**无 `Rc` 循环**，退役/移除后 fiber 经既有幽灵 fiber 语义（`runtime.rs:238-243` m2 记录）正常释放。
- **构造点唯一且正确**：`component: Rc::clone(&component)` 落在 `register`（`runtime.rs:214`），与 `inject`/`provide`/`apply` 的既有取值同源同质；`Fiber::component()`（`fiber.rs:171-173`）返回 `&Rc<dyn Component>` 只读访问。`WasmComponent`（`cordis-wasm/src/lib.rs:269`）与宏派生（`cordis-macro/src/lib.rs:104`）**未实现** `declared_metadata` → 走 trait 默认 `None`（ε），新增默认方法对既有 4 处 Component 实现零破坏（clippy/test 全绿佐证）。

### 测试强度（5 测试）

- **右偏合并主测**（`get_meta_merges_declared_with_carried_right_biased`）：声明 `read_only:false`/`"/a"`、携带 `read_only:true`/`"/b"` → 断言 `read_only:true`（ι 覆写 μ）+ `paths={"/a","/b"}`（取并）。**非假阳性**——`PathMeta::merge` 恒取 `new.read_only`，若实现误写成 `M::merge(&iota, &mu)`（颠倒 new/existing），`new`=μ 将得 `read_only:false`，断言 `true` 会失败，故该测试**真切区分合并方向**。
- **回退语义**（`get_meta_falls_back_to_carried_or_none`）：`None`+`None`→`None`、`None`+`Some(ι)`→`ι` 两分支覆盖；`Some(μ)`+`None`→`μ` 分支由主测首段覆盖。
- **就地不 reload**（`intercept_in_place_updates_without_reload`）：以 `matches!(state, Active)` 断言 fiber 状态不变 + `runtime.is_quiet()`（Def 49 式，含 PR #17 的 ζ 析取）断言无转换在途，**实质行使**「就地更新不触发 reload」这一 §5.2.1 核心语义。
- **派生隔离 vs 就地共享**（`derived_intercept_is_isolated_in_place_is_shared`）：直接对照派生 `intercept` 不触原 ctx（`ctx.intercept_of == None`）、就地 `intercept_in_place` 触原 ctx——精确锁定两 API 的语义差异。
- **类型冲突 panic**（`declared_metadata_type_conflict_panics`）：`should_panic(expected="拦截元数据类型冲突")` 验证组件声明读取类型不符 → panic，覆盖了 `declared_metadata_of` 的 `downcast_ref` 失败路径（是对普通 `intercept` 类型冲突测试的**声明侧补充**，此前无此路径覆盖）。

---

## 总结

- **blocker**：无。
- **major**：无。
- **nit**：nit1（`dom(d) ⊆ inject` 仅文档声明、未强制、未记偏差）、nit2（`get_meta` 声明侧 panic vs 携带侧 None 的读路径类型纪律不对称未文档化）、nit3（`register` 对 `component` 的 Rc 双重克隆）、nit4（`get_meta`/`intercept_in_place` 冗余 `&Rc<Self>`）。

**结论：通过。** 置信度：高——逐行审读全 268 行测试与 `context.rs`/`component.rs`/`fiber.rs`/`runtime.rs` 语义对照，实跑 5 测试全绿、`cargo test --workspace` 全绿（0 失败，含 wasm 后端）、fmt/clippy 干净。核心正确性（Def 31 右偏合并方向、与 intercept 操作侧 new 优先的自洽、借用纪律、root 回退、Fiber 无环构造、公开差异记录准确性）均确认无误；5 测试覆盖合并方向/回退/就地不 reload/派生隔离/类型冲突且非假阳性；`Component` trait 新增默认方法对 WasmComponent、宏派生、native、loader 四处实现零破坏。4 项 nit 为文档精确性（nit1/nit2）与结构性微冗余（nit3/nit4），均不阻塞合入，随后续 PR 顺手修正即可。
