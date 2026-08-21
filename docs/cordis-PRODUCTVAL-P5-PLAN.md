# 产品验证线 P-5 详细计划 —— 产品假设 spike：agent 插件

**依据**：C 探针评估（`docs/cordis-wasm-C-PROBE-EXIT.md`：真实 agent 场景——"调宿主 LLM → 拿回复 → 生成下一条"是**真实刚需**，两阶段不够、B 值得）；B 计划（core Await + wasm 完整 take-await 已打通）；P-3（Await 生产化：`poll_and_advance` 回路 = agent await 驱动底座）；P-4（go ABI 自动同步）。
**状态**：**草案——待开工指令**。
**保证**：Gate A/B 同前；commit 分 code/docs；不改 core/wit 语义；全回归绿；可运行示例 + 评估报告（产品价值结论）。

---

## 0. 目标

用一个**真实形状的 agent 插件**验证整套栈的产品强度：
1. **原生 agent 插件**：events（订阅用户消息）+ async（agent loop / 监听器）+ Remote（LLM 服务调用，worker 执行）→ 回复发布；卸载 flush；
2. **wasm agent 插件**：guest **多轮 submit/take**（LLM 会话：提交消息 → Await 等回复 → 再提交…）——验证 B 计划完整 take-await 在真实多轮形态下**好用**（C 探针结论的直接验证）；
3. **全栈串联**：一个宿主场景把 events + async + wasm + Remote + Await 全串（loader 挂 events provider + 原生 agent + wasm agent 协作回路）。

## 1. 形态与设计

### 1.1 原生 agent 插件（P5-1）
- **场景**：`user/msg` 事件（订阅）→ agent loop 经 `spawn_remote`（`RemoteRequest::from_future`——LLM 调用模拟：worker 上 async 计算"回复:你好"）→ join 回灌 → 发布 `bot/reply` 事件；卸载时 flush（会话落盘模拟）。
- **形态**：`examples/agent-plugin`（可运行 `cargo run`）或扩展 `async_combo`——独立示例更清晰（插件作者视角）。
- **验证**：订阅→调用→回灌→回复回路 + 卸载 flush + O-6（LLM 在 worker）。

### 1.2 wasm agent 插件（P5-2）
- **场景**：guest 多轮 LLM 会话——step: submit("llm", [msg]) → Wait（Await）→ take 回复 → set 结果（落盘/依赖者）→ 下一轮 submit…；宿主用 `poll_and_advance`（P-3）驱动。
- **形态**：扩展 `wasm-plugin-rust` 或新 guest（多轮迭代器）；端到端直证"多轮 await"（非单轮）。
- **验证**：多轮 submit/take 全通 + 错误轮（LLM 失败 → err → guest 分支）+ 卸载清理（挂起中退役）。

### 1.3 全栈串联（P5-3）
- **场景**：loader 挂 `EventsProvider` + 原生 agent（订阅 user/msg → LLM → bot/reply）+ wasm agent（经 store 键触发 LLM 会话）——协作回路：事件 → 原生 agent LLM → 事件 → wasm agent 远端 → 结果落盘断言。
- **形态**：集成测试（或扩展示例）+ 断言协作序。
- **验证**：四层全串（events + async + wasm + Remote + Await）单一场景直证。

### 1.4 评估报告（P5-4）
- **要点**：C 探针结论验证（agent await 形态在真实多轮下是否舒适——B/P-3 底座效果）；全栈体验/缺陷清单（如 LLM 模拟的形态限制）；产品价值结论（agent 插件模板的可用性）。
- **产出**：`docs/cordis-PRODUCTVAL-P5-EXIT.md`（评估 + 结论 + 建议）。

## 2. 分步

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| P5-1 | 原生 agent 插件（events+async+Remote 回路 + flush）可运行示例 | 开工指令 | 1–2 天 |
| P5-2 | wasm agent 多轮 take-await 端到端（含错误轮/卸载） | P5-1 | 1–2 天 |
| P5-3 | 全栈串联（四层协作回路直证） | P5-2 | 1 天 |
| P5-4 | 评估报告 + EXIT + 走查 | P5-3 | 0.5 天 |

全程约 3.5–5.5 天（含审查）。

## 3. 验收

- P5-1 示例可运行（`cargo run -p agent-plugin`）：订阅→LLM(worker)→回灌→回复 + 卸载 flush；
- P5-2 多轮 take-await 端到端（≥2 轮）+ 错误轮 + 挂起中退役清理；
- P5-3 全栈串联协作序断言（事件→LLM→事件→wasm 远端→结果落盘）；
- 全回归（workspace 无回归）+ 门禁绿；
- P5-4 评估报告（C 结论验证 + 产品价值判断）。

## 4. 决策点（开工前确认）

1. **原生 agent 形态**：独立示例 `examples/agent-plugin`（推荐——插件作者视角、可运行）vs 扩展 async_combo——确认独立示例；
2. **wasm agent 形态**：扩展 `wasm-plugin-rust`（多轮迭代器变体，guest 数不变）vs 新 guest crate——推荐扩展（避免新 crate 构建链）——确认；
3. **LLM 模拟**：Remote 操作 `llm(msg) -> "reply:<echo>"`（worker 执行）——确认（无真实 LLM 依赖，形态等价）。

## 5. 风险

- 多轮 take 的轮询/等待时序（poll_and_advance 回路循环终止）——以 P-3 模式 + 测试覆盖；
- 示例可运行性（examples 独立 workspace？—— agent-plugin 放 workspace member（同 im-bot）可 `cargo run -p`）。
- 不改 core/wit；guest 改动保持既有测试绿。

## 6. 纪律

Gate A/B 同前；commit 分 code/docs；可运行示例 + 评估报告为 P-5 独特交付（产品价值线核心）。
