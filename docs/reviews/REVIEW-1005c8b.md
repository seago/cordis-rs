# 代码审查报告：commit `1005c8b`（M0.1 前置小修 + cordis-async 骨架，Phase 0）

- **审查对象**：`1005c8b890a9e98ff220adcc4e425b59ebe075a8`
- **审查范围（静态）**：`git show 1005c8b --stat` / `git show 1005c8b`，对照 `docs/cordis-async-protocol-draft.md` v1.4 与 `docs/cordis-async-PHASE0-PLAN.md`
- **审查日期**：仓库时区
- **验证手段**：静态阅读（按要求不运行任何测试/构建/命令）

**改动统计**：6 文件，+118/-2。
- `crates/cordis-core/src/fiber.rs` +10：P1 `Fiber::target_view()`
- `crates/cordis-loader/src/lib.rs` +14/-2：P2 两处 hook 闭包改捕获弱引用
- `crates/cordis-async/`（Cargo.toml + src/lib.rs）新建：骨架占位
- `Cargo.toml` +1（workspace member）、`Cargo.lock` +20（cordis-async + tokio-macros）

---

## 逐条发现

### P1：`Fiber::target_view()`（crates/cordis-core/src/fiber.rs）✅ 通过

```rust
pub fn target_view(&self) -> Option<View> {
    self.target.borrow().clone()
}
```

- **只读**：`target: RefCell<Option<View>>`，方法先 `borrow()` 再 `.clone()`，借用守卫在表达式结束时释放，不返回悬垂 `Ref`，无 `borrow_mut` → 与 reload 的 `guard_target`（runtime.rs:468-471：`fiber.target.borrow().as_ref() == Some(&guard_target)`）同款只读借用纪律。`View = BTreeMap<Symbol, FiberId>`（fiber.rs:30），`Clone` 成立。
- **零语义变化**：新增 pub 访问器，无既有调用点，不改动 `target` 的写入方（`target` 仍为 `pub(crate)`，权威重算在 `refresh`）。doc 明确「只读」「与 `Runtime::refresh` 重算路径解耦」「目标仍由 `refresh` 权威重算」——文档与实现一致，无矛盾。
- 结论：符合「零语义变化 + borrow 克隆 + 与 guard_target 同款」的声明。

### P2：loader update/retire hook 弱引用（crates/cordis-loader/src/lib.rs）✅ 通过

改前：`register_update_hook(self: &Rc<Self>)` / `register_retire_hook` 闭包捕获强 `Rc::clone(self)`，存于 `runtime.set_update_hook/set_retire_hook`。引用链 `Loader → Loader.runtime(Rc<Runtime>) → Runtime.{update,retire}_hook(Rc<闭包>) → Rc<Loader>` 构成强环 → 关停泄漏。

改后：`let loader = Rc::downgrade(self);`，闭包捕获 `Weak<Loader>`；回调首行 `let Some(loader) = loader.upgrade() else { return; };`。

- **引用环消除**：强环 `Loader→runtime→hook→Loader` 断开，`Weak<Loader>` 不保活 Loader。`Runtime.update_hook/retire_hook` 为 `RefCell<Option<Rc<...>>>`（runtime.rs:79,83）强持有闭包，但闭包只持有 Weak → 无强环。✓
- **改前行为等价**：Loader 存活期间 `upgrade()` 恒为 `Some(Rc::clone)`，回调体逐字不变（`update`：`entry_of` → `find_loaded_mut` 写 `config`；`retire`：`in_apply` 分支 push `retire_pending` / 否则 `writeback_retire`）。唯一行为增量是 loader 已 drop 时静默跳过——即预期的关停语义（之前根本 drop 不掉）。✓
- **retire_pending / in_apply 路径未破坏**：新增守卫仅在闭包顶部，`upgrade()` 成功后 `loader.in_apply.get()` / `loader.retire_pending.borrow_mut()` 语义与改前逐字相同；apply 协调期延迟排空与 apply 外立即写回两分支不受影响。✓

