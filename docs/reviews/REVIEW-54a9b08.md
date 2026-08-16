# 代码审查报告：commit `54a9b08`（PR #12 wasm 依赖者消费）

- **审查对象**：`54a9b0818d7bc1f4f3718c0ab1687b8f42bc5b70` + docs `e447ec9`，8 文件，+586/-12 行
- **审查日期**：2026-08-17（仓库时区）
- **核心代码**：`Context::get_dyn`/`Store::get_value`（读侧类型擦除）、`WasmTaskIter::sync_injected`（注入同步）、consumer guest 示例 + `dependency_consumption.rs` 测试
- **验证手段**：`cargo test --workspace` **70 测试全绿**（新增 1）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 干净（上轮发现的 `get_dyn` parens 已修）；两个 guest 锁文件均入库一致

---

## 🟡 建议修复（minor，无 major）

### m1. `set_dyn` 对称性修复（984f3ee）后仍无 isolate × wasm 交叉回归测试
**位置**：`crates/cordis-wasm/tests/`（`bridge_core`/`dependency_consumption` 均无隔离覆盖）

**事实**：上轮 m1/m2 的修复（`set_dyn` 改按键判定 + 内部 `resolve_realm`）已使语义正确，但**没有任何测试覆盖"隔离上下文 + wasm 组件"组合**——`isolate(key, realm)` 后 use wasm 组件、供给纪律按键判定、`get_dyn`/`set_dyn` 的 ρ 解析路径均未验证。修复正确性目前仅由代码审查保证，缺回归护栏（与 PR #5 审查 M1 同型的问题：修复落地后未补交叉测试）。

**建议**：补一条交叉测试（隔离提供者/消费者双 wasm 或原生+wasm 混合），固化 m1/m2 修复。

---

## ⚪ 细节（nit）

1. **`sync_injected` 的 `is::<Value>()` + `downcast_ref::<Value>().expect(...)` 双查**——可合并为单次 `downcast_ref::<Value>()`（`if let Some(v) = value.downcast_ref::<Value>()`），省一次类型查询。
2. **每 step 全量同步注入键**（`next()` 开头 O(|inject|) 次 store 查找 + clone）——M1 阶段注入集小，可接受；注入集大时需增量同步（M2 优化项）。

---

## 正面确认（实现正确的点）

- **`get_dyn`/`set_dyn` 完全对称**：读侧同样经 `resolve_realm` 由核心承担隔离解析、`Ref::filter_map` 返回类型擦除引用、借用纪律与 typed `get` 一致——wasm 桥接的键/realm 语义统一。
- **`sync_injected` 边界声明诚实**：仅 wit `Value` 装箱（另一 wasm 组件提供）时同步；原生组件提供的值不同步（`mirror.remove` 防过期镜像）——跨类型值翻译的 M1 边界明确写入文档与 THEORY-MAP。
- **借用纪律三处预防性修复**（均为真实问题）：`apply` 中 `call_start` 收进作用域块（新增 `self.inject()` 借用前必须释放旧借用）；`run_inverse` 两步 take（if-let 临时借用显式结束）；测试块级隔离 `Ref`——单线程 RefCell 纪律意识到位。
- **双 wasm 依赖消费端到端闭环**：provider 激活 → consumer 注入满足激活 → step 内读镜像派生 `derived(wasm-pg)` → provider 退役 → consumer 级联停用 → 绑定全清——wasm 组件完整参与核心 notify/refresh 级联。
- **文档**：THEORY-MAP 记录 PR #12 完整（含借用纪律说明）；`REVIEW-e297098` 重命名入库与我方一致。

---

## 总结

- **必须修复**：无。
- **建议修复**：m1（补 isolate × wasm 交叉回归测试）。
- **nit**：1–2 可忽略。

**置信度**：高——语义推演与借用路径直接核验；测试实测通过。
