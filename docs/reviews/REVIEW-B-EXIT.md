# B 计划（core 步进扩展 Await）出口走查报告

- **审查对象**：`docs/cordis-core-AWAIT-EXIT.md`（commit b06e866 + 关联 A1/A2a/A3 提交链）
- **审查日期**：2026-08-20
- **审查人**：independent-review-agent（委派出口走查）
- **范围**：EXIT §1–5 与实现（`crates/cordis-core` / `crates/cordis-wasm` / `examples/wasm-plugin-rust*` / THEORY-MAP）逐条对证；A2b 遗留诚实性。

---

## 总体结论

✅ **PASS — B 计划主体出口成立**

- **major**：0
- **minor**：1（发现 A2，见下）
- **nit**：1（发现 A1，见下）

EXIT §1–3 与实现**逐条一致**（核心机制/添加性/挂起恢复/advance 护栏/unload 回收/wasm 接线/端到端直证/门禁声明均命中）；A2b（go）遗留**诚实记录**（ignore + 处置建议），不阻塞主体出口判定。

---

## 核实表（EXIT ↔ 实现）

| EXIT 声明 | 核实 |
|---|---|
| `Step::Await` + 添加性（execute 对 Await panic=走错路径） | effect.rs:46/66-68（clone panic 提示；`try_execute_with` :87/102 Err((iter,acc))）✅ |
| `try_execute_with` 带初始 acc 连续恢复 | effect.rs:87（`mut acc: Vec<Disposer>` 参数 + Err 返回累计 acc）✅ |
| fiber `resumable` + `is_suspended` | fiber.rs:138/158-159 ✅ |
| `Runtime::advance` 未挂起 panic=bug | runtime.rs:313-318（`未挂起于 Await` panic）✅；guard 注记 :321（m-2）✅ |
| unload 挂起残留逆 LIFO 归账 | runtime.rs:535 挂起分支 + unload 回收（父会话 A1 提交已含）✅ |
| PushingIter 透传 Await | context.rs:732-734 ✅ |
| wasm：wit `effect-step.inverse→option` | wit:41 ✅ |
| WasmTaskIter 在途 join→Await | lib.rs:697-706（`!done && !remote_joins.is_empty() → Step::Await`）✅ |
| a2_e2e 2 测试（guest 自取 + O-6 + err 通道） | a2_e2e.rs:32/105；**抽测 2/2 过**（0.68s）✅ |
| core 验收直证 | `test --lib advance` 抽测 2 passed（resume + unresumed-panic）✅ |
| go_guest 2 测试 #[ignore] | go_guest.rs:92/129（`#[ignore="A2b…"]`）✅ |
| THEORY-MAP B-A1 授权行 | THEORY-MAP:169 ✅ |

门禁/回归：clippy/fmt/doc 0、workspace 无回归由父会话已验；本轮抽查（a2_e2e / core advance）绿——未独立重跑全量 clippy（范围说明）。

---

## 发现

### Minor

**A-1（应补）**：EXIT §4 的 A2b root-cause 陈述「host 解码 go 的 effect-step 报 `invalid option discriminant`」——本轮 `--ignored` 强制复跑 go_guest 只呈现 `fiber.rs:56` 的 L-Raise（wasmtime 原始 decode 错误被 `FiberError` 包裹，panic 文本未透出），**确切判别错误需 A2b 开工时实证**。建议把 root cause 措辞改为「**疑似** go 侧 `option<resource>` 编码与 wasmtime 期望判别布局不符（待 A2b 实证）」——避免放空头背书。**不阻塞主体出口**。

### Nit

**N-1（应修）**：`docs/cordis-core-AWAIT-EXIT.md` 文件**末尾混入 2 行 shell 命令文本**（`echo EXIT-wrote && git add … && git commit …`，第 41–42 行）——heredoc 终止后命令尾随写入文档的杂质。交付文档应清理（删除末两行）。

---

## 出口判定

- B 计划**主体**（core Await + wasm guest 完整 take-await + 错误通道 + 文档归位 + 门禁绿 + 0 Major 未决）**出口成立**——EXIT 与实现一致、无夸大/无遗漏。
- **A2b（go ABI 收尾）**为既定遗留独立跟踪（rust 侧 B 目标已达成；go 属 M1 历史双语言门禁恢复项），不阻塞本出口——附 A-1（root-cause 措辞待实证）与 N-1（EXIT 末尾杂质清理）两项建议交由父会话落地。
