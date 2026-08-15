# 代码审查报告：commit `742375e`（PR #4 共效应操作 + 通知分类）

- **审查对象**：`742375e6221964216c59e3f2e22fc27c5bda72ef`（相对 `6afd394`），6 个文件，+657/-171 行
- **审查日期**：2026-08-15（仓库时区）
- **核心代码**：`context.rs`（Runtime/Context 重构：`get`/`set`/`isolate`/`intercept`/`satisfies`/`notify`）、`notify.rs`（新增 Def 26 分类）、`store.rs`（realm 键控重构）
- **验证手段**：`cargo test -p cordis-core` **39/39 全绿**（新增 9：context 7 + notify 1 + store 1）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 零警告

---

## 🔴 必须修复（major）

### M1. `Context::notify` 的重入陷阱：反应器内 `on_notify` 直接 RefCell panic，反应器内 `set` 导致递归广播无防护
**位置**：`context.rs`（`notify` 实现）——`for reactor in self.runtime.reactors.borrow().iter() { reactor(self, keys); }`

**事实**：
- `borrow()` 返回的 `Ref` 在整个 `for` 循环期间存活；循环体内调用 `reactor(self, keys)`。
- 若反应器内调用 `runtime.on_notify(...)`（borrow_mut）→ **RefCell 双重借用 panic**（不可变借用期间请求可变借用），运行时必炸。
- 若反应器内调用 `ctx.set(...)`（或任何触发 notify 的共效应操作）→ 嵌套 `notify` 再次遍历全部反应器 → 同一事件被重复广播；反应器若对通知响应式地再次 set（如"收到激活后注册自己的效应"——这是 Algorithm 3 的典型反应式用法）→ **无限递归、栈溢出**。

**为什么是问题**：通知传播机制（Algorithm 3 骨架）是反应式系统的心脏，反应器在通知处理中执行共效应操作是论文场景的常态（refresh 驱动 fiber 状态变化）。当前实现既无借用释放（快照迭代）也无递归防护（工作队列/深度限制/事件去重），测试反应器恰好只记录事件，掩盖了该缺陷。

**建议**：① 先克隆反应器列表再迭代（`Rc` 克隆廉价）：`let reactors: Vec<Reactor> = self.runtime.reactors.borrow().iter().cloned().collect();`——消除 borrow_mut panic；② 递归语义需明示：或在文档中约定"反应器不得在通知处理中触发新通知"（并 `debug_assert`），或按 Algorithm 3 引入工作队列语义（PR #5 接入 fiber 反应器时一并设计）。至少补一个"反应器内 set"的重入测试来固化预期行为。

---

## 🟡 建议修复（minor）

### m1. `classify` 无系统内调用点——Def 26 分类与 `notify` 传播链路断裂
**位置**：`notify.rs`（`classify(prev, next, spec)`）vs `Context::notify(keys: &[Symbol])`

**事实**：`classify` 需要前/后两个 `Store` 快照；`Context::notify` 只广播受影响**键**，不携带任何状态快照；`Store` 不可克隆（`Box<dyn Any>`），系统内也没有快照机制。因此 Def 26 分类在系统内**无任何调用点**——反应器收到 `(&Context, &[Symbol])` 后既无 `prev` 也拿不到 `next` 以外的历史，无法完成分类。THEORY-MAP 标注"完成（PR #4 分类部分）"——分类**函数**完成，但分类与传播的衔接留白。

**建议**：在 THEORY-MAP 或 notify 模块文档中明确"快照/变更日志机制由 PR #5 提供"，并在 PR #5 设计时确定 `notify` 携带 `prev` 快照（或变更描述）的形态；否则 `classify` 将成为悬空 API。

### m2. `set` 报 `AlreadyBound(key)`，`Store::bind` 报 `AlreadyBound(realm)`——同一错误类型两个载体
**位置**：`context.rs`（`set` 前置检查返回 `StoreError::AlreadyBound(key)`）；`store.rs`（`bind` 返回 `AlreadyBound(realm)`）；THEORY-MAP 偏差记录只覆盖了 Store 层

**事实**：`set` 的前置检查发现 `realm ∈ dom(σ)` 后返回携带**用户键**的 `AlreadyBound`；若绕过前置检查直达 `Store::bind`（或未来 async 路径），错误携带 **realm**。同一 `StoreError::AlreadyBound(Symbol)` 枚举在两个层面语义载体不同，THEORY-MAP 记录"Store 错误携带 realm"未覆盖 set 层的 key 语义。

**建议**：文档化该区分（set 层报用户键、Store 层报 realm），或统一为一个载体。

