# 代码审查报告：commit `798d479`（PR #8 loader 最小协调）

- **审查对象**：`798d479d5f9a4cbd4e925e6e762a05facec90e12` + docs `fcb565e`，9 文件，+485/-39 行
- **审查日期**：2026-08-16（仓库时区）
- **核心代码**：`cordis-loader`（`Entry`/`Loader` 增量协调，+452 行）、`config` 参数 `Box<dyn Any>` → `Rc<dyn Any>` 迁移（core/native/示例/测试随迁）
- **验证手段**：`cargo test --workspace` 全绿（loader 6 测试）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 干净；`cargo run -p hello-plugin` 通过

---

## 🟡 建议修复（minor，无 major）

### m1. 替换条目（同供给）的单次 `apply` 顺序缺陷——先新增后移除导致 `ProvisionClash` panic
**位置**：`crates/cordis-loader/src/lib.rs`（`apply`：先 `for entry in desired { load/reconcile }`，后移除消失条目）

**事实**：`apply` 先处理所有 desired（新增/重建），**然后**才移除消失条目。替换场景（desired 用 Y 替换 X，X/Y 提供同一供给键）：Y 实例化时 X 仍在 registry → `Runtime::register` 的供给不相交检查命中 X → `ProvisionClash` → **panic**（配置错误策略），协调中止且 X 也未移除。用户被迫分两次 `apply`（先清空 X 再上 Y）才能原子替换——这是配置协调的常见场景，文档未说明。

**建议**：两阶段协调（先移除消失/重建条目释放供给名，再实例化新增/重建条目）；或至少文档注明"替换同供给条目需分两次 apply"。

### m2. `unload_fiber` 的 `HasChildren` panic 可达——文档假设与公开 API 冲突
**位置**：`crates/cordis-loader/src/lib.rs`（`unload_fiber` 内 `remove_fiber(...).unwrap_or_else(|err| panic!("...（根级条目应无子代）"))`）

**事实**：文档声明"条目全部实例化在 root、无子代，`HasChildren` 前提不受影响"。但 `Loader::fiber(id)` 与 `Fiber::ctx()` 都是公开 API——用户**可以**在 loader 管理的条目 fiber 下注册子组件（`loader.fiber("x").unwrap().ctx().use_component(...)`），此后该条目被移除/重建时 `remove_fiber` 返回 `HasChildren` → panic，且错误消息"根级条目应无子代"对用户误导（明明是用户创建的子代）。

**建议**：错误消息改为如实说明（"条目存在子代 fiber，先处理子代"），或在协调中检测子代并级联移除；至少文档注明"loader 管理的条目不得在其 fiber 下实例化子组件"。

### m3. disabled 条目上的 `component`/`revision` 变更静默——逻辑正确但零测试覆盖
**位置**：`crates/cordis-loader/src/lib.rs`（`reconcile` 的 `if entry.disabled { return; }` 分支）

**事实**：disabled 且状态未变时 component/revision 变更**不更新记录**（`LoadedEntry` 保持旧值）；enabled 分支用**新 entry** 实例化并 `update` 记录——最终一致，逻辑正确。但"disabled 期间改组件 → enabled 后用新组件"的路径**无测试**（现有 `disabled_toggle_unloads_and_reloads` 同 component/revision 往返）。

**建议**：补一条测试固化该路径（防止未来重构破坏）。

---

## ⚪ 细节（nit）

1. **desired 中重复 id**：first load + second reconcile，component/revision 不同时浪费一次实例化（last-wins 语义）——可文档化或直接拒。
2. **`Entry` 的 `derive(Debug)` 对 `config: Rc<dyn Any>` 输出 "Any"**——调试信息无价值（std 的 `dyn Any: Debug` 恒打印 "Any"），如需可观察配置建议字段级定制。
3. **`register_component` 只增不减**（无 unregister）——与 PR #4 反应器同型，M2 处理可接受。
4. **`Box → Rc` 为破坏性 API 变更**——0.1 阶段直接改合理，THEORY-MAP 已记录理由（loader 重建需复用配置）✅。

---

## 正面确认（实现正确的点）

- **协调四分支覆盖完整**：新增实例化 / 消失卸载 / `disabled` 切换 / `component`+`revision` 变更重建，未变条目零操作（幂等以 fiber id 不变断言）——§5.2.1 per-field dispatch 语义正确。
- **级联正确性**：条目移除走 `retire → remove_fiber` 标准路径，依赖者级联停用（`removed_entry_unloads_and_cascades` 验证，绑定全部恢复）。
- **`config` 迁移理由充分**：loader 重建需保留配置，`Rc` 共享是正解；`register` 的 apply 闭包捕获 `Rc` 后每次 reload 复用同一配置，无生命周期问题。
- **测试覆盖**：6 个测试（依赖序加载、幂等、disabled 往返、config 重建、移除级联、未知组件 panic）覆盖主路径。
- **文档纪律**：THEORY-MAP 新增 5 条记录（最小范围、revision 代行 config diff、根级实例化、panic 策略、API 调整），PLAN M0 同步。

---

## 总结

- **必须修复**：无。
- **建议修复**：m1（替换条目顺序——两阶段协调或文档化）、m2（HasChildren panic 可达——消息/文档修正）、m3（disabled 期间变更的测试固化）。
- **nit**：1–4 可忽略。

**置信度**：高——m1 由 `apply` 的执行顺序与 `register` 的供给检查直接推出；m2 由公开 API 链（`Loader::fiber` → `Fiber::ctx` → `use_component`）可达性确认；m3 为测试缺口事实。
