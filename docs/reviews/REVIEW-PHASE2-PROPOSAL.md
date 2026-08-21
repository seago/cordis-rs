# Phase 2 决策提案评审记录

- **评审对象**：`docs/cordis-PHASE2-PROPOSAL.md`
- **评审日期**：2026-08-22
- **评审人**：independent-review-agent（对照仓库核实出处 + 遗漏扫描）

---

## 总体结论：**PASS WITH NITS** — 提案可交付用户决策（出处真实、逻辑清晰；1 项应补） 

- **Major**：0
- **Minor**：2（P-2 量级偏乐观；P-3 依赖说明）
- **Nit**：2（P-6 标识；§4 与 P-3 的 core 措辞）

---

## 1. 出处核实（全部命中，无凭空项）

| 候选 | 核实 | 判定 |
|---|---|---|
| **P-1** | `docs/reviews/REVIEW-2a7a686.md` m3 确述"`core_inverses` 与 `next_rep` 只增不减"，建议"文档记录已知边界" | ✅ 真实 |
| **P-2** | `crates/cordis-wasm/src/lib.rs` 40-45 行确述"原生→wasm 依赖方向（仅为一个值类型）"、THEORY-MAP PR#13 边界 | ✅ 真实 |
| **P-3** | `fiber.rs:158 is_suspended` 仅 `a2_e2e.rs` 测试消费（生产无消费点）；B 提案 §2 判据 v2；REVIEW-8589ca2 m-2 guard 弱化记录 | ✅ 真实 |
| **P-4** | `cordis_core_plugin/wit_bindings.go` 判别常量 0/1/2（`EffectStepStep/Done/Wait`）+ `wit_exports.go` 手写（REVIEW-6a714ca M-1） | ✅ 真实 |
| **P-5** | C 探针评估 `cordis-wasm-C-PROBE-EXIT.md`（真实刚需：持续 agent/多次往返/真失败）+ 各 EXIT "产品 spike" 取向 | ✅ 真实 |
| **P-6** | 明确标注"待用户输入"——诚实 | ✅ |

## 2. 遗漏扫描（建议补 1 项）

仓库里另有开放项此前判定"记录不阻塞"，但与 P-5（产品 spike）联动后**具实质价值**，建议补入：

- **P-7：错误策略线开放项定案**（`docs/cordis-rs-error-strategy-draft.md` / `CORDIS-ERROR-STRATEGY-REVIEW.md`）：
  - **O-1**：native 组件供给纪律越界写（`context.rs:245/306/347/401` panic ×4，Bug=作者义务）→ 升级为 ComponentFailure（待插件生态场景）——**安全深度**：目前仅 wasm 走失败模型，native 第三方插件越界仍 panic；P-5 引入真实 native 插件后该边界待定。
  - **O-4**：HMR 失败（reload 报告面 vs 事件通道）定案——P-5 agent 插件高频 HMR 触发真实失败时需明确。
  - 量级：O-1（1-2 天）+ O-4（0.5-1 天）。

**合理排除（无需补）**：O-2（告警节流——待高频 apply 真实场景，随 P-5 触发再定即可，可注明）；O-3（组内子条目失败聚合——**已落地** §4.5）；O-5（错误码/国际化——待 UI 场景）。这些不入提案正确，但建议在提案 §1 加一句"O-2/O-5 随 P-5 场景触发再评估"的衔接说明。

## 3. Minor / Nit

- **m-1（P-2 量级）**：`Value` 下沉到 core/独立 value crate 是**大改动**——涉及 wit 绑定重新生成（cordis-wasm）+ 原生/双后端互通测试重编 + 依赖面重构；保守估算应 **4–6 天**而非 2–4 天（提案写"含依赖面调整"但天数偏乐观）。
- **m-2（P-5 依赖）**：P-5 与 P-3 有依赖关系未明示——P-3 的"Await 生产化"（尤其挂起集轮询）会让 P-5 的 agent 插件对"多插件并发远端 + 挂起调度"更顺；建议在 §3 依赖图注明"P-5 依赖 P-3 可选（先 P-3 更顺）"。
- **n-1**：P-6 表"用户输入"可明确"若本轮无则跳过（无则不影响收口线）"。
- **n-2**：§4"不改 core 除非专项授权"与 P-3 ③（advance guard/判据 v2 可能动 core）——提案已在括号注明"单独立案如 B 计划先例"，措辞可再点明"P-3 若涉 core 改动须单独授权（含 THEORY-MAP 偏离标注）"。

## 4. 结论

**提案可交付用户决策**：6 项候选出处全部真实（无凭空）、定位/顺序/决策点逻辑清晰、诚实无夸大。**建议**：
1. 补 **P-7**（O-1 native 供给纪律升级 + O-4 HMR 报告面——与 P-5 产品 spike 联动，实质价值）；
2. m-1 修正 P-2 量级（4–6 天）；
3. m-2 依赖图注明 P-3→P-5 可选前置；
4. n-1/n-2 措辞微调 + §1 补"O-2/O-5 随 P-5 评估"衔接。

交由提案作者落地后即可交付用户拍板。
