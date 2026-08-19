# wasm 桥两点式探针（C）详细计划 —— 验证 guest-await 需求强度

**依据**：`docs/cordis-core-AWAIT-PROPOSAL.md` §6（C 为 B 的前置决策输入）；`docs/cordis-wasm-WASMREMOTE-EXIT.md` §4 时序边界。
**状态**：**待开工指令**（按纪律用户下达后开工）。
**核心约束**：**cordis-core 零改动**（C 探针全在 wasm/host 侧）；快、可运行、产出评估报告。

---

## 0. 探针要回答的问题

「guest 必须以**远端结果为输入**继续自己的逻辑」是不是真实刚需且**值得动 core（B）**？
- C 用**两阶段 guest**（零 core 改动）模拟"await"：阶段 1 提交并完成 → 宿主拿结果**经绑定回注** → 阶段 2 作为新 task 再驱动、guest 从注入端**读回结果**继续。
- 评估：作者体验（两阶段拆分是否反直觉 / 状态传递是否别扭）对照真实插件形态（如"调宿主 LLM → 拿回复 → 生成下一条"）。

## 1. 探针组件

| 件 | 说明 | 复用 |
|---|---|---|
| guest（改造 `examples/wasm-plugin-rust`） | 新增**两步任务**形态：阶段 1 步 = `submit("echo", params)` + 一步完成（逆 = 空）；阶段 2 步 = 从注入键 `probe_in` 读回结果（`get`）+ 继续产物（`set "probe_out"`） | 现有 guest + wit `remote`/`context` |
| 宿主驱动（新测试，仿 `load_guest`/`bridge_core`） | 激活阶段 1 → `poll_remotes` 等到结果 → **`ctx.set_dyn` 把结果写入注入键**（PR#12 注入同步：阶段 2 激活时镜像读到）→ revision bump 重 apply 驱动阶段 2 → 断言 `probe_out` | 注入同步机制（PR#12）+ 错误策略报告面工具 |
| 评估清单 | 两阶段拆分的体验缺陷记录（状态在哪、失败如何处理、TAKE 缺失的表现力缺口） | — |

## 2. 分步计划

### Step C1：回注机制打通（C1）
- **目标**：宿主侧把 worker 结果"回注为阶段 2 的输入绑定"（经核心 `set_dyn` 到注入键——PR#12 已支持注入同步）。
- **任务**：
  1. 确认注入同步路径（`WasmTaskIter::sync_injected`：阶段 2 激活 step 前把注入键值同步进镜像）；
  2. 宿主测试辅助：`await_result_and_note(comp, rep, key)`（poll 到结果 → `ctx.set_dyn(key, result Value)`）。
- **产出**：回注辅助 + 单点测试（结果 → 注入键 → 阶段 2 镜像可见）。
- **验证**：注入键在阶段 2 激活时 `get` 能读到回注值。

### Step C2：两阶段 guest + 端到端（C2）
- **目标**：真实两步插件形状（阶段 1 发起 → 阶段 2 消费结果）。
- **任务**：
  1. guest 两步任务：阶段 1 = submit + done；阶段 2 = `get("probe_in")` → 处理 → `set("probe_out")` + done；
  2. 端到端测试：阶段 1 激活 → 宿主 poll 回填 → 回注 `probe_in` → revision bump 阶段 2 激活 → 断言 `probe_out` 反映回注值（O-6 隔离仍持）；
  3. 失败路径演示：worker Err → 回注 err 标记 → 阶段 2 读到并走错误分支（不崩）。
- **产出**：端到端 + 失败路径 + 既有回归绿。
- **验证**：全套 wasm 测试绿（containing C 探针）。

### Step C3：评估报告（C3）
- **目标**：产出「guest-await 需求强度」结论 → 作为 B 决策输入。
- **任务**：
  1. 按评估清单记录两阶段拆分的体验缺陷（状态显式化、失败分支手动、多次往返的复杂度）；
  2. 对照真实插件形态判断"略难受但可用" vs "明显反直觉→B 值得"；
  3. 结论 + 建议（B 开工 / B 降级待办），写入 EXIT。
- **产出**：`docs/cordis-wasm-C-PROBE-EXIT.md`（评估 + 结论 + 建议），流转 B 决策。

## 3. 里程碑与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| C1 | 回注机制打通 | — | 0.5–1 天 |
| C2 | 两阶段 guest 端到端 | C1 | 1 天 |
| C3 | 评估报告 + EXIT | C2 | 0.5 天 |

全程约 2–3 天（含审查门禁）。

## 4. 门禁与纪律

- **core 零改动**（护栏：`git diff` 不触碰 `crates/cordis-core`；仅 wasm/host/tests）。
- Gate A：fmt/clippy -D warnings/wasm 全套绿/workspace 无回归；Gate B：每里程碑独立审查（C 探针为评估型，审查聚焦"探针有效性 + 评估诚实性"）。
- commit 分 code/docs；探针结论仅供 B 决策，不预设必须开工 B。

## 5. 出口

**标准**：两步插件端到端 + 失败路径 + 既有回归绿 + 评估报告（含对 B 的明确建议）→ C 探针结束，用户据报告决定是否授权 B（`docs/cordis-core-AWAIT-PROPOSAL.md` 决策点）。
