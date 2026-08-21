# A2b 出口走查报告（go ABI 收尾，修法① variant）

- **审查对象**：A2b（`docs/cordis-wasm-A2B-PLAN.md`，修法① wit 显式 variant）
- **审查日期**：2026-08-22
- **审查人**：independent-review-agent（A2b 出口走查）
- **依据**：G1+G2（REVIEW-c6feb44 PASS）/ G3（REVIEW-6a714ca PASS）；`docs/cordis-core-AWAIT-EXIT.md` §4；`README.md` 已知边界

---

## 总体结论

✅ **A2b 出口成立**（root cause 消除 + M1 双语言门禁恢复 + saracens 无缝全回归 + 审查闭环）——**0 Major / 1 Minor / 2 Nit**（Minor 为出口文档一致性，不阻塞出口）。

与走查 brief 的前提不同：**本走查基于当前工作区实证**（brief 声称的 c6feb44/G1+G2、6a714ca/G3、go_guest 恢复、wit variant 均已核实属实；`REVIEW-A2B-EXIT.md` 当时不存在——本报告即为缺失出口走查）。

## 核对（链路逐点，全部命中）

| 项 | 核实 |
|---|---|
| **wit variant** | `variant effect-step { step(inverse), done(inverse), wait }`（cordis.wit:38-46），注释标注"消除 `invalid option discriminant` 根因" |
| **宿主三分支** | `WasmTaskIter::next`（lib.rs:701-713）：`Some(EffectStep::Wait)` 且在途 join → `Step::Await`（core 挂起、advance 恢复）；`Some(Step(_))` → `Yielded`（逆走 pending 转发收账）；`Some(Done(_))` → `Finished` |
| **rust 4 guest** | 主 guest `EffectStep::Step(inverse)`（wasm-plugin-rust:75）等适配；ABI 全对齐 |
| **go 绑定重生成** | `wit_exports.go`：`EffectStepStep/Done/Wait uint8 = 0/1/2` 判别 + `EffectStep` struct + `Tag()/Step()/Done()`——**显式判别消除 option 坑** ✓ |
| **go_guest 恢复** | 2 测试去 `#[ignore]`；**抽测实跑绿（12.71s，go 工具链）**——M1 双语言门禁恢复 |
| **全套 wasm** | 抽测**全套 0 ignore 0 FAILED**（lib 7 + 集成 12 全绿，含 go_guest / a2_e2e / sandbox / dual_backend） |

## 发现

### Minor
- **M-1（文档一致性，不阻塞）**：`docs/cordis-core-AWAIT-EXIT.md` **§5 出口判定**仍写"**A2b（go ABI 收尾）为既定遗留**"——与 §4「A2b——已闭环（2026-08-22）」**矛盾**（同一文档 §4 已闭环、§5 仍称遗留）。建议：§5 同步为"含 A2b 全部闭环、双语言门禁恢复、无未决遗留"（B 计划全程闭环）。

### Nit
- **N-1**：EXIT §4 与 README 均注明 **go ABI 维护 = 手写同步**（`wit_exports.go` 判别 + 绑定重生成 + 重跑 go_guest，build.sh 未自动化）——属实且已记录（REVIEW-6a714ca M-1），作为**长期维护注意**确认（非新问题）。
- **N-2**：§5 若更新（M-1），建议同时把「判定日期」统一（§4 已闭环 2026-08-22，§5 仍 2026-08-20）——同类一致性问题。

## 已闭环确认（非发现）

- root cause（`option<resource>` → `invalid option discriminant`）由 variant **结构上消除** ✓
- M1 双语言门禁（Rust + Go guest 互通）恢复 ✓
- 全套门禁（fmt/clippy/doc/workspace）父会话已验 + wasm 抽测绿 ✓
- 审查闭环：G1+G2（c6feb44）+ G3（6a714ca）PASS ✓

## 出口判定

**A2b 出口成立**：B 计划（core Await + wasm 完整 take-await + go 双语言恢复）**全程闭环**，root cause 消除、零遗留未决（1 项 Minor 为出口文档 §5 措辞同步，建议父会话顺手修；2 项 Nit 为维护注意）。**建议父会话处理 M-1（EXIT §5 同步）+ 入库本报告**。

（作为被委派子代理仅写报告、未做任何 commit/文件修改。）
