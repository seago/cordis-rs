# 代码审查报告：commit `1440fce`（落实 PR #2 审查发现）

- **审查对象**：`1440fce65541db7fbfcaa53726eb77bc33b93e7a`（相对 `8ddb885`），7 个文件，+219/-13 行
- **审查日期**：2026-08-15（仓库时区）
- **上游**：对 `docs/reviews/REVIEW-8ddb885.md`（commit `8ddb885` 审查）的落实核查
- **验证手段**：`cargo test -p cordis-core` **28/28 全绿**（原 7 + 新增 21：interp +1、store +5、symbol +3、keyset +3、PR #3 的 effect/context +9）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` **零警告**（与 CI 门禁一致）

---

## 落实对照（8 项应落实项，全部落实 ✅）

| 审查项 | 落实方式 | 验证 |
|---|---|---|
| M1 Store/Symbol/Key 测试缺失 | `store.rs` +5 测试（三分支、unbind 类型检查先于移除、satisfies ∀ 语义、`symbols()` 计数）；`symbol.rs` +3；`keyset.rs` +3 | 全部通过；THEORY-MAP 同步修正（`Key`/`FiberId` 诚实标注"间接覆盖"） |
| M2 `Symbol` 序跨运行 | 走文档修正路线：`keyset.rs:8` 改为"进程内确定"、`symbol.rs` 模块文档注明跨边界以名称为媒介、THEORY-MAP 已知偏差记录 | 声明与实现一致 |
| m1 `reload` 错误分类 | `reload` 先 `contains_key` 检查；新测试 `lifecycle_rules_on_unknown_fiber_classify_consistently` 统一 4 规则 → `UnknownFiber` | 通过 |
| m2 `symbols()` 确定性 | `HashMap` → `BTreeMap`（doc 同步） | 编译通过 |
| m3 CI 门禁 | 恢复 `RUSTFLAGS: "-D warnings"` + clippy `-- -D warnings`，注释说明零依赖前提与未来局部放宽路径 | 本地复现通过 |
| m4 ProvisionClash 退役 fiber | 代码注释 + THEORY-MAP 记录；作者核对论文后确认"与论文一致，无偏差"（比审查时更确定的结论） | 记录准确 |
| nit1 字符串双拷贝 | `by_name: HashMap<&'static str, u32>` 借用已泄漏存储，`Box::leak(name.into())` 单份分配 | 编译通过（`&'static str: Borrow<str>` 成立） |
| nit7 步数上界 | `drive_to_quiescence` 注释补充推导（每 fiber 轮换 ≤ 6 步、8 倍裕量） | 无行为变化 |

**未处理项**（均属原审查中"可接受/无需处理"类别，不阻塞）：nit2 全局锁热点、nit3 `NameExists` 死代码、nit4 `unload` 先算 target、nit5 `FiberId::fresh` 测试绕过、nit6 `Debug` 不转义。

## 新问题检查：未发现

- `symbol.rs` 中 `HashMap<&'static str, u32>` 的 `get(&str)` 借用查找——合法，编译通过
- `reload` 中 `contains_key` → `target`（不可变借用）→ `get_mut` 的借用顺序——合法，无悬垂/双借
- CI 组合（env `RUSTFLAGS` + clippy `-- -D warnings`）——rustc 与 clippy 两级门禁叠加，无冲突
- 新测试均不依赖跨运行顺序（`Symbol::intern` 同进程同一性），无脆断风险
- `docs/reviews/REVIEW-8ddb885.md` 在本次提交中以重命名方式纳入（`| 0` 行变化），与落实内容一并入库

## 两个可选的微观察（非问题）

1. `drive_to_quiescence` 注释的推导"每 fiber ≤ 6 步"偏保守（正常轨迹实际 ≤ 3 步：reload→unload→reload）；结论无碍（8 倍裕量 + panic 兜底），如追求严谨可简化为"≤ 4 步/每 fiber 已覆盖，8 倍留裕量"。
2. `interp.rs` 测试中 `FiberId::fresh(&mut 100)` 造 ghost id 的模式已出现两处（`unknown_parent_rejected` 与新测试），可提取 `fn ghost()` 辅助函数。

## 总体结论

✅ 落实完整、忠实于审查意见，且测试先行（新增测试先于/伴随行为修正），文档（THEORY-MAP 已知偏差、模块注释）与实现同步更新，CI 门禁在零依赖前提下恢复并注明未来路径。未引入新问题，可合入。
