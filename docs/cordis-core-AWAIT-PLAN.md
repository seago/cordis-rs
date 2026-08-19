# core 步进扩展（Await）详细开发计划 —— wasm 桥异步化 路径 B

**授权**：用户 2026-08-20「授权 B」；决策提案 `docs/cordis-core-AWAIT-PROPOSAL.md` 已立项；C 探针（`docs/cordis-wasm-C-PROBE-EXIT.md`）结论支持（真实 agent/多次往返/真失败场景需要）。
**状态**：**草案——详细计划，待开工指令**（本次为 `cordis-core` **首次代码改动**（非文档）——成功计划需用户确认后开工）。
**保证**：每里程碑 Gate B 独立审查；Gate A（fmt + clippy -D warnings + 单测全绿 + workspace 无回归）；commit 分 code/docs；**core 改动额度 = 本计划 A1 之范围，专项内不再扩面**；THEORY-MAP 偏离标注一并落地。

---

## 0. 目标与非目标

**目标**：给核心 `EffectIter` 增加**挂起/恢复步（Await）**——fiber 在 Await 处停止执行（保留未完成迭代器与逆累积），组合线程在就绪后 `advance` 恢复。从而 wasm 桥的 guest `take` 获得完整 "await 远端结果" 语义（等同 TokioRemote join）。**添加性**：既有迭代器不返回 Await → 行为不变（零回归语义）。

**非目标**：不改 `Step::Yielded/Finished` 既有语义；不引入并发调度（仍单线程 push，ADR-0002）；不做通用 async 运行时。

## 1. 核心设计（A1 落地对象）

### 1.1 新步态
- `effect::Step` 增 `Await`（无载荷：纯挂起标记——恢复判据由调用方在 advance 前自行满足）。
- 向后兼容：既有 `once`/其他迭代器不产生 `Await` → execute 路径行为完全不变。

### 1.2 可恢复执行上下文
- fiber 增 `resumable: RefCell<Option<ResumableState>>`：`{ iter: Box<dyn EffectIter>, acc: Vec<Disposer> }`（跨 advance 保留）。
- 执行路径：新 `execute_resumable(iter, guard)`——循环 next：
  - `Yielded(d)` → `acc.push(d)` 继续；
  - `Finished(d)` → push + 完成（acc 进 fiber 逆累加器）；
  - **`Await` → 停止**，把 `(iter, acc)` 存 fiber.resumable，fiber 状态 `Active`（挂起）；
  - guard false → 停止（同既有中断语义）。
- 兼容性：`use_component`/`apply` 激活时先按既有 `execute` 语义跑——**探测迭代器是否产出 Await**：产出一个即切换可恢复路径（挂起 + 保留）；不产出 → 走既有完整执行（行为/逆顺序零变化）。

### 1.3 恢复入口
- `Runtime::advance(fid) -> Result<(), AdvanceError>`：组合线程在判据满足后调用。
  - fiber 无 resumable → `panic = bug`（调用方违约：未挂起不可 advance）——同既有调用纪律；
  - 有 resumable → 恢复 `execute_resumable`（继续到下一 Await/Finished）；
  - 卸载/退役中断挂起 → 逆照常 LIFO（resumable.drop 时逆补进累加器）。
- `AdvanceError` 非必需（可纯 panic=bug）——A1 定案用 panic=bug（既有风格）。

### 1.4 契约交互复核
- 注入依赖同步 / 级联 / 退役 / 逆 LIFO / loader / events / hmr / async ——全部不回归（A1 测试覆盖）。

## 2. 分步计划

### Step A1：core 步态 + 可恢复执行 + advance（A1）
- **目标**：`Step::Await` + resumable + `Runtime::advance` + THEORY-MAP 偏离标注。
- **任务**：
  1. `effect.rs`：`Step::Await` + `execute_resumable`（探测/挂起/恢复）；
  2. `fiber.rs`：`resumable` 字段 + 挂起状态 + 退役时残留逆归账；
  3. `runtime.rs`：`advance(fid)`；
  4. THEORY-MAP：新行（Await 步 vs 论文一次性效应执行的产品级扩展 + 授权记录）。
- **产出**：core 机制 + core 单测（Await 挂起/advance 恢复/无 Await 零变化/未挂起 advance=panic）。
- **验证**：core+loader+events+hmr+async 全回归（添加性零破坏）。

### Step A2：wasm 桥接线 + 端到端（A2）
- **目标**：guest `take` 未就绪 → `Step::Await` → worker 回填 → 宿主 `advance` 恢复 → take 读到继续。
- **任务**：
  1. `WasmTaskIter`：take 未就绪返回 `Await`（升级 `poll_remotes` 为"回填后 advance"的宿主辅助语义）；
  2. guest 恢复**多步 take**（DbTask：submit → Await → take 回填 → probe 落盘）；
  3. 端到端测试：guest 完整 await 远端结果（非宿主断言——guest 自己 take 到结果继续）+ 错误通道（worker Err → take err → guest 分支）+ O-6 隔离。
- **产出**：wasm 桥接线 + 端到端 + 既有回归。
- **验证**：wasm 全套绿（含新端到端）+ workspace 无回归。

### Step A3：C 探针归位 + 收尾（A3）
- **目标**：preseed/take 时序边界文档更新（take 完整语义解锁）；C 两阶段路径保留为轻量捷径（文档标注）；占位 doc 同步。
- **任务**：
  1. `WASMREMOTE-EXIT.md` §4 时序边界 → 已解锁记录（M2 落地）；
  2. `C-PROBE-EXIT.md` → 增补"B 落地后 C 定位：轻量捷径/探针归档"；
  3. wasm `WasmRemote` 占位 doc → Await 机制引用。
- **产出**：文档归位。
- **验证**：doc 0 告警。

### Step A4：出口（A4）
- **目标**：专项出口走查 + EXIT 文档（`docs/cordis-core-AWAIT-EXIT.md`）。
- **任务**：验收（core Await 挂起/恢复直证 + guest 完整 await 端到端 + 错误通道 + 全回归绿）逐条对证。
- **验证**：全部绿 + 出口走查。

## 3. 里程碑与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| A1 | core 机制 + 单测 + THEORY-MAP | 开工指令 | 1–2 天 |
| A2 | wasm 桥接线 + guest 完整 await 端到端 | A1 | 1–2 天 |
| A3 | C 归位 + 文档 | A2 | 0.5 天 |
| A4 | 出口走查 + EXIT | A3 | 0.5 天 |

全程约 3–5 天（含审查门禁；A1 是 core 首次改动，复核面大，可能 +1 天）。

## 4. 决策点（开工前确认）

1. **await 步无载荷**（恢复判据由调用方 advance 前满足）——确认；
2. **advance 协议**：`Runtime::advance(fid)` + 未挂起 = panic=bug（调用纪律同既有）——确认；
3. **执行路径**：先按既有 execute 探测（产 Await 才切可恢复）→ 零回归——确认；
4. **core 额度**：A1 范围一次性授权；未来 core 新改动再单独立项——确认。
5. **THEORY-MAP 偏离标注**（论文一次性效应 vs 产品异步 await）——授权写入。

## 5. 纪律

- core 改动额度=A1（其余 core 文件仅当 A1 需要）+ 门禁全绿 + 零回归；commit 分 code/docs；审查入库；**TODO 本专项内不扩面**。
