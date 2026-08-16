# 代码审查报告：commit `3a54c01`（PR #6 oracle × 引擎元理论 property suites）

- **审查对象**：`3a54c019e6403b54d930cbe6c195dfb346570d98`（相对 `64ce520`），6 文件，+634/-5 行
- **审查日期**：2026-08-16（仓库时区）
- **核心代码**：`tests/property.rs`（253 行，proptest 双驱对比套件）、`runtime.rs`（+38 行公开只读 API：`active_fibers`/`provided`/`store`）、`Cargo.toml`（dev-dep proptest）
- **验证手段**：`cargo test -p cordis-core` **53 单元 + 1 property 全绿**；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 干净；`PROPTEST_CASES=2000` 手动跑亦通过（0.55s）

---

## 🟡 建议修复（minor，无 major）

### m1. THEORY-MAP 声称"2000 用例"，实际 CI 只跑 256——验证强度文档与事实不符
**位置**：`docs/THEORY-MAP.md`（"随机编排 ≤12 步 × 2000 用例"）；`tests/property.rs`（无 `proptest_config` 覆盖）；`.github/workflows/ci.yml`（无 `PROPTEST_CASES`）

**事实**：proptest 默认 `cases = 256`；测试文件无 `#![proptest_config(...)]`、CI 无环境变量。实测：默认配置下 property 测试 0.07s 完成（256 用例特征）；`PROPTEST_CASES=2000` 时 0.55s 通过。即**引擎确实能过 2000 用例**，但 CI 实际执行的强度只有声称的 1/8。

**建议**：三选一——① 在 `property.rs` 加 `proptest_config!(ProptestConfig::with_cases(2000))`（固化强度，0.55s 可接受）；② CI 设 `PROPTEST_CASES: 2000`；③ 修正 THEORY-MAP 为实际值。推荐 ①（强度随仓库走，不受 CI 环境影响）。

### m2. 动作空间缺 parent 维度——`HasChildren` 前提与父级联（Def 47）在 property 套件中不可达
**位置**：`tests/property.rs`（`RawAction::Insert` 无 parent 字段，恒 root；`Harness::apply` 恒 `oracle.insert(None, ...)`）

**事实**：全部插入挂在 root 下 → 任何 fiber 都无子代 → `Remove` 的 `HasChildren` 错误分支（oracle `Violation::HasChildren` / 引擎 `RegistryError::HasChildren`）**永不被 property 覆盖**；父卸载级联退役子的语义（PR #5 集成测试 `parent_unload_cascades_to_children` 覆盖）也缺随机化验证。

**建议**：`RawAction::Insert` 增加 `parent: Option<insert_idx>` 维度（随机引用已插入 fiber），oracle 与引擎侧对应传 parent——引擎 `register` 目前无 parent 参数（恒 root），需先补齐引擎的父级实例化能力（或至少让 oracle 侧 parent 也恒 None 并在文档声明"父拓扑留待 PR #7 嵌套组件"）。当前实现下这是**覆盖缺口**，至少应在套件文档注明。

---

## ⚪ 细节（nit）

1. **`bind_symbol` 的 3 键 match 硬编码**（`tests/property.rs`）——键宇宙扩展时需同步修改；测试局部可接受。
2. **compare 范围诚实但有限**：只对比活跃集/σγ/绑定总数，不对比 committed view / 退役标志 / inject-provide 表——文档已明确声明范围（"活跃集/σγ/绑定总数逐步一致"），无问题，仅记录。
3. **`inserted` 向量保留已移除 fiber 的 id**（`Remove` 成功后不删条目）——后续 Retire/Remove 引用已移除 id 时两侧一致（`Err`/`None`），语义正确。

---

## 正面确认（设计良好、实现正确的点）

- **双驱对齐机制正确**：oracle 与引擎都在**前提检查通过后**才分配 fiber id（拒绝的插入不消耗 id），`assert_eq!(oid, fiber.id())` 的硬断言成立。
- **错误一致性断言**（同侧拒绝）覆盖 `ProvisionClash`/`UnknownFiber`/`NotRetired`/`StillActive` 四类前提——oracle 与引擎的拒绝行为逐步互锁。
- **Thm 66**：每个编排动作后立即断言 `is_quiet`（同步核心的 progress 形态）；**Thm 73**：活跃集与 σγ 逐步一致；**Cor 62**：绑定总数 == σγ 大小（离场无残留）——三定理联合断言设计正确。
- **Def 69 规范组件**：CanonicalComponent 激活恰好安装 provide 全键，与 oracle 规范化建模一致，使对比有效。
- **dev-dependency 纪律**：proptest 仅 dev-dep，库本身零依赖声明不被破坏（Cargo.lock 计入仓库，与 PR #2 的决策一致）。
- **失败诊断**：`compare(&format!("step {i}: {action:?}"))` 的上下文断言利于 proptest 收缩定位。

---

## 总结

- **必须修复**：无。
- **建议修复**：m1（用例数声明与配置对齐——推荐在测试文件固定 2000）、m2（parent 维度缺口——补引擎父级实例化或在套件文档声明留待 PR #7）。
- **nit**：1–3 可忽略。

**置信度**：高——m1 经实测确认（默认 256 / PROPTEST_CASES=2000 通过）；m2 由动作空间定义与 `register` 无 parent 参数的代码事实直接推出。
