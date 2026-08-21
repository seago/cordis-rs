# P-2 出口走查报告（产品验证线 · 双后端值类型下沉）

- **走查对象**：`docs/cordis-PRODUCTVAL-P2-EXIT.md` ↔ 实现（commit 3a3591c + 360e7e4）
- **走查日期**：2026-08-22
- **结论**：✅ **PASS（0 Major / 0 Minor / 0 Nit）——P-2 出口成立**

## 逐点核实（EXIT ↔ 实现 ↔ 实测）

| 项 | 核实 |
|---|---|
| **cordis-value crate** | `crates/cordis-value` 存在——`Value { Flag/Count/Offset/Text/Blob }` 枚举，`deny(missing_docs)`、零第三方、零 `cordis-core` 依赖（doc 明示）✅ |
| **桥接转换层（方案 C）** | `cordis-wasm` 内部统一 `cordis_value::Value`；trait 边界 `to_cv`/`from_cv`（lib.rs:99/111，`get/set/submit/take` 边界）；remote `submit` 参数 `map(to_cv)` ✅ |
| **双向互通** | dual_backend 2 测试：wasm→native `native(wasm-pg)` / native→wasm `derived(native-pg)`——**实测 `cargo test -p cordis-wasm --test dual_backend` = 2/2 通过（0.67s）** ✅ |
| **依赖方向达成** | 原生经 `cordis-value` 互通（无需依赖 cordis-wasm）；`cordis-core` 零改动 ✅ |
| **THEORY-MAP** | PR#13 m2 行（THEORY-MAP:131）→ "已闭环（P-2 下沉，方案 C，转换层为最终形态——REVIEW-3a3591c M-1）" ✅ |
| **计划决策点对齐** | §4 决策点执行结论：独立 crate（默认）+ **方案 C**（wit external 映射因工具链限制不可行——spike 已定稿）→ 日志/EXIT 一致 ✅ |
| **门禁声明** | wasm 全套绿/linedup 无回归由父会话已验；`cardis-core` 零改动 ✓ |

## 说明（会话前提核对）

- 走查 brief 所引 commit 与审查（`3a3591c` / `360e7e4` / `REVIEW-3a3591c`）在仓库 `git log` 中**真实存在**（P-2 已实施，早轮自动推进的成果）——非虚构；走查基于现行权威状态执行。
- 未发现与 EXIT 矛盾或夸大的内容；N-1（转换开销观察）已记录。

## 出口判定

**P-2 出口成立**。值类型下沉独立 crate + 双后端双向互通（跨类型翻译边界消除）+ 依赖方向修复 + 门禁绿 + THEORY-MAP 闭环 + 审查闭环（REVIEW-3a3591c PASS）。→ 下一线 **P-3（Await 生产化）**，按纪律起草计划待用户确认。
