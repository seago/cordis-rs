# 代码审查报告：commit `1df64a1`（PR #13 双后端共存）

- **审查对象**：`1df64a1f6c28afc4b26734d05523113bcec14f65` + docs `458a15e`，3 文件，+194 行（`dual_backend.rs` 188 行 + Cargo.toml dev-dep）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`cargo test --workspace` **74 测试全绿**（dual_backend 2 通过）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 干净

---

## 🟡 建议修复（minor，无 major）

### m1. `Cargo.lock` 未随 `Cargo.toml` 提交——可复现构建纪律中断
**位置**：`crates/cordis-wasm/Cargo.toml`（+`cordis-loader` dev-dep）；工作树 `M Cargo.lock`（未提交）

**事实**：本 commit 新增 dev-dependency 但**锁文件更新仍留在工作树**（cordis-wasm 的依赖列表新增 `cordis-loader`）。CI 非 `--locked` 模式不红，但仓库 PR #2 已确立"commit Cargo.lock for reproducible builds"纪律。建议随提交补上锁文件。

### m2. `Value` 类型住在 cordis-wasm——原生互通需依赖 wasm crate（架构耦合方向）
**位置**：`dual_backend.rs`（`use cordis_wasm::wit::cordis::core::context::Value` 供原生组件装箱）

**事实**：值类型统一决策（原生侧也用 wit `Value` 装箱）正确，但 `Value` 类型定义在 `cordis-wasm` 的 wit 绑定中——**原生组件要与 wasm 组件互通，必须依赖 cordis-wasm crate**（仅为一个值类型）。依赖方向变成"原生 → wasm"，与"wasm 依赖 core、core 无关后端"的既有分层不一致。测试内可行；生产上建议后续把 `Value`（或统一值类型）下沉到 `cordis-core`/独立 value crate（M2 或正式双后端支持前处理）。文档可先记录该边界。

---

## ⚪ 细节（nit）

1. **`Cargo.toml` 多余空行**（`anyhow = "1"` 与 `[dev-dependencies]` 之间两个空行）——格式瑕疵。
2. **原生测试组件手写 `Component` impl**（未用 `#[component]` 宏）——测试组件需符号级声明（无 Key 类型），宏不适用，合理。

---

## 正面确认（实现正确的点）

- **M1 门禁 1/3 达成**：同一 `Loader` 同时加载原生与 wasm 组件——`WasmComponent: Component` 使 loader 的 `register_component`/`apply` 天然兼容两类，无需 loader 改动（抽象边界验证成功）。
- **双向值互通闭环**：① 原生 consumer 经 `get_dyn` 读 wasm 提供的 wit `Value`（`native(wasm-pg)`）；② wasm consumer 经 `sync_injected` 读原生提供的 wit `Value`（`derived(native-pg)`）——PR #12 的"仅 wit Value 同步"分支在此被原生 provider 场景**正向验证**（此前只有 wasm→wasm 路径）。
- **级联正确性**：两个测试均验证条目移除 → 依赖者（原生或 wasm）级联停用 → 绑定全清 → 静止。
- **借用纪律**：`get_dyn` 的 `Ref` 块级隔离（`set_dyn` 前释放）注释与实现一致。
- **值类型统一决策文档化**：THEORY-MAP 记录"跨类型值翻译边界收窄为双方走动态值 API 即可互通"。

---

## 总结

- **必须修复**：无。
- **建议修复**：m1（提交 Cargo.lock）、m2（Value 类型下沉/记录边界）。
- **nit**：1–2 可忽略。

**置信度**：高——测试实测通过；m1/m2 为代码与工作树事实直接核验。
