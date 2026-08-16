# 代码审查报告：commit `e8bd96e`（PR #22 M2-PR6——Alg 6 Proxy 访问层 + 处置⑥⑨ 评估收尾）

- **审查对象**：`e8bd96e1f1e580065be05a89e94e5230b08ffd88`（`crates/cordis-core/src/context.rs` +56/−0、`crates/cordis-core/src/lib.rs` +1/−1、`crates/cordis-core/tests/access.rs` +149）及配套 docs 提交 `a929a3ea4676311c73d12f352e3fb42e2b1b6690`（`docs/PLAN.md` +1/−1、`docs/THEORY-MAP.md` +4/−3）、`c4f0bb7d05fbe4a96da88bb5608a449e1f09f940`（`docs/PLAN.md` +1/−1、`docs/THEORY-MAP.md` +37/−0）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show e8bd96e` / `git show a929a3e` / `git show c4f0bb7` 逐行核对 diff；读 `context.rs`（`resolve`/`resolve_realm`/`get`/`isolate_in_place`）、`fiber.rs`（`View`/`committed`/`parent`/`inject`/`ctx`）、`store.rs`（`get`/`StoreError`）、`runtime.rs`（`resolve_view`/`compute_target`/`provider_of`/`satisfied`）、`loader/src/lib.rs`（`patch_isolation`/`realm_of`/`collect_subtree_ids`）；从 `paper/paper.pdf` 提取 §5.1.4 Algorithm 6 原文逐行对照；对照 THEORY-MAP M0 走查 Alg 6 行 / PR #22 行 / 处置②⑥⑨ 行、PLAN M2 进度行与 §7 走查程序。实跑 `cargo test --workspace`（**全绿，0 失败**，30 个 test result 行含 doc-tests）、`cargo fmt --all -- --check`（exit 0）、`cargo clippy --workspace --all-targets`（exit 0，0 警告）。

---

## 结论：**有条件通过**

Algorithm 6 的判定顺序（committed 先于 inject、inject 先于到父）、INACTIVE_ACCESS（声明未提交）/UNDECLARED_ACCESS（root 无声明）语义、经 `self`（调用方持有的 ctx）而非本地 `fiber` Rc 借用 store 的 `Ref` 逃逸规避、读视图（committed）而非裸 store 的 Thm 63 语义，均与论文逐行吻合，实现整体正确。存在 1 项 major（覆盖/标注口径：Algorithm 6 的"链上行"核心路径未被任何测试行使，测试④ 注释误导）与 3 项 nit（realm 漂移边界未记录、`AccessError` 缺 `Display`/`Error` 实现、`TypeMismatch → Inactive` 折损）。详见下文。

---

## 🔴 需决策（major）

### major1. "链上行"（`fiber.parent` 遍历）是 Algorithm 6 的核心路径，但 4 个测试均止于首个 fiber 的 committed 视图，遍历循环零行使 + 测试④ 注释与实际路径不符

**位置**：`crates/cordis-core/src/context.rs:199`（`fid = fiber.parent`）、`crates/cordis-core/tests/access.rs:127-149`（`resolve_climbs_fiber_chain`，尤其 `:146` 注释）

**事实**：`resolve` 的 do-while 循环中 `fid = fiber.parent`（`:199`）是 Algorithm 6 第 7 行 `fiber ← fiber.parent.fiber` 的直译，是"沿 fiber 链向上"语义的唯一承担者。但现有 4 个测试无一真正触发它：

- 测试① `resolve_authorizes_via_committed_view`（Consumer 注入 db、Provider 提供 db）与测试④ `resolve_climbs_fiber_chain`（父 DbProvider 提供 db、子 Consumer 注入 db）**都在访问 fiber 自身的 committed 视图处短路**——因为 `committed = resolve_view(fiber)` 只包含 `fiber.inject` 的键（`runtime.rs:506-513`），Consumer 注入 db ⟹ 其 committed 视图本就绑定 `db → 提供者`，循环第一次迭代即命中 `committed.contains_key(&key)`（`:184`）返回，**从不走到 `fid = fiber.parent`**。
- 测试④ 的注释 `:146`"子经 resolve 沿链（自身 committed → 父 committed）读到 db"**与实际执行路径不符**：实际是"子在**自身** committed 视图（`db → 父提供者`）直接授权"，父 fiber（DbProvider）的 committed 视图为空（注入为空），从未被 consult；`parent` 字段全程未被遍历。

**纸面语义核对**（Algorithm 6 原文）："Algorithm 6 walks the fiber chain upward from the accessing context: at the first fiber whose committed view binds key, the access is authorized"。要真正行使"向上走"（`fiber ← fiber.parent.fiber`），需要**访问 fiber 自身不声明 key**（既不在 committed 也不在 inject），仅其**祖先**声明——典型场景是组件在 `apply` 内对未写入 `inject()` 的键调用 `ctx.resolve::<K>()`（Proxy 的"读父作用域共效应"用途），使其落入 `:196`（inject 不含）→ `:199`（爬父）→ 父（或更上）的 committed 命中。现有测试缺这一形态，导致：

1. `fiber.parent` 取父、跨多级祖先（祖父链）、以及"父 committed 授权"分支——**全部未经验证**；
2. 纸面宣称的"链上行"（THEORY-MAP PR #22 行与处置② 均写"沿 fiber 链向上：committed 授权 / … / 链上行父子"）与测试实际覆盖的语义存在落差。

**影响（major，非 blocker）**：循环逻辑经代码审视**判无 bug**（`fiber.parent` 类型正确、`None → Undeclared`、父子链构造在 `register` 时 `parent: ctx.fiber` 已建立）；但 Algorithm 6 的**招牌语义（向上遍历）恰是唯一未经行使的分支**，且测试④ 的命名与注释误导读者以为已覆盖。这与仓库"测试必须可区分语义分支、不得假阳性/名不副实"的纪律（M0 走查已多处强调）相悖。**建议**补一个用例：一个 `inject = ∅` 的组件（或 direct 构造 `Context::resolve` 调用点）挂在"声明并已加载该键"的祖先之下，断言 `resolve` 命中**祖先**的 committed（而非自身），使 `fid = fiber.parent` 至少行使一次；同时修正测试④ 的注释（"自身 committed"而非"沿链到父 committed"），避免继续误导。

---

## ⚪ 细节（nit）

### nit1. realm 漂移（committed 视图只存 key→provider，值经授权 fiber 的**当前** ρ 重解析）未记录为边界（审查要点 1 点名）

**位置**：`crates/cordis-core/src/context.rs:181-194`（`fiber.committed` 判授权后经 `fiber.ctx.resolve_realm(key)` 读 store）

**事实**：`committed` 视图类型 `View = BTreeMap<Symbol, FiberId>`（`fiber.rs:29`）只记录 `key → 提供者`，**不记录 key → realm**。授权后读值走 `fiber.ctx.resolve_realm(key)`（`:189`），用的是授权 fiber 的 ctx 的**当前** ρ。而承诺视图在激活时经 `resolve_view` → `provider_of` → `ctx.resolve_realm(key)`（`runtime.rs:459-466`）以**当时的** ρ 解析。若之后 `isolate_in_place`（Algorithm 7，`loader/src/lib.rs:656/659` 对子树 fiber ctx 就地改 ρ）改动了该 fiber 的 ρ 但**未移动其提供者绑定**（`own=false`，`:681-687` 跳过迁移——典型为"消费者 fiber 在子树内、但其 db 提供者在子树外"），则该 consumer fiber 的 committed 视图仍记录 `db → 外部提供者`，而 `resolve_realm("db")` 已指向新 realm 且那里无绑定 → `store.get` 失败 → 错误折成 `Inactive`。即在 isolate 重指派后的瞬态窗口内，"承诺视图承诺激活时解析"与"访问时重解析 ρ"之间可能漂移。

**paper 对照**：Algorithm 6 第 4 行 `return fiber.committed[key]` 语义上返回承诺视图在**提交时**绑定的值，实现却以访问时 ρ 重读——忠实性存在一个未声明的机制差异。

**影响（非 block/major）**：在 loader 的 `patch_isolation` 会同步 `refresh` 子树（`:664-669`），quiescent 态下 consumer 要么按新 realm 重激活（committed 重建）要么卸载，漂移只在同步核心的重指派瞬态窗口理论可见、不构成可复现 functional bug。但**这是"读视图而非 store"承诺下唯一由"机制差异"而非"实现缺陷"承载的语义边界**，且审查要点 1 明确点名，仓库惯例（THEORY-MAP"公开差异声明/适应记录"栏）要求如实记录。建议在 THEORY-MAP 补一条适应记录：committed 视图存 key→provider（不含 realm），resolve 以访问时 ρ 重解析——与 Algorithm 6"返回提交时绑定"的机制差异，quiescent 态可观察等价、isolate 重指派瞬态窗口存在漂移边界。

### nit2. `AccessError` 未实现 `Display` / `std::error::Error`，与同级公开错误类型不一致

**位置**：`crates/cordis-core/src/context.rs:38-44`

**事实**：`StoreError`（`store.rs:36`）与 `FiberError`（`fiber.rs:59-65`）均实现 `Display` + `std::error::Error`；新导出的 `AccessError` 只有 `#[derive(Debug, Clone, PartialEq, Eq)]`，无 `Display`/`Error` 实现。调用方拿到 `Result<Ref<K::Value>, AccessError>` 无法用 `?` 与其它错误类型贯通（`Box<dyn Error>` / `anyhow` 场景须额外适配），且与 crate 既有错误类型约定不一致。