### m3. `set` 前置检查与绑定之间的 TOCTOU 窗口（单线程安全，async 化后是雷）
**位置**：`context.rs`（`set`：`contains(realm)` 检查 → `self.effect(...)` 内 `bind(...).expect("前置条件已检查")`）

**事实**：单线程 + 无重入路径下窗口不可达，`expect` 安全。但 PR #5 接入 async（`await iter.next()`）后，检查与绑定之间可插入其他任务——届时 `expect` 将 **panic** 而非返回 `Err(AlreadyBound)`，且 panic 发生在效应迭代器内部（步内），`execute` 无 unwind 保护，上下文将处于不一致状态。

**建议**：PR #5 前把绑定错误路径改为可传播（如效应迭代器内捕获错误并跳过绑定，前置检查只作快速失败），或在 async 设计文档中列为必改项。

### m4. `InterceptMeta` 要求 `Send + Sync`，但 `Context` 是单线程 `Rc` 结构
**位置**：`context.rs`（`InterceptMeta: Any + Send + Sync + 'static`）

**事实**：`Box<dyn InterceptMeta>` 存放在 `Context.intercept: RefCell<HashMap<...>>` 内，`Context` 本身 `!Send`（`Rc`/`RefCell`）。`Send + Sync` 约束当前无实际需要，且约束实现者（单线程场景的元数据类型被迫满足线程安全，可能逼出 `Arc<Mutex>` 等噪音）。

**建议**：去掉 `Send + Sync`（与宿主同步核心一致），或在 trait 文档注明"为 wasm/多线程后端预留"的理由。

### m5. `Runtime::on_notify` 只增不减——无反应器移除 API
**位置**：`context.rs`（`on_notify` push 到 `Vec<Reactor>`）

**事实**：fiber 卸载路径（Algorithm 5 第 26 行，PR #5）需要移除对应 fiber 的反应器；当前 API 无移除能力，反应器表单调增长（泄漏 + 幽灵通知）。

**建议**：PR #5 设计反应器注册表时提供移除句柄（如返回 `ReactorId`），或在文档标注该限制。

---

## ⚪ 细节（nit）

1. **`Context::get` 双查找**（`get` 先错误检查、`Ref::map` 再取一次，O(log n)×2）——正确性无碍，可留待性能阶段。
2. **`Reactor` 类型别名未导出**（lib.rs 导出 `Runtime`/`InterceptMeta` 但非 `Reactor`）——外部无法命名该类型，PR #5 若需自定义反应器需导出。
3. **`set` 中 `&[key]` 临时切片**（`ctx.notify(&[key])`）——合法且惯用，无需改。
4. **`store_with` 测试辅助**（notify.rs）中所有绑定共用一个键类型 `K1I`——测试专用可接受。

---

## 正面确认（设计正确、实现到位的点）

- **realm 键控重构**（Def 28/29 转译）：`Store` 以显式 `realm` 为键、`Context` 经 `ρ` 解析——方向正确，`bindings_are_keyed_by_realm` 测试覆盖同键多 realm 独立绑定。
- **`isolate`/`intercept` 派生实现**（Def 27）：继承 `ρ`/`ι`、**空累加器**、原上下文不受影响——`isolation_binds_same_key_independently` 验证隔离独立性；`intercept_merges_right_biased_and_derives` 验证右偏合并与派生语义。
- **`InterceptMeta` 的 dyn-clone 模式**：`merge` 为静态方法（`where Self: Sized` 排除 vtable）保持对象安全 + `clone_box` 深拷贝——trait 设计正确。
- **`set` 可逆 + 双侧 notify**：绑定与撤销两侧均触发通知（Algorithm 2 第 8/11 行），`set_notifies_on_bind_and_unbind` 覆盖。
- **`Context::get` 的 `Ref::map` 借用守卫**：返回 `Ref<K::Value>` 而非裸引用，借用纪律与 `store()` 一致。
- **分类矩阵测试**：`classification_matrix` 覆盖 8 种组合（含"值变更而满足状态不变 = Neutral"的边界）。
- **文档纪律**：THEORY-MAP 已知偏差新增 3 条（fiber 遍历推迟、`Clone` 约束、realm 错误载体），与仓库风格一致。

---

## 总结

- **必须修复**：M1（notify 重入——快照迭代 + 递归语义明示/工作队列，这是反应式机制的核心缺陷）。
- **建议修复**：m1（分类链路衔接）、m2（错误载体一致性）、m3（async 化 TOCTOU 预案）、m4（Send+Sync 约束理由）、m5（反应器移除 API）。
- **nit**：1–4 可忽略。

**置信度**：高——代码事实均直接核验；M1 的两个场景（borrow_mut panic、递归广播）由 RefCell 语义与调用路径直接推出，无不确定处。
