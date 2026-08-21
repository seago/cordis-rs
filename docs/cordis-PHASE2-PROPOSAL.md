# Phase 2 决策提案（cordis-rs 下一阶段定义）

**状态**：**决策已拍板（2026-08-22）**——定位 C 组合；范围 P-1..P-7 全做；顺序 P-1 → P-2 → P-3 → P-4 → P-5 → P-7；P-6 待论文调研结论；**阶段命名：产品验证线（Product Validation Line，文档前缀 `cordis-*-PRODUCTVAL-*`）**。
**背景**：Phase 1 及后续工作线已全部收官——M0–M3 主线、Phase 0、Phase 1.1–1.4（events / async 门面 / Remote / DX）、错误策略线、M1 wasm 桥（WasmRemote）、B 计划（core Await + wasm 完整 take-await + A2b go 双语言恢复）。`docs/PLAN.md` 的 M 系列里程碑全部完成，此后无既定下一阶段定义。

---

## 0. 一句话

Phase 2 = 把仓库里**已记录、未闭环的剩余项**收口（技术/架构/维护债），并（可选）以**真实产品形态 spike** 验证整套栈——范围与顺序由本提案决策点定。

## 1. 候选范围（全部有出处、可验证）

| # | 候选 | 性质 | 出处 | 量级估算 |
|---|---|---|---|---|
| P-1 | **wasm 逆表回收（M2 级）**：`InstanceState.core_inverses` 槽位单调增长 → 提供回收（组件生命周期内 set 次数上界） | 技术债 | REVIEW-2a7a686 m3 | 1–2 天 |
| P-2 | **双后端值类型下沉**：wit `Value`（统一值类型）下沉到 core 或独立 value crate——消除"原生→wasm 依赖方向"（wasm 组件互通须依赖 cordis-wasm 仅为值类型） | 架构债 | cordis-wasm lib.rs / THEORY-MAP PR#13 边界 | 4–6 天（含 wit 重生成 + 双后端重编——REVIEW-PHASE2-PROPOSAL m-1 修正） |
| P-3 | **Await 生产化**：① `is_suspended` 消费（宿主统一挂起集轮询 / 批量 advance / 挂起状态上报）；② 判据形态 v2 泛化（条件回调）；③ advance guard（`target.is_some()`）与 `update_fiber` 更新路径交互复核 | B 计划收尾 | REVIEW-dbc2384 n-1 / B 提案 §2 / REVIEW-8589ca2 m-2 | 1–3 天 |
| P-4 | **go ABI 同步自动化**：`examples/wasm-plugin-go/wit_exports.go` 手写判别（0/1/2）随 wit 变更自动重生成（build.sh 化） | 维护债 | REVIEW-6a714ca M-1 / A2B-EXIT §4 | 0.5–1 天 |
| P-5 | **产品假设 spike**：真实 agent 插件形态（调宿主 LLM → 拿回复 → 生成下一条）跑通 events + async + Remote + wasm + Await 全栈；对齐 dsh 生态接线 | 产品价值 | C 探针评估（真实刚需场景）/ 多份 EXIT 取向 | 3–5 天 |
| P-6 | **论文后续章节落地**：若论文存在尚未映射的章节/定理（需用户指明范围；我不凭空列举） | — | 用户输入 | 待定 |
| P-7 | **错误策略开放项联动**：O-1（native 供给纪律越界写 → ComponentFailure 升级——当前仅 wasm 走失败模型）+ O-4（HMR 报告面定案）——与 P-5 产品 spike 联动后具实质价值（REVIEW-PHASE2-PROPOSAL 遗漏扫描）。合理排除并记录：O-2/O-3/O-5（O-3 已落地；O-2/O-5 待高频/UI 场景） | 安全/收口 | 错误策略草案 §10 + REVIEW | 1–3 天 |

## 2. 建议定位（二选一或组合）

- **A. 收口与加固线**（P-1..P-4）：把 Await/wasm/go 线的尾账清零——每项有明确验收、低风险、做完后 B 计划及 wasm 体系"无未决遗留"。
- **B. 产品价值线**（P-5）：真实 agent 插件验证全栈实战强度（事件订阅 + async 监听器 + Remote + wasm guest 远端 + Await 完整 take-await 的组合）。
- **C. 组合（推荐）**：先 P-1..P-4 收口（每项独立 Gate），再 P-5 产品 spike（用收口后的稳定基座）——P-6 按用户输入追加。

## 3. 依赖与顺序建议

```
P-1 ─┐
P-2 ─┤（互不依赖，可并行/任意序）
P-3 ─┤
P-4 ─┘
  → P-5（产品 spike 用收口后基座；**先 P-3 更顺**——Await 生产化（挂起集/状态上报）是 agent 插件 await 形态的底座，REVIEW-PHASE2-PROPOSAL m-2）
P-6/P-7（独立，随用户输入/优先级）
```

## 4. 纪律（沿用）

- 每项/里程碑 Gate A（fmt/clippy -D warnings/workspace 无回归）+ Gate B 独立审查（REVIEW-<hash>.md）。
- commit 分 code/docs；不改 core 除非专项授权——P-3 若需 core 改动须**单独立案 + THEORY-MAP 偏离标注**（如 B 计划先例，REVIEW-PHASE2-PROPOSAL nit-2）。
- 开工前每线出详细计划 + 决策确认（用户下达）。

## 5. 决策点（请拍板）

1. **定位**：A 收口 / B 产品 / C 组合（推荐）——还是其它？
2. **范围**：P-1..P-6 哪些纳入本阶段（可全选/部分）；
3. **顺序**：按 §3 建议或自定义；
4. **P-6**：论文是否还有要落地的章节（如有请指明；无则跳过）；
5. **命名**：本阶段就叫"Phase 2"，还是换名（如"收口线"/"产品验证线"）——纯标签，随你。

## 6. 下一步

选定后（默认 C + P-1..P-5 + 顺序 §3），我按线起草详细计划（P-1 逆表回收 → P-2 值类型下沉 → P-3 Await 生产化 → P-4 go 自动化 → P-5 产品 spike），逐线决策确认后开工。
