# Backlog 清理线计划 —— ① is_suspended 生产化消费 / ③ requirements 账本评审 / ② Await 判据 v2 启用

**依据**：产品验证线收官后遗留 backlog（用户指定顺序：**先①③，再②**）。

- ①：REVIEW-dbc2384 n-1 —— `Fiber::is_suspended` 仅测试/探针消费，生产语义留 A3/后续评估（P-3 已把挂起**集合**生产化：`suspended_fibers`/`advance_suspended`/`poll_and_advance`；本项补访问器本身的生产消费）。
- ③：`docs/cordis-rs-requirements.md`（用户文件，untracked，未评审）——仓库自身需求/状态账本，基线 commit `8752d0e` 已落后（当前 `bdb8905`），需逐条审计更新。
- ②：P-3 判据 v2 评估结论"不启用"（外部判据 + 显式批量驱动已覆盖；自动轮询有"谁在何时 poll"问题）——本线设计**显式评估**形态并启用（用户指示）。

---

## ① is_suspended 生产化消费（core 内部重构，公开语义不变）

**现状**：`Fiber::is_suspended()` = `resumable.borrow().is_some()`，生产代码零消费；挂起集为并行 `Runtime.suspended: HashSet<FiberId>`，在 4 处与 `resumable` 双维护（advance 撤/登记 326/346、reload 挂起登记 565、unload 撤销 603）——当前一致，但双事实源有漂移风险。

**方案（单一事实来源）**：
- `Runtime::suspended_fibers()` 改为**派生**：遍历 `fibers` 表 `filter(|f| f.is_suspended())`；
- 删除并行 `suspended` 集合字段 + 4 处维护点（公开语义不变——返回同一集合，P-3 测试不改）；
- `is_suspended()` 成为"挂起"语义唯一入口，生产路径真实消费（n-1 闭环）；
- 复杂度 O(k)→O(n)（n=fiber 数，组合内核规模小，P-3"枚举"语义本就 O(k)，可接受；文档注明）。

**决策**：
- ①-1：派生重构（删集合）——**同意**（若否：改为 suspended_fibers 内 filter 与集合断言一致的最小消费，保留集合）。
- ①-2：core 内部重构、无公开 API/语义变化 → 不需 THEORY-MAP 授权行；Gate B 走查照常。

## ③ requirements 账本评审与更新（纯文档）

**现状**：基线 `8752d0e`（2026-08-20）落后：A2b 已闭环（`6a714ca`/`9496e94`，go_guest 恢复绿无 ignore）、wasm 插件边界已全量完成（P-4 go ABI 同步自动化 `951709d`）、错误策略 O-1/O-4 已落地（P-7）、产品验证线 P-1..P-7 已收官、另有新 backlog 项。

**方案（逐条审计 + 更新账本）**：
- 逐条核对能力表/剩余工作/纪律/接口面断言 vs 当前代码与文档（commit 证据）；
- 更新：基线 commit、wasm 边界状态、async 层（补 P-3 挂起集生产化）、错误策略（补 O-1/O-4、ERROR-QUIET §3bis）、剩余工作表（A2b 行删除；O-items 保留；补"①③② 清理线"记录）、接口面清单核对（`is_suspended`/`suspended_fibers`/`advance_suspended`/`poll_and_advance`/版本化键等公开面）；
- 评审结论记录：文件内加"评审记录"节（日期/基线/结论）。

**决策**：
- ③-1：更新后**入库**（该文件自称"本仓库的需求清单与状态账本"；三个 protocol/error drafts 保持 untracked 不动）——**同意**？
- ③-2：审计发现的账本错误 → 修正并记录；无 → 如实记录。

## ② Await 判据 v2 启用（core 机制面扩展 + wasm 桥接线）

**现状**：`Step::Await` 无载荷；恢复判据在调用方（`advance_suspended(judge)`，wasm 桥 `poll_and_advance` = `poll_remotes` + `advance_suspended(|_| true)` 全驱）；P-3 结论：自动轮询不启用（谁在何时 poll）。

**方案（v2 = 挂起时自报判据 + 显式统一评估）**：
- `Step::Await(Option<Box<dyn Fn() -> bool>>)`：迭代器/翻译层在产生 Await 时自报就绪判据（None = 无自报，仍由宿主外部驱动——向后兼容，P-3 结论的"外部判据"保留）；
- 新 `Runtime::poll_ready()`：遍历挂起集，评估各 fiber 自报判据 → 满足即 `advance`（**组合线程在显式驱动点调用**——"谁在何时 poll"答案：组合线程、显式点、一次集中评估；无自主轮询/timer，ADR-0002 单线程 push 保持）；
- **消费者（真实）**：wasm 桥 `poll_and_advance` 改为 `poll_remotes` → `poll_ready()`；`WasmTaskIter` 生成 Await 时携带"本 fiber 远端结果槽就绪"判据（宿主翻译层构造，**wit/effect-step/ABI 零改动**——Await 载荷不经 wit）；
- `advance_suspended(judge)` 保留（既有调用方兼容）；advance guard（`target.is_some()`）不变；
- 判据纪律：纯检查不调度（文档注明）；判据随挂起上下文存亡（unload/advance 完成自然释放——闭包在 resumable 里）；
- **THEORY-MAP**：B-A1 行注记扩展（Step::Await 载荷 = A1 授权范围内形态演进）或新增一行（机制面扩展）——走查确认；
- 测试：判据满足/不满足两路 + 混合挂起集 + 与 advance_suspended 并存 + wasm a2_e2e 经 poll_ready 全绿。

**决策**：
- ②-1：判据形态 = Step 载荷（A）vs Runtime 登记表（B）——**推荐 A**（内聚：判据随挂起上下文存亡；wit 零改动；B 需额外登记/撤销 API 面与生命周期管理）。
- ②-2：`poll_ready` 为唯一新公开 API（advance_suspended 保留）——同意。
- ②-3：core 机制面扩展 → THEORY-MAP 授权注记 + Gate B 独立走查（含②-1 复核）。

---

## 执行与纪律

- 顺序：① → ③ → ②（用户指定；①③ 先行，② 后置——② 为独立小线，另走 Gate A/B）。
- 每项 Gate A（fmt/clippy -D warnings/test/doc 0）+ 完成后独立走查（subagent_fork 一次完成写盘）→ `docs/reviews/REVIEW-*.md` 入库。
- ①③ 合并一轮提交（code: ①；docs: ③）+ 走查；② 独立提交链 + EXIT + 走查。
- commit 分 code/docs；零第三方；全回归绿；最终 push origin main。
- ③ 涉及用户 untracked 文件入库：确认后执行（③-1）。