**影响（非 block/major）**：纯 API 卫生/一致性，不涉及语义。

### nit3. `store.get::<K>(realm).map_err(|_| AccessError::Inactive)` 把所有 `StoreError`（含 `TypeMismatch`）折为 `Inactive`，类型不匹配的语义被吞

**位置**：`crates/cordis-core/src/context.rs:191`

**事实**：`:191` 把 `StoreError::NotBound` 与 `StoreError::TypeMismatch` 一律映射为 `AccessError::Inactive`。其中 `NotBound → Inactive` 对应"teardown 窗口 / 绑定尚未就绪"是合理的（Thm 63 语义的防御性兜底）；但 `TypeMismatch`（`store.rs:162-165`，同一 realm 绑定在另一值类型下）是**编程错误（bug）**，折为 `Inactive` 会把它伪装成"组件未加载"，与"panic = bug 策略"（如 `set` 的越界写 panic、`intercept`/`get_meta` 的类型冲突 panic）相悖。审查要点 2 的"teardown 窗口（store 读失败 → Inactive 映射）"只点名了 NotBound→Inactive，未评估 TypeMismatch 分支。

**影响（非 block/major）**：防御性折中的合理性可接受（resolve 是宽松的 proxy 查找，把类型不符当作"该 realm 无可读绑定"并非全然失准），但至少应加一行注释说明 TypeMismatch 也随 NotBound 折 Inactive 是有意为之，避免与仓库"类型不匹配 panic"的既有约定表面矛盾。

