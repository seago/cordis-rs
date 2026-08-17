# 代码审查报告：commit `54814d0`（PR #34 / G8 就地改值 + G9 可用性谓词）

- **审查对象**：`54814d08937844c4aa51806a8052fe148e24267a`（core：`src/store.rs` +45、`src/context.rs` +93、`src/runtime.rs` +14、`tests/check_in_place.rs` 新 +202）+ docs 提交 `72b0201cd3ab35cad285bfaeebb0d5553b816329`（`docs/THEORY-MAP.md` +1、`docs/TS-REFERENCE-GAP.md` +2/−1）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show 54814d0` / `git show 72b0201` 逐行核对 diff；读 `src/store.rs`（`replace_value`/`bind_value_checked`/`Binding`）、`src/context.rs`（`set`/`set_in_place`/`set_with_check`/`get`/`resolve`）、`src/runtime.rs`（`provider_of`/`satisfied`/`compute_target`/`is_quiet`/`refresh`）。**源码级对照 TS 参照**（`/tmp/cordis-ts`）：`packages/core/src/reflect.ts`（`set` 变异 `impl.value`、`provide(name,value,check)`、`_getImpl`、notify 中 `internal/service`）、`packages/core/src/fiber.ts`（`_checkImpl` 在 `_refresh` 内调用、`check` 假时删 `_store[name]`）。**实跑**：`cargo test -p cordis-core --test check_in_place`（**3 passed / 0 failed**）；`cargo fmt --check`（exit 0）；`cargo clippy -p cordis-core --tests --no-deps`（exit 0）。`Cargo.toml` 无变更、`std::rc::Rc` 为标准库——**零新增第三方依赖**。**未跑 wasm/全 workspace**（按审查范围限定）。

---

## 结论：**通过**

PR #34 的 G8/G9 是一组**诚实、自洽、与 TS 参照同型**的落地：`set_in_place` 精确复刻 TS `ReflectService.set` 的"变异当前值、不追踪、无逆"语义（与论文 "overwriting its own binding in place is therefore not observed" 一致）；`set_with_check` 在 `Binding` 上携带 `check` 谓词、由 `provider_of` 每次求值（假 → 依赖者解析不到 → 目标 None → Inactive），与 TS `_checkImpl` 语义方向对应。逐条核实的 6 个审查点中，**关键论断全部准确**（见下）：`replace_value` 只改 `value` 字段、`provider`/`check` 保留；非安装者拒绝 = `AlreadyBound`（TS "cannot set in multiple fibers" 同型）；撤销语义与 `set` 一致（unbind + notify）；check 门控依赖者 false→Inactive / true→恢复直证。`is_quiet` + check 无死锁（Inactive(None) + target None 静止判定一致）。未发现语义错误、断言失真或文档矛盾。发现若干 **nit**（测试覆盖缺口、`get`/`resolve` 对 check 盲读的口径说明、谓词在 store 借内求值的重入脚枪、"即时生效"措辞），均不阻塞合入。

> **核心设计评估**：G8 正确选择"变异 + 无逆"而非"撤销绑定再重绑"——这样既保留原绑定逆（fiber teardown 时照常撤销，丢弃被替换值），又规避了"in-place 改值对依赖者引入重解析"的反应式连锁；`Result<(), StoreError>` 返回值形态与"本操作是纯命令式、无生命周期可供追踪"自洽。G9 把谓词放进 `provider_of`（核心可达路径）而不是建模成一次 notify 源，换取"惰性、无副作用、每次评估见新值"，是符合 Rust 单线程 Rc 宿主模型的最小实现。

---

## 逐条核实

### §1 G8 `set_in_place`（前置 / 不被观察语义 / 类型检查 / 越界纪律 / 返回形态）——**通过**

- **前置**（context.rs:295-321）：`resolve_realm` → `binding(realm)` 存在（否则 `NotBound`）→ `binding.provider != self.fiber` 时 `AlreadyBound`（TS `set` 的 `impl.fiber !== this.ctx.fiber → "cannot set property in multiple fibers"` 同型）。类型不符由 `replace_value::<K>` 的 `downcast_ref` 报 `TypeMismatch`。越界供给纪律（Def 43/48）与 `set` 完全一致（`fiber.provide.contains(key)` 否则 panic）。
- **不被观察语义**：`set_in_place` 全路径**不 `notify`、不建逆**（不经 `effect`/累加器、无 disposer）。对照论文 "overwriting its own binding in place is therefore not observed"——in-place 改写本 fiber 自己的绑定，不对依赖者触发重解析（反应式上"不被观察"）。对照 TS `ReflectService.set`：`impl.value = value` 后**无派发、无追踪**（reflect.ts:171），同型为零。`Result<(), StoreError>` 返回形态与"无逆、无生命周期"自洽（`set`/`set_dyn` 返回 `Disposer` 是因为它们可逆；G8 不可逆，返回值形态如实）。**一致**。
- **`replace_value` 保留 provider/check（审查重点——✓ 确认）**：store.rs:132-146 仅 `binding.value = Box::new(value)`，`provider` 与 `check` 字段**不动**。`set_in_place` 后绑定安装者不变、谓词保留。
- **nit §9.1**：错误载荷不一致——`set_in_place` 非安装者拒绝报 `AlreadyBound(realm)`（realm 语义，context.rs:314），而 `set`/`set_with_check` 前置拒绝报 `AlreadyBound(key)`（用户键，252/345）。隔离场景下两者符号可不同；`set` 对该 split 有已知偏差记录（THEORY-MAP），`set_in_place` 未声明。属口径统一建议，非缺陷。

### §2 G9 `set_with_check` / `provider_of`（谓词求值 / `is_quiet` 一致性）——**通过**

- `Binding.check: Option<Rc<dyn Fn() -> bool>>`（store.rs:63），`bind_value_checked` 为 `bind_value` 超集。`set_with_check` 经 `effect` 绑定（可逆 + notify，同 `set`），谓词封装为 `Rc<dyn Fn() -> bool>`。
- `provider_of`（runtime.rs:538-550）：`binding(realm)` 存在 → `check.as_ref().is_some_and(|check| !check())` 为真即返回 `None`（视为未提供）→ 进而 `satisfied` 假 → `compute_target` 为 `None` → 依赖者目标撤销 → Inactive。**与 TS `_checkImpl` 语义方向对应**（TS 在 fiber `_refresh` 内对每个 name 调 `_checkImpl`，check 假时删 `_store[name]`，依赖者重新解析不到——两者都把"谓词假 → 不可用 → 依赖者退化"落到解析路径）。
- **`is_quiet` + check 一致性（无死锁）**：check 假 → 依赖者 Inactive(None)、`target = None` → `is_quiet` 分支 `Inactive(None) => target.is_none()` 成立 → 静止一致；check 真 → 依赖者 Active{view}、`target == view` → 静止一致。`compute_target` 每次调用都重算 `provider_of` → 谓词翻转后依赖者经 `refresh` 恢复。无死锁/不一致路径。测试 3 直证（false→Inactive、true→恢复）。
- **`provider_of` 的实现**正确地在调用 `is_active` 前 `drop(store)`（避免同时持 store 与 fibers 两个 borrow）。✓
- **nit §9.2（谓词重入/借冲突脚枪）**：`provider_of` 在**持有 `store.borrow()`** 期间调用 `check()`（runtime.rs:541 持借 → 544 求值）。文档要求"谓词须纯、无副作用"，但"纯函数"仍可经捕获的 `Context` 句柄调 `ctx.get`（`RefCell` 已借 → panic）或调 `set_in_place`（需 `borrow_mut` → panic）。建议文档把约束从"纯、无副作用"收紧为"**不得触碰本 store（含经 ctx 的读写）**"，或对 check 传入 store 借以阻止——属设计脚枪说明，非语义错误。
- **nit §9.3（"即时生效无需 notify"措辞）**：代码之真的语义是"每次 `provider_of`/`compute_target` 求值都看到新 check 值，**无需 notify 事件来推动谓词重算**"——但**谓词翻转本身不会触发任何 refresh**，依赖者须由其它触因（如测试 3 的手动 `runtime.refresh(&consumer)`）才重求值。测试 3 正因如此须手动 refresh。TS 侧同理（check 只在 `_refresh` 时经 `_checkImpl` 重估），并非 Rust 独有偏差；但"即时生效"字面含义易被误读为"翻 flag 即自动传播"，建议文档补"谓词变化需依赖者 refresh 才感知（无反向驱动的通知）"。

### §3 撤销语义（set_with_check disposer = unbind + notify）——**通过**

`set_with_check` 的逆（context.rs:358-365）为 `unbind_value(realm)` + `ctx.notify(&[realm])`，与 `set` 的逆同构（unbind + notify）；绑定撤销后绑定离开 `dom(σ)`，依赖者 `provider_of` 返回 `None` → 目标撤销 → `resolve` 走 committed-view 授权路径读不到 → Inactive。文档声明"撤销语义与 `Context::set` 相同（可逆、notify）"**如实**。✓

### §4 测试质量——**核心断言皆直证，边界场景有覆盖缺口（nit 级）**

3 个新增测试逐一对应：

| 声明 | 测试 | 直证手段 |
|---|---|---|
| 就地改值不 notify / 不追踪 | `set_in_place_replaces_value_without_notify` | 替换后 `db_value()=="v1"` + 消费者 `id` 不变 + `state()=Active`（未重激活）+ `is_quiet()`；同时直证非安装者 root 就地改值 `AlreadyBound` 且值未被篡改 |
| 未绑定 → NotBound | `set_in_place_unbound_errors` | retire+remove fiber（经原 set 逆 unbound）后 `set_in_place` → `NotBound` |
| check 门控依赖者 | `check_predicate_gates_dependents` | flag 翻转 + `runtime.refresh` → Active → Inactive → Active；`is_quiet()` 静止 |

- **nit §9.4（业务分支未直证）**：
  - `set_in_place` 后 `check` 保留（§1 的核实推论）**无测试**——测试 1 用的是无谓词绑定，未验证 replace_value 在 checked 绑定上的字段保持。
  - `set_with_check` disposer 撤销后依赖者恢复（§3）**无测试**——测试 3 全程不调用 `_check_disposer()`（从未撤销 check 绑定），仅用 flag 翻转。
  - 谓词求值在 store 持借内的 reentrancy 禁止、以及 `set_in_place` 对已装箱动态绑定（`set_dyn` 装的值）的 `K::Value` 类型判别路径均未覆盖。
  - 测试 2 结构略绕（先建一个 `runtime`/`root`/`provider` 后 `let _ =` 弃置、再另建 `runtime2` 走 retire 清理推断 `NotBound`）——可用更直接的"全新空 runtime 直接 `set_in_place` 即 NotBound"替代，直证性更高。

### §5 docs 一致性——**一致，一处措辞口径待补（nit 级）**

- **TS-REFERENCE-GAP.md** G8/G9 条目标记 ✅ 均已落地，措辞与代码一致（`set_in_place` 不 notify/不追踪、`set_with_check` 谓词由 `provider_of` 求值、谓词假 → 依赖者 Inactive）。
- **THEORY-MAP.md** PR #34 行与代码一致（引 Def 23/29/45、"multiple fibers" 同型、"overwritten in place is therefore not observed"、测试 `check_in_place.rs` 3 项）。
- **nit §9.5（check 只影响依赖解析、`get`/`resolve` 盲读未注明）**：文档只声明"谓词假 → 消费者**视为未提供**（`provider_of` 每次求值）"，未显式说明 `Context::get`（裸 store 读，context.rs:157-165）与 `Context::resolve`（commit-view 授权 + 裸 store 读，178-216）**对 check 盲读**——即对 checked 绑定直接 `get`/`resolve` 仍返回旧值（即使谓词为假）。对照 TS：`_checkImpl` 假时删 `_store[name]`，使 `ctx.get(name)`（reflect.ts `get` 走 `fiber.store?.[name]`）返回 undefined——**TS 直读路径 respect check，而 Rust 侧不**。这个分歧是 `get`（裸读 vs 依赖解析）设计分工下可辩护的（Rust 的 `get` 本就是"永不失败的原语读"、`resolve` 是承诺视图授权），但作为与 TS 的**公开语义差异**应当在 TS-REFERENCE-GAP 补一句（"check 只影响依赖解析（σγ/`provider_of`），`get`/`resolve` 直读不经 check"）。

### §6 纪律（fmt / clippy / 零第三方依赖）——**通过**

`cargo fmt --check`（exit 0）、`cargo clippy -p cordis-core --tests --no-deps`（exit 0）。`Cargo.toml` 本 PR 无变更；G9 用 `std::rc::Rc`、`std::cell::Cell` 均为标准库。**零新增第三方依赖**。✓

---

## 逐条核实的完整性核对（本次未逐一展开但已覆盖）

- `set_in_place` 用 `binding.provider != self.fiber`（`Option` 判等）做安装者甄别：组件 fiber ctx（`Some(id)`）与 root ctx（`None`）判定正确；root 绑定（provider=None）+ root ctx 调用也可放行（`None != None` 假）——语义自洽。
- `set_with_check` 的 `AlreadyBound` 前置在 `contains(realm)` 检查（344），与 `set`/`set_dyn` 同构；绑定不通过裸 `bind` 而是 `bind_value_checked`（`Some(check)`），`value` 已装箱。
- 谓词为 `'static` 闭包（`Rc<dyn Fn() -> bool>`），被 `provider_of` 每次求值——`compute_target`/`is_quiet` 对全 fiber 集合都会触发求值，性能由调用方（谓词内耗）自担，文档已注明"谓词须纯"。

## 严重度汇总

- **major**：0
- **nit**：5（§9.1 错误载荷口径 / §9.2 谓词重入借冲突脚枪 / §9.3 "即时生效"措辞 / §9.4 测试覆盖缺口 / §9.5 `get`/`resolve` 对 check 盲读的公开差异未注明）

**总评**：G8 与 G9 语义准确、自洽、与 TS 参照及论文说法一致，测试直证核心断言，fmt/clippy/依赖纪律干净，docs 与代码一致。无阻塞性问题；5 处 nit 均为边界口径与测试/文档覆盖建议，可在后续 PR 完善。**可以合入。**