### 骨架：crates/cordis-async/ ✅ 通过

- **协议类型占位与草案 §1 一致**：`lib.rs` 的 `LocalBoxFuture<T> = Pin<Box<dyn Future<Output=T> + 'static>>`、`AsyncDisposer = Box<dyn FnOnce() -> LocalBoxFuture<()> + 'static>`、`trait AsyncEffectIter: 'static { fn next(&mut self) -> LocalBoxFuture<AsyncStep> }`、`enum AsyncStep { Yielded/Finished/Failed }`、`struct AsyncFiberError(String)` 且 `derive(Clone, Debug, PartialEq, Eq)`——与 `docs/cordis-async-protocol-draft.md` §1（v1.4 冻结）逐项一致。新增便利方法 `AsyncFiberError::new/message` 属草案之外的合理增强，不构成矛盾。
- **tokio features 最小化**：run-deps `tokio = { version="1", features=["rt"] }`（最小）；dev-deps `["rt","macros","sync"]`（测试用）。与 Phase 0 Plan §Step 0 的「按需最小 feature」一致；dev-deps 的 extra feature 经 Cargo 对本 crate 依赖图统一启用，但不污染 sync 侧零依赖（tokio 仅入本 crate）。✓
- **deny(missing_docs) + workspace lints**：`#![deny(missing_docs)]` 本地开启，`[lints] workspace = true` 继承 `unsafe_code=deny` 与 `clippy all=warn`；`#![allow(dead_code)]` 注明「M0.1 骨架占位，各里程碑逐项启用」——标注合理、符合 Phase 0 Plan §Step 0 的骨架定位。✓

---

## 问题清单

### nit-1（唯一发现）：broken intra-doc links（crates/cordis-async/src/lib.rs）

`lib.rs` **未 `use` 任何 cordis_core 类型**（仅有 `use std::future::Future; use std::pin::Pin;`），而若干 doc 注释使用了非限定的 intra-doc 链接：

- 行 20：`` [`Disposer`] ``（`AsyncDisposer` doc）
- 行 33 / 35：`` [`Step`] `` / `` `Step::Yielded` ``（`AsyncStep` doc）
- 行 43：`` [`FiberError`] ``（`AsyncFiberError` doc）

`cordis-core` 无 crate 根重导出（其 `lib.rs:21-30` 均为裸 `pub mod`），故这些链接无法在当前模块作用域解析 → rustdoc 报 `broken intra-doc links` 警告。本 crate 仅 `deny(missing_docs)`，工作区 lints 未 `deny(rustdoc::broken_intra_doc_links)`，因此**不阻塞编译/合入门禁**，但属真实缺陷（rustdoc 生成时告警，会污染后续 -D warnings 型文档检查）。

> 注：本报告之前的独立审查版本在此点误判为「intra-doc 链接可解析 ✅」，经核验不成立——故以本报告为准（有争议处已复核）。

**位置**：`crates/cordis-async/src/lib.rs` 行 20、33、35、43。
**建议**：改为全限定路径（核心类型路径已核实）：
- `cordis_core::effect::Disposer`
- `cordis_core::effect::Step`
- `cordis_core::fiber::FiberError`

或顶部 `use` 相关类型（注意 `#![allow(dead_code)]` 下未用的 `use` 可能触发 unused 警告，全限定 intra-doc 路径更稳妥）。

---

## 总体结论

✅ **通过（1 项 nit 需修）**

- **major**：0
- **nit**：1（nit-1 未解析 intra-doc 链接，非阻塞，建议全限定修复）

P1 只读访问器语义零变化、P2 弱引用真实消除 `Loader→runtime.hook→Loader` 强环且改前行为等价 / retire_pending·in_apply 路径未破坏、async 骨架类型与草案 §1 一致 / tokio features 最小化 / lints 标注合理——三线均核实通过，无 major 语义错误、无未消环、无文档矛盾。nit-1 为文档级小问题，可合入门禁后作为后续小修处理。