---

## 正面确认（实现正确的点）

### Algorithm 6 判定顺序与错误语义忠实（审查要点 1 核心）

- **顺序正确**：committed 检查（`:182-195`）先于 inject 检查（`:196-198`），与论文第 4 行（`key ∈ fiber.committed`）先于第 5 行（`key ∈ fiber.inject`）一致；inject 检查后才爬父（`:199`），与第 6-7 行"inject 判定后才 `fiber ← parent`"一致。
- **错误语义正确**：声明未提交（committed 不含 key 且 inject 含 key）→ `Inactive`（对应 `INACTIVE_ACCESS`）；至 `fid = None`（root）→ `Undeclared`（对应 `UNDECLARED_ACCESS`）。root 上下文 `fiber=None` 无 inject，paper 第 6 行 root 检查在实现中于循环顶部 `:171-173` 以 `fid=None → Undeclared` 等价承接（root 无声明 ⟹ 两处顺序等价）。
- **committed 判据正确**：`committed`（`Option<View>`）只在 Active/Reloading 时 `Some(view)`、Inactive 时 `None`（`runtime.rs:384/448-449`），且 `view` 仅含 inject 键（`resolve_view` 遍历 `fiber.inject`），故 `committed.contains_key(key)` 精确等价"该 fiber 已声明且已提交（加载）key"——与论文"committed view binds key"吻合。

### Ref 借用路径正确（审查要点 2）

- **经 self 借用、无本地 fiber 逃逸**：`let store = self.runtime.store.borrow()`（`:190`）经 `self`（`&Rc<Self>`）借用共享 store；`Ref::map(store, …)`（`:192-194`）把借用导入返回 `Ref<'_, K::Value>`，其生命周期绑定于 `self`（调用方持有的 ctx）而非局部变量 `fiber`（`:174-180` 的临时 `Rc<Fiber>`）。`fiber` 在函数返回时被 drop，但返回的 `Ref` 不借用之——规避了"本地 fiber 引用逃逸"的悬垂。注释 `:186-188` 准确。
- **双读模式与 `Context::get` 一致**（`:146-154`）：先经 `store.get`（`:191`）做存在性 + 类型检查，再 `Ref::map` 内层 `expect("checked above")`（`:193`），与既有 `get` 的"先错误路径检查再借用守卫映射"同构，无 TOCTOU（同步核心、单线程、`fiber.committed` 在授权判定后未被 yield）。
- **teardown 窗口映射合理**：`map_err(|_| Inactive)`（`:191`）为 Thm 63 顺序承诺提供防御性兜底——依赖者 teardown 期提供者绑定先于自身 dispose（`runtime.rs:431-443` 依赖者先撤、绑定保持可读），正常路径应命中；`NotBound → Inactive` 覆盖竞态/未就绪的罕见窗口（TypeMismatch 分支见 nit3）。

### 测试强度评估（审查要点 3）

