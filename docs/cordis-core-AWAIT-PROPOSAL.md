# core 步进扩展（Await）决策提案 —— wasm 桥异步化路径 B

**状态**：**草案——待决策授权**（本提案若授权，将首次打破「cordis-core 零改动」纪律——单次受控，见 §5）。
**依据**：`docs/cordis-wasm-WASMREMOTE-EXIT.md` §4 时序边界（guest `take` 无法在单次激活内等到异步 worker）；草案 v1.4 §2/§4（WasmRemote 完整 join 语义）；REVIEW-ERM-WASM-EXIT。
**配套**：C 探针详细计划 `docs/cordis-wasm-C-PROBE-PLAN.md`（并行决策输入）。

---

## 0. 一句话

给核心 `EffectIter` 加一个**挂起/恢复步（Await）**，让同步桥（wasm 等）能表达"未完成但等待外部就绪"，从而把 wasm 桥的 `take` 从「接口面」升级为「guest 完整 await 语义」。

## 1. 为什么 A（不动 core）不成立

- 核心 `use_component` 激活时 `execute` **一口气**跑完迭代器（`Step` 仅 `Yielded`/`Finished`，无"等待"形态，同循环内连续 next）；
- wasm 桥无法在激活后"暂停任务"——没有步进层面的"未完成但暂停"表达；
- 桥若自建驱动绕过 `execute` → 游离于核心激活/卸载/级联/逆累积语义之外（寄生旁路，维护与正确性都差）。
- **结论：完整 await 语义必然要求核心新增最小挂起/恢复机制** → 路径 A 是伪选项，B 是正解（通用：任何同步桥将来等待外部异步均可用）。

## 2. 方案 B 设计（最小单一机制）

| 面 | 设计方向（实现前细化） |
|---|---|
| 新步态 | `Step::Await(…就绪判据句柄…)`：迭代器**挂起**，fiber 停在 `Active`（未 Finished、**逆累积保留**），等待外部就绪 |
| 与 `Yielded` 的差别 | `Yielded`：立即可继续（execute 循环内继续）；`Await`：须外部就绪后才继续（execute 停止，等恢复） |
| fiber 状态 | `Active`（挂起于 Await）——不产生 Finished、不丢弃逆、不干扰退役级联（退役仍可中断挂起，逆照常 LIFO） |
| 恢复入口 | `Runtime::advance(fid)`（或 ctx 面）：组合线程在使用者判据满足后调用 → 恢复驱动 collect 后续 next 到下一 Await/Finished |
| 并发纪律 | **仍是单线程 push 模型**（ADR-0002）：advance 只在组合线程调用；不引入调度器/并发；外部异步（worker）仅通过"就绪回填 + advance"接回 |
| 判据形态 | v1：注入 `Remote` join 就绪（`JoinHandle::is_finished`/结果槽落位）；v2 可泛化条件回调 |

## 3. wasm 桥接线

- `WasmTaskIter` 的 `take` 未就绪 → 返回 `Step::Await`（判据 = 结果槽就绪）；
- worker 完成回填 Host 结果槽后，宿主（测试/插件宿主）调 `Runtime::advance(fid)` → 恢复 guest 后续步 → `take` 读到结果继续；
- `poll_remotes`（现有宿主导入口）语义升级为"advance 的触发源之一"（步骤：poll 回填 → 若就绪则 advance）。

## 4. 范围 / 代价 / 收益

- **范围（M2 级）**：核心新步态 + fiber 保留未完成迭代器 + `advance` API + 全部既有契约回归（注入依赖同步、级联、退役、逆 LIFO、事件/loader/hmr/async 不回归）+ wasm 桥接线 + 端到端（guest take 回真）。
- **打破纪律**：`cordis-core` 首次非文档改动 → **单次受控授权点**（本提案授权 → 仅本机制之改动允许进入 core；专项内不再扩面）。
- **论文偏离**：论文效应为确定性一次性执行；异步等待属**产品层扩展** → THEORY-MAP 标注（新行：Await 步与 §4 效应执行的偏离说明 + 理由：wasm 第三方插件需 await 宿主远程）。
- **收益**：wasm 桥 take 完整 join 语义（guest 以远端结果为输入持续运行的真实插件形态）；机制通用（其它同步桥同受益）。
- **风险**：既有契约复核面大；advance 误用（非组合线程/重复调用）需护栏（panic=bug 同既有纪律）。

## 5. 决策点（用户拍板）

1. **授权路径**：单选 B（动 core，本提案范围）—— 还是先 C 探针再定（推荐）；
2. **Await 步形态**：判据句柄（v1 join 就绪）——确认；
3. **advance API 面**：`Runtime::advance(fid)`——确认（或并入 `update_fiber` 复用名）；
4. **THEORY-MAP 偏离标注**：授权写入——确认。

## 6. 关联

C 探针（两阶段 guest，零 core）：若探针显示"两阶段拆分体验差→B 值得做"则授权本提案并起草 B 详细计划；若"两阶段勉强可用"→ B 降级待办。探针结论为本提案生效的**前置输入**。
