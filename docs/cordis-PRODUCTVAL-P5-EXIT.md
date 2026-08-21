# 产品验证线 P-5 出口判定 —— agent 插件产品 spike

**依据**：计划 `docs/cordis-PRODUCTVAL-P5-PLAN.md`；C 探针评估（真实 agent 场景刚需）；审查 REVIEW-P5AB（P5-1/P5-2 PASS）/ REVIEW-P53（P5-3 PASS）。
**判定日期**：2026-08-22。

## 1. 交付与验收

| 里程碑 | 内容 | 审查 |
|---|---|---|
| P5-1 | 原生 agent 插件 `examples/agent-plugin`（可运行）：events 订阅 user/msg→Arc 队列→async loop→spawn_remote LLM(worker)→join 回灌→bot/reply 事件→卸载 flush；修复 `RemoteRequest::boxed` 双重装箱陷阱 | REVIEW-P5AB PASS |
| P5-2 | wasm agent 多轮 take-await：guest 多轮 LLM 会话（首轮 Step(db)+后续 Wait/Await→take 累积→收尾 Done(probe)/失败轮 Done(probe_err)）经 `poll_and_advance` 驱动；3 轮累积 r0|r1|r2 + 失败轮终止 + O-6 每段 tid 隔离 | REVIEW-P5AB PASS |
| P5-3 | 全栈串联：events+async+wasm+Remote+Await 四层协作回路（user/msg→LLM(worker)→bot/reply→preseed 镜像注入 wasm/in→延迟挂载 wasm 多轮→probe；sync_injected 保留 preseed 桥接；guest 协作输入非注入依赖） | REVIEW-P53 PASS |

**门禁**：wasm 全套绿（lib 8 + 集成 13 含 full_stack/a2_e2e/wasm_agent/go）；workspace 无回归；clippy/fmt/doc 0；不改 core/wit 语义。

## 2. 评估（产品价值结论）

**C 探针结论验证**：真实多轮 agent 场景下 await 形态**舒适可用**——wasm 多轮 take-await 经 P-3 `poll_and_advance` 底座直接驱动（B 计划 + Await 生产化的价值实证）；原生 agent（events+async+Remote）回路直接、flush 干净。**产品价值确认：agent 插件（原生 + wasm 双形态 + 四层协作）可作模板**。

**缺陷/限制清单（记录）**：
- **协作输入通道**：事件→store 的原生通道受 listener `Send+Sync` 限制（不能捕获 Rc<Context>）——现以 Arc 队列 + 桥接层（main 循环）呈现；更直接形态（sync 订阅写 store）需事件层支持（记录为待办/未来）。
- **sync_injected preseed 保留（REVIEW-P53 M-1）**：核心无值时保留 preseed 对**注入依赖键**有残留偏差（提供者消失读到残留）——当前 guest 非注入形态无影响，已知边界。
- **LLM 模拟**：`reply:<echo>` 形态等价（无真实 LLM 依赖）——真实 LLM 接入只需替换 op 实现。

## 3. 出口判定

**P-5 完成**：agent 插件产品 spike（原生 + wasm 多轮 + 全栈串联）全部跑通、可运行示例 + 协作直证 + 评估报告（C 结论验证 + 缺陷清单）+ 审查闭环（0 Major 未决）。**产品验证线核心价值落地**。→ 下一线 **P-7（错误策略 O-1/O-4 联动）**，计划按纪律起草待用户确认。
