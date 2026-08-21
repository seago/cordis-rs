# 代码审查报告：产品验证线 P-3 · P3-1（commit `e8e9b8c`）

- **审查对象**：`e8e9b8c` — feat(core): P-3 P3-1 Await 生产化——Runtime.suspended 挂起集 + suspended_fibers() + advance_suspended(judge) + 单测
- **审查日期**：2026-08-22
- **范围**：`crates/cordis-core/src/runtime.rs`（suspended 集 + 批量）+ `docs/THEORY-MAP.md`（P-3 授权行）

---

## 总体结论

**✅ PASS WITH NITS** — 功能正确、登记/撤销完备、测试直证；**1 项 Minor（THEORY-MAP 编辑损坏）** 需修正，不阻塞放行 P3-2。

- **Major**：0
- **Minor**：1（文档损坏，非代码）
- **Nit**：2

## 通过项（核心核查）

1. **登记点完备**：① 激活挂起分支（`recovered_suspended.1` → `suspended.insert(fiber.id())`）；② `advance` 再挂起（`Err` 分支 → `suspen ded.insert(fid)`）——所有挂起进入点均登记 ✓
2. **撤销点完备**：① `advance` 取走（`resumable.take()` 后立即 `suspended.remove(&fid)`——恢复完成不残留、再挂起重登记）；② `unload` 收账残留逆后 `suspended.remove(&fiber.id())`（退役/卸载撤销）✓
3. **L-Raise 隔离**：激活失败走 `Err(payload)` 分支（不登记）——失败路径无漏撤 ✓；`advance` 未挂起 panic 在 `remove` 之前（panic 即中止，无错撤）✓
4. **`advance_suspended`**：快照遍历 + `judge` 过滤 + `advance`——单线程 push（ADR-0002 保持）；advance 对未挂起 panic=bug 纪律不变；judge 语义（就绪判定）设计正确（回填就绪 → advance → 若再挂起是新等待，judge 不立即可满足，无忙循环）✓
5. **一致性**：`suspended_fibers()` 与各 `fiber.is_suspended()` 一致（测试 `s == want` + 恢复后剩 b 断言）✓
6. **core 额度合规**：改动 = suspended 集（登记/撤销）+ 查询 + 批量——与 P3-1 授权范围（§1.1/§1.2）一致；零第三方（`HashSet` std）；添加性（既有测试 59/59 全绿）✓
7. **单测直证**：`suspended_set_tracks_and_batch_advances`——两 fiber 挂起登记、judge 只放行 a（b 保留）、全量恢复清空、挂起中退役撤销（独立 rt2）——登记/撤销/批量/退役全覆盖 ✓
8. **实测**：`cargo +1.97.0 test -p cordis-core --lib` = **59/59 通过**（含新测试）

## 发现

### Minor-1（文档损坏，须修）：THEORY-MAP 「B-A1」行被 P-3 授权行编辑截断

- **位置**：`docs/THEORY-MAP.md` 「已知偏差」表——原 `B-A1` 行被破坏：
  - 现某行变为 `| 2026-08-20 / B-A1 | **Step::Await 挂起/恢复`（**截断**——丢失 `= 论文 §4「确定性一次性效应」的产品级扩展（授权记录）**：核心效应原为有限步一次性执行...`）；
  - 而这些剩余内容（"= 论文...授权记录）：核心效应...到 `授权：用户 2026-08-20「授权 B」...` 整段）被**并接到新加入的 P-3 行尾部**（P-3 行末尾混入大段 B-A1 正文）。
- **影响**：纯文档表格损坏——B-A1 记录信息被截断/移置，B-A1 与 P-3 两行的语义混淆（P-3 行尾残留 `= 论文 §4...` 无上下文）。
- **建议**：修正 THEORY-MAP——恢复 B-A1 行完整原文（含 `**Step::Await 挂起/恢复 = 论文 §4（授权记录）**：...`），P-3 行保持纯 P-3 内容（不混 B-A1 尾巴）。

### Nit
- **N-1**：`advance_suspended` 快照后逐 `advance`——若同批 advance 中一个 fiber 的恢复**影响**另一纤维 target（罕见跨依赖挂起），后者的 judge 按快照时序判定（先判先 advance）——语义可接受，未记录文档（观察）。
- **N-2**：`suspended_fibers()` 返回 `Vec`（非排序）——调用方断言需排序（测试里已 `sort`）；快照语义（非迭代安全）未 doc 说明——建议 doc 注一句"快照，调用方排序/独立使用"。

## 结论

**P3-1 功能达成**（挂起集登记/撤销 + 查询 + 批量恢复 + 单测直证 + 59/59 + core 额度合规）；**建议修正 THEORY-MAP Minor-1 后**放行 **P3-2**（判据 v2 评估 + advance guard 复核）。Min-1 由父会话修正即可。
