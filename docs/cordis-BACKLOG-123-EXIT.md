# Backlog 清理线出口判定 —— ① is_suspended 生产化 / ③ requirements 账本评审 / ② Await 判据 v2

**依据**：`docs/cordis-BACKLOG-123-PLAN.md`（用户指定顺序：先①③，再②；决策：①派生重构 / ③入库 / ② Step 载荷 + poll_ready）。
**判定日期**：2026-08-22。

## 1. 交付与验收

### ① is_suspended 生产化消费（REVIEW-dbc2384 n-1 闭环）
- `Runtime::suspended_fibers()` 改为**派生**自 `Fiber::is_suspended()`（filter fibers 表）——`is_suspended` 进入生产路径，成为挂起语义唯一入口；
- 删除并行 `suspended: HashSet<FiberId>` 字段 + 4 处双维护点（advance 撤/登记、reload 挂起登记、unload 撤销）——**单一事实来源**，未来更新路径无漂移面；
- 公开语义不变（返回同一集合）；O(k)→O(n) 权衡已注记；core 61/61 无回归。

### ③ requirements 账本评审（用户 untracked 文件，已入库）
- 逐条审计：基线 `8752d0e`→`bdb8905`；A2b 由"既定遗留"更正为**已闭环**（go_guest 无 ignore + `build.sh` 第 0 步自动化）；wasm 边界"主体完成"→**完成**；补 P-3 挂起集生产化、P-6 版本化链接、P-7 O-1/O-4、backlog ① 记录；错误策略冻结条款补 P-7 扩展注记；公开面清单核对（含 `is_suspended`/`suspended_fibers`/`advance_suspended`/`poll_and_advance`/`key@version`）；
- 评审记录节（§六）入库；其余断言核对属实。

### ② Await 判据 v2 启用（core 机制面扩展 + wasm 桥接线）
- **判据载荷**：`Step::Await(Option<Box<dyn Fn() -> bool>>)`——迭代器/翻译层挂起时自报就绪判据，随挂起上下文存亡（advance 完成/unload 收账即释放）；`Await(None)` 保持外部判据语义（P-3 评估结论兼容）；
- **显式统一评估**：新 `Runtime::poll_ready()`——组合线程在显式驱动点一次集中评估所有挂起 fiber 的自报判据，满足即 advance。**"谁在何时 poll"定案**：组合线程、显式点、无自主轮询/timer（ADR-0002 单线程 push 保持）；
- **真实消费者（wasm 桥）**：`WasmTaskIter` 挂起时携带"本 fiber 在途远端 rep 全部落位"判据（每 fiber `inflight` 追踪 + 惰性剪枝）；`poll_and_advance` = `poll_remotes` → `poll_ready()`——相对 `advance_suspended(|_| true)` 全驱的**精确化**（未就绪 fiber 保持挂起不空转；同组件多 fiber 互不阻塞）；
- `advance_suspended(judge)` 保留（外部判据编排兼容）；advance guard（`target.is_some()`）不变；wit/effect-step/ABI **零改动**（Await 载荷在宿主翻译层构造）；
- **THEORY-MAP** backlog ② 授权行（B-A1/P-3 行后续：Step 载荷 + poll_ready，§4.3.3/Def 51）；
- 直证：core `poll_ready_advances_judge_satisfied_fibers`（判据未满足不动/部分满足只恢复就绪/无判据不受驱动/外部判据并存）+ a2_e2e/wasm_agent/go_guest 全绿（判据驱动回路端到端）。

## 2. 门禁

- Gate A：fmt 0 / clippy `-D warnings` 0 / `test --workspace` 全绿（core 62——新增 poll_ready 直证；a2_e2e 3/3、wasm_agent 2/2、go_guest 2/2 无 ignore）/ doc 0；
- Gate B：①③ 走查 `docs/reviews/REVIEW-123-13.md` PASS（3 Minor 全部落地：O(n) 权衡注记、THEORY-MAP 断言修正、THEORY-MAP P-3/P-7 行表格断裂修复）；② 走查 `docs/reviews/REVIEW-123-2.md`。

## 3. 出口判定

**Backlog 清理线（①③②）全部完成**：挂起语义单一事实来源 + 账本审计入库 + Await 判据 v2 启用（显式 poll_ready 定案"谁在何时 poll"、wasm 桥精确驱动）。框架侧剩余工作仅 O-items（待真实消费者）。后续取向按纪律由用户下达。
