# 评估走查报告：wasm 桥两点式探针（C）C3 — `docs/cordis-wasm-C-PROBE-EXIT.md`

- **评审对象**：`docs/cordis-wasm-C-PROBE-EXIT.md`（C 探针评估报告与出口，2026-08-20）
- **评审日期**：2026-08-20
- **评审人**：independent-review-agent（C3 评估走查，静态对照 c_probe 实测 + REVIEW-C1/C2）
- **范围**：EXIT §1 可实现性 / §2 评估表 / §3 结论与建议 / §4 出口——对照 `crates/cordis-wasm/tests/c_probe.rs`（C1+C2 实测）与零 core 护栏

---

## 总体结论

✅ **PASS — 评估诚实、结论与证据匹配、无夸大；C 探针出口成立**（Major 0 / Minor 0 / Nit 3）

三点评估（① 一次性消费可用 ② 多次往返/真失败/生命周期不足 ③ 建议授权 B）与探针实测及 REVIEW-C1/C2 逐条一致。

---

## 核实过程（EXIT ↔ 实测 ↔ REVIEW）

### ① 一次性消费"可用且不算别扭"（EXIT §2 行 21，🟡）
- 实测：C2 — 阶段 1 激活（db 绑定 + `submit("echo",[Count7])`）→ `await_remote_value` 回填 `Count(14)` → `preseed_mirror("probe_in", Count14)` 回注镜像 → loader `rev+1` 重建阶段 2 → guest `get("probe_in")` 读回 → `probe_out=Count(15)` 断言通过 ✓（c_probe.rs `c2_two_phase_guest_consumes_backfilled_result`）。
- 与 REVIEW-C2「两阶段形态核查点全部达成（14+1=15）」一致 ✓。

### ② 三处不足（EXIT §2 行 22–24）
- **多次往返/持续 agent 🔴**：EXIT 判「每次往返都要回注 + rev bump + 分支重扫；状态显式落键、跨激活传递」——与 C2 实测形态（单次往返 + preseed + rev bump）一致；推论（多次往返会重复该编曲）为合理延伸，无夸大。
- **真远端失败无通道 🔴**：EXIT 行 23「`preseed_mirror` 只存 `Value` 成功形态——worker 失败无通道达镜像」——与 **REVIEW-C2 M-1**（失败 path 只覆盖"回注载荷形态不合法"≠"远端操作失败"）**逐字一致** ✅；`preseed_mirror`（lib.rs）签名 `(key, Value)` 无 `Result`/err 通道，事实成立。
- **生命周期 🟡**：EXIT 行 24「preseed 无逆、unload 不清、值跨卸载存活」——与 **REVIEW-C2 M-2**（手动插入无逆、unload 清理只处理 set 产物）**逐字一致** ✅。

### ③ 建议授权 B（EXIT §3 行 33）
- 结论「倾向于授权 B」理由 = C 暴露 ①②③（多次往返编排、真失败无通道、状态显式化）在真实 agent 插件形态必现 → B 给完整 take-await + 统一错误通道 + 免显式状态编排 + 通用。
- 与 `docs/cordis-core-AWAIT-PROPOSAL.md`（B 提案）目标一致（恢复驱动、错误通道、通用性）；两份文档互为衔接，无矛盾 ✓。
- 一次性质与持续形态的**判定分层**（🟡 捷径 vs 🔴 不足）有实证支撑（C2 直证前者、M-1/M-2 支撑后者），非拍脑袋 ✓。

### 零 core 护栏（EXIT §1 行 13）
- `git diff d3e07e2^..HEAD --stat -- crates/cordis-core` 为空（C 链仅 `cordis-wasm/{src,tests}` + `examples/wasm-plugin-rust`）；core 最近改动为 562c3a3/2753b1e（C 链之前的 wasm 专项 W3 文档文案）——**C 探针全程零 core** ✅。

### 表达准确性
- EXIT 行 12「失败兜 `probe_err` ✓」+ 行 23 明确"不覆盖远端操作失败"——两处并存**非矛盾**，是"形态失败 ✓ / 远端失败 ✗"的覆盖范围澄清（REVIEW-C2 M-1 建议落地），诚实 ✅。

---

## 发现（Nit，不阻塞）

- n-1（极低）：EXIT §2 行 22「明显反直觉/笨重」为**评估型主观**判定——有 M-1/M-2 佐证，属合理评估而非事实断言；可留。
- n-2（极低）：EXIT 未列 C 链测试计数（c_probe 2/2 + 回归数字）——篇幅精简可接受，走查已实测确认。
- n-3（极低）：EXIT §3 建议未把「C 保留为轻量捷径」的入口标注成文档（preseed 形态仅注释标注）——与 REVIEW-C2 M-2「不视为最终 API」一致，正式化（如 B）时自然收编，无需单独行动。

---

## 通过项（与证据匹配，无夸大）

- 可实现性两行（C1 回注核心 store / C2 全链路 + 失败兜 + 静止）与 c_probe 实测逐条吻合；
- 评估表三行+三列（可作捷径 / 明显不够 / 缺通道 / 正式化须定）方向正确；
- 结论「**倾向于授权 B** + C 保留轻量捷径 + B 前确认 B 提案 §5 决策点」合理且可执行；
- 出口流转（用户据报告 + B 提案决策）清晰。

---

## 结论与出口判定

**C3 评估走查 PASS**：探针回答清楚——「guest 必须以远端结果为输入继续」在**持续 agent / 多次往返 / 真失败**形态下是**真实刚需**（两阶段模拟暴露 ①②③），一次性"请求-消费"可作轻量捷径；**证据充分支持"倾向于授权 B（core 步进扩展 Await）"**。评估诚实（边界如实标注）、结论与证据匹配、无夸大。

→ **C 探针出口成立**；流转：用户据 `C-PROBE-EXIT` + `cordis-core-AWAIT-PROPOSAL` 决策（授权 B / 降级待办 / 再议）——父会话（主导 agent）待用户拍板；本走查不预设结论。