- 4 测试均**非假阳性**：`resolve_authorizes_via_committed_view` 断言 `Active` 态 + `resolve` 返回值精确 `"pg"`，可区分"授权"与"返回错误"；`resolve_raises_inactive_access` 先断言 `Inactive(_)` 态再断 `Inactive` 错误，因果链完整；`resolve_raises_undeclared_access` 以 root 上下文直读未声明键；测试④ 断言子 `Active` + 返回 `"pg"`。无"裸断言恒真"的通病。
- **覆盖缺口**（承 major1）：多级祖先链、teardown 中可读性的 **resolve 直证**（Thm 63 已有 `runtime.rs` 的 `withdrawal_cascade_disposes_dependents_first` / `consumer_asserting_readable_teardown` 经 `get` 覆盖，但 `resolve` 在 teardown 窗口的可读性无直证）缺席；`resolve` 的 `fiber.parent` 遍历零行使。

### 处置⑥⑨ 评估与走查 §5.2 门禁（审查要点 4）

- **处置⑥（命令式 Disposer 保留）评估如实**：THEORY-MAP 处置⑥行（`:201` 附近）把结论落为"保留命令式 `Box<dyn FnOnce>`——语义由 Thm 7/16/20/21 命令式测试保证，wasm 边界逆句柄化已对齐，纯函数 `g: Γ→Γ` 仅在形式化侧有价值、代码无可表达结构——关闭为记录"。与 M0 走查"Disposer 为命令式载体、变换幺半群无可表达结构、独立性退化为一阶约定"的公开差异声明（THEORY-MAP `:109` 附近 Disposer 行）一致；关闭口径"代码无可表达结构"未夸大。
- **处置⑨（typed world 评估）如实**：处置⑨行落为"保持通用 context + 符号级动态解析——§6.4 运行时动态中介正是论文要求（dependency access must be dynamically mediated）；结构检查由宿主注入键核对承担（load_guest 断言）；typed world 列为 DX 增强（M3 或按需）"。与 M1 走查 §6.4"空间维度 = … 运行时符号级动态中介"（THEORY-MAP M1 走查行）及 PLAN §4.3 `ctx.inject::<K>()` 编译期类型安全设想一致，关闭为"DX 增强"未越过论文要求。
- **走查 §5.2 门禁成立**：c4f0bb7 的"M2 走查记录"严格遵循 PLAN §7 程序（重读 §5.2 → 逐条核对已知偏差 → 补查映射 → 走查记录 + 处置清单）；§5.2.1（Def 74 全字段 / per-field dispatch / 配置树 / 托管 realm / Alg 7 适应）与 §5.2.2（Alg 8/9/10 三阶段 + 事务回滚 + 模块图边界）逐段有"论文段落 / 实现证据 / 对照"三列，无未解释偏差；处置清单去向明确（⑦⑩⑪⑫ 归 M3、⑩双向写回 ⑪组条目 isolate ⑫模块图适配器），与 PLAN M2 门禁列"4/4 全部达成"及 M0/M1 走查的处置承接链条闭环。

### 回归与卫生（审查要点 5/6）

- `cargo test --workspace` **30 个 test result 行全绿、0 失败**（含 `tests/access.rs` 4、cordis-wasm 全部、wasm guest 构建、doc-tests）；`cargo fmt --all -- --check` exit 0；`cargo clippy --workspace --all-targets` exit 0、**0 警告**。
- 命名一致（`resolve` / `AccessError::{Inactive,Undeclared}` 与论文 `INACTIVE_ACCESS`/`UNDECLARED_ACCESS` 对应；`AccessError` 经 `lib.rs:34` 导出）。
- 文档一致性：THEORY-MAP PR #22 行、处置②⑥⑨ 行、M2 走查记录、PLAN M2 行四处回填与实现一致（测试④ 注释的误导另见 major1）；无 THEORY-MAP 与 PLAN 之间的自相矛盾。

---

## 总结

- **blocker**：无。
- **major**：major1（Algorithm 6"链上行"`fiber.parent` 遍历零行使 + 测试④ `:146` 注释失真——建议补一个"访问 fiber 自身未声明、祖先声明"的 climb 用例并修正注释；循环逻辑审视判无 bug）。
- **nit**：nit1（realm 漂移边界未记录为公开差异/适应记录）、nit2（`AccessError` 缺 `Display`/`Error` 实现）、nit3（`TypeMismatch → Inactive` 折损未注释说明）。

**结论：有条件通过。** 置信度：高——逐行细读 `resolve`（56 行新增）与 `resolve_view`/`compute_target`/`provider_of`/`committed` 语义、从 paper.pdf 提取 Algorithm 6 原文逐行对照、实跑 `cargo test --workspace` 30 结果行全绿 + fmt/clippy 双门禁干净。Algorithm 6 的判定顺序、错误语义、读视图非裸 store、Ref 经 self 借用、处置⑥⑨ 评估、M2 走查 §5.2 门禁均确认无误；唯一实质风险是 major1 的"向上遍历"分支未经验证（机制经审视正确、应由测试行使），配合 nit1 的 realm 漂移边界记录，即可放行。
