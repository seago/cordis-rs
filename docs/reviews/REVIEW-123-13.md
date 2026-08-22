# Backlog 清理线 ①③ 审查（REVIEW-dbc2384 n-1 闭环 + requirements 账本审计）

**审查对象**：0f48fe5（① `is_suspended` 生产化消费）/ b27a9c5（③ requirements 账本审计）/ d5c1c63（计划文档）——backlog 清理线。
**审查日期**：2026-08-22。**手段**：静态阅读 + 逐点核对 + 门禁全量实测。

---

## 总体结论：✅ PASS —— ①③ 均达成（0 Major / 3 Minor / 2 Nit）

核心 ① 重构正确、公开语义不变、门禁全绿；③ 账本断言与代码/提交证据逐点吻合。仅 3 项文档一致性 Minor（THEORY-MAP 关联），不阻塞本线出口，建议落地。

### ① 核查命中（逐点）

**字段与维护点删除**（0f48fe5，仅动 fiber.rs/runtime.rs）：
- `Runtime.suspended: RefCell<HashSet<FiberId>>` 字段、初始化 `HashSet::new()`、`use std::collections::{...HashSet}` 引用全部移除 —— `grep HashSet runtime.rs` 零命中 ✓
- 4 处旧维护点（advance 撤/登、reload 挂起登记、unload 撤销）逐处删除，无 `self.suspended` 残留 ✓

**派生重构正确**：`suspended_fibers()` 现为 `self.fibers.borrow().values().filter(|f| f.is_suspended()).map(|f| f.id()).collect()`——单一事实来源（`resumable`），遍历 `fibers` 表（`fibers` 为全部活跃 fiber，激活 line 240 插入、retire line 279 移除；挂起 fiber 必在表内）✓

**等价性（关键）**——逐一比对 `resumable` 状态迁移 vs 旧 `suspended` 集合维护点（完全双射）：

| `resumable` 迁移 | 位置 | 旧 `suspended` 对偶 | 一致性 |
|---|---|---|---|
| init `None` | fiber.rs:238 | 未在集合 | ✓ |
| advance `take()`（→None） | runtime.rs:317 | 撤（remove） | ✓ |
| advance Err 再挂起 `Some` | runtime.rs:339 | 登（insert） | ✓ |
| reload 挂起分支 `Some` | runtime.rs:564 | 登（insert） | ✓ |
| unload 收账 `take()`（→None） | runtime.rs:598 | 撤（remove） | ✓ |

- L-Raise 路径：reload 捕获 FiberError → `self.unload(fiber)`（line 555）→ unload 收账 `resumable.take()` → `None` → 不挂起；旧代码同经 unload remove。语义一致 ✓
- 恢复完成后 `resumable=None` ↔ 不在集合；挂起期间 `resumable=Some` ↔ 在集合。**无遗漏路径令两语义分叉**——删集合后派生集与旧维护集**逐状态等价** ✓

**公开语义不变**：签名 `-> Vec<FiberId>`、集合成员、撤销/恢复行为不变；迭代顺序均未定义（旧 HashMap 无序、新 HashMap `values()` 无序——测试均先 `sort` 或单元素断言，无序容忍）✓

**测试未改仍绿**：0f48fe5 未改动任何 `#[test]`；P-3 直证 `suspended_set_tracks_and_batch_advances`、`update_during_suspend_reclaims_and_restarts` 均在 core 61/61 中通过 ✓；a2_e2e（wasm 侧消费 `is_suspended`）3/3 ✓

**门禁（实测）**：`fmt --check` ✅ / `clippy --workspace --all-targets -- -D warnings` ✅ / `test --workspace` 全绿（core 61、loader 60、wasm lib 8、a2_e2e 3、go_guest 2 无 ignore…）/ `doc --workspace --no-deps` ✅ 0 告警。

### ③ 核查命中（逐点）

| 账本断言 | 证据 | 判定 |
|---|---|---|
| core 61/61 | `cordis_core` lib 61 passed | ✓ |
| loader 60/60 | `cordis_loader` lib 60 passed | ✓ |
| a2_e2e 3/3 绿 | `a2_e2e` 3 passed | ✓ |
| go_guest 无 ignore | `go_guest` 2 passed / 0 ignored | ✓ |
| build.sh 第 0 步 wit-bindgen 重生成 + go.mod 恢复 | build.sh:21-22 | ✓ |
| `poll_and_advance` 存在 | cordis-wasm lib.rs:440 | ✓ |
| ERRORS-QUIET §3bis（O-4 定案） | cordis-ERRORS-QUIET.md:44 | ✓ |
| P-3 挂起集生产化 / P-6 版本化键 / P-7 O-1/O-4 / backlog ① | 各提交 + THEORY-MAP 记录 | ✓（见 Minor） |
| 边界清单 / 冻结协议 / 公开面 | 核对属实 | ✓ |

基线 8752d0e→bdb8905 更新、A2b 更正为已闭环、wasm 边界更正为完成——均与提交链（P-4 `951709d`、A2b `6a714ca`）吻合，无夸大。

---

## Minor（建议落地，不阻塞出口）

- **M-1（文档-计划偏差）**：计划 §①（line 19）称复杂度 `O(k)→O(n)`、`文档注明`。但 `suspended_fibers` docstring（runtime.rs:346-347）只提"单一事实来源"，**未注明** O(n) 遍历权衡。最小修复：docstring 补一句"派生遍历 `fibers` 表，O(n)（n=fiber 数）；组合内核规模小，可接受"。
- **M-2（③ 账本不准确）**：requirements 文档 §二 async 行称 backlog ① "**THEORY-MAP 授权偏离标注齐备**"。但 (a) THEORY-MAP **无 backlog-① 行**（grep 零命中），(b) 计划 ①-2 明确"core 内部重构、无公开 API/语义变化 → **不需** THEORY-MAP 授权行"。该断言与事实及计划相悖。最小修复：改为"无需 THEORY-MAP 授权（内部重构，公开语义不变，见计划 ①-2）"。
- **M-3（THEORY-MAP 陈旧引用）**：THEORY-MAP 行 170-171（P-3 授权行）仍引用 `Runtime.suspended` 挂起集字段——已被 ① **删除**。该行现为陈旧描述，且行 170 起行 171 结束处疑似表格行断裂（P-3 行尾"…单独立案）**: `Runtime.suspended`…"被并入 P-7 行末，P-3 行首 170 未闭合——先前 B-A1 修复遗留，非 ① 引入）。最小修复：将 P-3 行字段引用改为"挂起集现由 `is_suspended` 派生（backlog ① 单一事实来源）"，并修复行断裂。

### Nit（记录）
- n-1：`suspended_fibers()` 派生依赖 `fibers` 表含全部挂起 fiber——补一断言或文档说明"挂起 fiber 必在表内"更稳（现经测试间接覆盖）。
- n-2：requirements §六 自称"未发现账本级错误之外的偏差"，但存在 M-2/M-3 文档一致性偏差——自评略乐观，见上。

---

## 出口判定

**①③ 达成且出口成立**：① 使 `is_suspended` 成为挂起语义唯一事实来源、删并行集合与 4 处双维护点（REVIEW-dbc2384 n-1 闭环），公开语义不变、4 处状态迁移与旧集合完全双射、无残留死引用、门禁全绿；③ requirements 账本逐条审计更新属实、断言与代码/提交证据吻合、无夸大。3 项 Minor 均属文档一致性（THEORY-MAP 关联），建议委派方落地后视为闭环；本线出口不受阻。
