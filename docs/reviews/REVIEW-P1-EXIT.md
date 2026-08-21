# P-1 出口走查报告（wasm 逆表回收）

- **审查对象**：`docs/cordis-PRODUCTVAL-P1-EXIT.md` ↔ `crates/cordis-wasm/src/lib.rs`（P-1 实现）
- **审查日期**：2026-08-22
- **验证**：抽测 `cargo +1.97.0 test -p cordis-wasm --lib` = **8/8 通过**（全套含 go 由父会话已验）

---

## 总体结论：✅ PASS（P-1 出口成立）—— Minor 1 / Nit 2

出口文档与实现逐点一致，无夸大、无遗漏（除一处文档陈旧，见 Minor-1）。

## 核实要点（EXIT ↔ 实现）

| EXIT 声明 | 实现核实 | 结论 |
|---|---|---|
| `Host.inverse_free` 复用池 | lib.rs:147 字段 + 164 初始化 ✓ | ✅ |
| `set` 分配优先复用 | lib.rs:255 `inverse_free.pop().unwrap_or_else(next_rep++)` ✓ | ✅ |
| `run_inverse` 逆执行后入池 | lib.rs:311 `host.inverse_free.push(rep)`（先 `slot.take`、后 `task()`）✓ | ✅ |
| `drop` 保持 no-op（句柄销毁 ≠ 逆执行） | lib.rs:214 `Ok(())` ✓（稳妥：逆未执行则不复用 rep，避免句柄销毁但逆仍存活的回收风险） | ✅ |
| n-2 panic 留白注释 | lib.rs:305 注释 ✓（core 逆不应 panic，违反即宿主 bug） | ✅ |
| 借用无冲突 / 防重复入池 | `slot.take()` 幂等（once）✓ | ✅ |
| P1-2 有界性测试 | `host_inverse_free_reuse_bounds_rep_allocation`：1000 次 set→释放→set，断言 `r == first`（复用）+ `next_rep == 1`（有界恒定）✓ | ✅ |
| 门禁 | lib 8/8（本走查抽测）+ fork clippy/fmt/doc 0 + 不改 core（git 未触 cordis-core——父会话已验） | ✅ |

## 发现

### Minor-1（文档一致性，建议落地）：`HostInverse::drop` doc 注释陈旧

- `drop` 的 doc（lib.rs:211-213）仍写「槽位与 `next_rep` 空间**单调增长**属已知边界…**M2 提供回收**」——与 P-1 已回收的事实**矛盾**（回收现经 `run_inverse` 入池实现，lifecycle 内分配量有界）。
- 建议：`drop` doc 更新为「句柄销毁 ≠ 逆执行——回收经 `run_inverse`（P-1 产品验证线）将已执行逆的 rep 入 free list；`drop` 保持 no-op（防御，m3 移交）」。

### Nit-1（观察）：有界性测试走宿主模拟而非真实 wasm 链路

- 测试直接 `ContextHost::set` 分配 + 手动 `host.inverse_free.push` 模拟释放——未走真实 `run_inverse`。EXIT §2 m-1 已**诚实记录**（真实路径集成直证留 P-5 长驻组件场景）——非遗漏，记录合理。

### Nit-2（可忽略）：`run_inverse` 的 `task()` 在 `host.inverse_free.push` 前调用

- 顺序：`task()`（执行逆）→ `bindings.remove` → `inverse_free.push`——若 `task()` panic 则不入池（n-2 已注）。当前顺序正确（执行成功才回收）。

## 出口判定

**P-1 出口成立**——free list 机制 + 有界性直证 + 抽测 8/8 + 边界处置诚实（m3 已回收、m-1 记录、n-2 留白）。建议落地 Minor-1（drop doc 更新）后进入 **P-2**（双后端值类型下沉）。

> 说明：全套 wasm 回归（含 go 20s）+ clippy/fmt/doc 0 由父会话已验；本走查只抽测 `--lib`（8/8），全量可重跑确认。
