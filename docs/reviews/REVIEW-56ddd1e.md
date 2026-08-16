# 代码审查报告：commit `56ddd1e`（PR #7 DX 层 + hello-plugin 示例）

- **审查对象**：`56ddd1e5eab4c14543aa34ef769277b0e05f56c3`（相对 `2389441`），12 文件，+395/-17 行
- **审查日期**：2026-08-16（仓库时区）
- **核心代码**：`cordis-macro`（`#[component]` 过程宏，127 行）、`cordis-native`（`with_ctx` 辅助）、`cordis` 门面 re-export、`examples/hello-plugin`（M0 验收示例，150 行）
- **验证手段**：`cargo test --workspace` 全绿（含 property 2000 用例与 native 新测试）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 干净；`cargo run -p hello-plugin` 全部断言通过

---

## 🟡 建议修复（minor，无 major）

### m1. 门面 re-export 遗漏 `execute`——与"统一 re-export 全部公开 API"声明不符
**位置**：`crates/cordis/src/lib.rs`（re-export 列表）

**事实**：`cordis_core` 根导出含 `execute`（`pub use effect::{Disposer, EffectIter, Step, execute, once}`），但门面的列表（`Classification, Component, Context, ..., View, classify, once`）**缺 `execute`**。使用 `execute` 的用户无法仅依赖 `cordis` 门面，需额外依赖 `cordis-core`——与门面定位（"统一 re-export 各子 crate 的公开 API"）不一致，且宏文档声称"依赖本门面即可"（对高级用法不成立）。

**建议**：补 `execute`（核对 core 根导出与门面列表逐项一致，防止后续新增 API 再漏）。

### m2. CI 不运行 hello-plugin 示例——M0 验收断言仅在本地手动验证
**位置**：`.github/workflows/ci.yml`（本 PR 未改）；README 声称"全部断言通过即成功"

**事实**：`cargo test --workspace` 只**编译** hello-plugin（bin 成员），不执行其 main；示例的全部断言（激活顺序 → 级联卸载 → 重连）只在 `cargo run -p hello-plugin` 时验证，CI 无此步骤。M0 验收件随引擎演化可能静默过时（README 演示与真实行为脱节）。

**建议**：CI 加一步 `cargo run -p hello-plugin`（示例运行 <1s），把端到端验收纳入门禁。

---

## ⚪ 细节（nit）

1. **宏对未实现 `Key` 的类型报错指向展开后的代码**（`inject()` 生成体内）而非用户属性——DX 不友好，属阶段可接受（宏糖是薄包装）。
2. **宏重复参数覆盖语义未文档化**（`inject = [A], inject = [B]` 时后者覆盖）——建议文档注明或改为报错。
3. **宏 doc 示例用 ```ignore 不编译验证**——hello-plugin 已有真实编译覆盖，可接受。
4. **`::cordis` 路径要求依赖名固定为 `cordis`**（别名依赖会破坏宏展开）——文档已说明，可接受。

---

## 正面确认（实现正确的点）

- **宏卫生**：生成代码全部绝对路径（`::cordis::`、`::std::rc::Rc`、`::std::boxed::Box`），无名称捕获风险；泛型经 `split_for_impl` 正确处理；解析错误经 `to_compile_error` 走标准错误路径；proc-macro crate 结构标准（`[lib] proc-macro = true`，syn full + quote）。
- **类型安全**：`inject/provide` 列表中的类型经 `<T as ::cordis::Key>::SYMBOL` 完全限定——非 `Key` 类型在用户 crate 编译期报错（不产生运行期错误）。
- **`with_ctx`**：`once(Box::new(move || step(&ctx)))` 实现正确简洁，测试覆盖激活 + 绑定往返。
- **hello-plugin 端到端质量**：真实覆盖三阶段（依赖激活顺序 → 退役级联 Thm 63 → 移除后供给名释放与 auth 自动重连），断言驱动、输出清晰，是合格的 M0 验收件。
- **文档纪律**：THEORY-MAP 新增 2 条记录（DX 层、示例），PLAN M0 里程碑同步。

---

## 总结

- **必须修复**：无。
- **建议修复**：m1（门面补 `execute`）、m2（CI 运行 hello-plugin）。
- **nit**：1–4 可忽略。

**置信度**：高——m1 经 core 根导出与门面列表逐项比对确认；m2 经 ci.yml 现状与示例执行方式直接核验；其余为代码事实。
