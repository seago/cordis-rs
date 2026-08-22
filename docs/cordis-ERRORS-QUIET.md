# cordis-rs 错误与安静语义（P1.4 DX）

基于冻结草案 v1.4（async §3.3 / 事件 §2.2）与已审查实现；语义不夸大。

## 1. async 失败通道（组件失败，非 panic）

| 阶段 | 机制 |
|---|---|
| 组件失败 | `AsyncStep::Failed(e)`（`AsyncFiberError`，载荷 `String`——P1.2 决策 O-4 保持）→ `drive` 先 LIFO await 恢复已完成步骤 → `Err(e)` |
| 进入终态 | `on_failed`：代次匹配 → `AsyncFiberState::Failed(e)`（**静止终态**） |
| 自退役 | `fiber.retire()` → core ctx 级联卸载依赖者 + loader `retire_hook` **写回条目 `disabled = true`**（G1 通道） |
| 收账 | 失败路径 slot **留空**（无 disposer），tail 正常 settle（恒可完成，I-4） |
| 复活 | 编排方重启用（loader 重载 `disabled=false` → 重建）→ **新代** spawn（条目换代，`Running{gen}`） |

- 失败 ≠ panic：组件要表达失败必须用 `Failed`；失败经值通道传播，不进入
  panic 通道。
- **panic 隔离**：async 任务内 panic = **宿主 bug**——JoinHandle 捕获、
  记录诊断，**不进失败通道、不级联**（邻组件不受影响），进程存活
  （草案 §3.3；协议测试 6 直证）。

## 2. 事件层错误语义

- 派发中 listener panic 传播（`panic = bug`，与 core 一致）；不做隔离收集
  （若 app 层需「一个插件崩溃不拖垮整轮派发」再评估 catch_unwind 版，
  草案 O-4）。
- 符号冲突（同名异载荷 / 同模式异 R / 跨模式异载荷）→ 订阅点 panic +
  类型名诊断（事件层义务纪律）。
- 派发 R 与订阅写定的 R 不符 → 派发 panic。

## 3. 安静语义（is_quiet / shutdown）

- **`AsyncRuntime::is_quiet()`** = **无待收尾巴** ∧ **无仍 `Active` 的
  async 组件**（`Failed` 视为静止——自退役后 fiber 即 `Inactive`）。
  - 与 core `Runtime::is_quiet`（`Active` = 无在途转换即静止）的差异：
    async 视图要求关停后无任何运行中 async 组件——**仅在
    `&& core.is_quiet()` 合取下**才是整体静止判定（P1.2 决策）。
- **`shutdown()`（契约 C-7）**：编排方**先行退役**（facade retire / loader
  teardown，退役不污染持久化配置）；本方法兜底（cancel + enqueue + settle，
  不代做 core 退役）；完成后断言 `core.is_quiet() ∧ async.is_quiet()` **双真**
  ——未退役即关停 = 违约（断言失败，测试 11 直证）。
- **`settle()`**：FIFO 排空尾巴队列（I-3 序免费来自 sync 级联）；`Failed`
  恒可 settle；drain 自再生 64 轮守卫（死锁诊断）。

## 3bis. HMR 失败呈现（P-7 O-4 定案）

- **双通道分工**（2026-08-22 定案）：**报告面 = 最近一次 apply 状态**
  （`Loader::report()`——回滚场景下失败尝试报告被回滚后的成功 apply 覆盖）；
  **失败通知 = 事件通道**（`loader/entry-failed`，经 `register_entry_failed_hook`
  → `cordis-events` 发射）——HMR reload 失败（回滚）时以事件通道为失败
  呈现（直证：`hmr_failure_reports_and_emits_entry_failed`）。

## 4. 速查

| 情形 | 行为 |
|---|---|
| `Failed(e)` 后 | 静止 + 自退役 disabled + 重启用复活 |
| async 任务 panic | 诊断 + 不级联 + 进程存活 |
| 事件 listener panic | 传播（panic=bug） |
| shutdown 未退役 | 双真断言失败（调用方违约） |
| `is_quiet()` | 无尾巴 ∧ 无 Active async 组件；Failed = 静止 |
