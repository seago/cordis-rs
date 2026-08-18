# `cordis-async` 协议草案（v1.1）评审意见

**日期**：2026-08-18 ｜ **评审对象**：`docs/cordis-async-protocol-draft.md` v1.1（工作文件，**未受版本管理**；评审意见以独立文档记录，不外改草案）
**评审性质**：草案设计评审（非实现审查）——对照本仓库已落地的 sync core/loader 语义（G1–G9 全部完成），评估 async 层草案的可行性与正确性。

---

## 0. 结论

**可行，无阻塞问题，建议按 Phase 0 推进。** 草案与现有 sync core 高度咬合；两阶段卸载（sync 快逆 + async settle）设计成立，I-3 顺序由 core 的 Thm 63 级联序免费获得。v1.1 已正面回应上轮全部四点（A/B/C/D），v1.1 引入的 B/C-6/F/G 均为经得起推敲的工程决策。新增评审点 H（§3.1 竞态）建议补记，不阻塞。

## 1. 上轮评审点回应核查（v1.0 → v1.1）

| 上轮点 | v1.1 处置 | 判定 |
|---|---|---|
| A. async 段读视图语义未定 | C：`get_cloned` = Running 期**活 store 读取 + 立即克隆释放借用**（Active ⟹ target==committed ⟹ 与提交视图等价）；teardown 窗口依赖改由**步创建处 Arc 捕获**（C-1'）——Thm 63 的 async 等价物 = 快照纪律；新增测试 7 | ✅ 干净解决，消掉类型擦除问题 |
| B. sync→async 桥未定义 | D：**O-6 政策**——v1 禁止 sync await async（请求 + oneshot/事件回灌，pending-set 通用形态）；`spawn_bridge` 逃生口（独立线程 runtime + 阻塞等待，注明不得触碰组合线程资源） | ✅ 政策决策诚实、约束明确 |
| C. `Fiber::target` pub(crate) | A：O-1 决议 core 只读访问器 `Fiber::target_view()`（增量、无语义变化）——与本仓库 `target: RefCell<Option<View>>` 完全兼容 | ✅ |
| D. Arc 值约定非强制 | C-1 + C-1' 快照强化 | ✅（仍为约定，诚实标注） |

## 2. v1.1 新引入点的确认（全部成立）

- **B（引用环消除）**：注册器闭包捕获 `Rc<AsyncFiberEntry>` 而非 `Rc<AsyncRuntime>`——消除 `AsyncRuntime → core → fibers → 闭包 → AsyncRuntime` 环，关停可 drop。**真实改进**。
- **C-6（逆契约）**：注册器逆 O(1)、不 await/panic/再借 RefCell——与 core 卸载路径"逆绝对干净（panic = 宿主 bug）"一致。
- **guard 双保险（A/E）**：`target_view()` 精确步界比较 + 取消 token 退场提示——闭环成立：target 变化 → refresh → unload → cancel → drive 步界退场 → 尾巴入 settle；`update(config)` 路径同构。
- **C-5（无裸 spawn）**：async 任务一律可追溯（尾巴/注册表句柄）——防野任务泄漏。
- **G（Remote trait）**：TokioRemote（v1，Send future 分池/spawn_blocking）+ WasmRemote（M1 宿主驱动协议接入）——与本仓库 M1 桥（宿主驱动 step）对齐。
- **新增测试 7/8/9**（快照纪律 / 代次与更新 / 无环关停）——直击修订点。

## 3. 评审点 H（新）：§3.1 `mark_running` 的竞态窗口

**问题**：`AsyncRegistrar` 的 drive 任务在 guard/取消下**正常返回 `Ok(disposer)`** 后调用 `entry.mark_running(fiber, disposer, cx.generation)`。但存在竞态窗口：**drive 完成时该 fiber 可能已被卸载**（cancel 已发、甚至 settle 已在排空该代尾巴）——此时 `mark_running` 会把一个已不在 Active 的 fiber 标记为 running；该 disposer 既未入 tail（未被 settle 收账），又不该进 running 表。

**提议处置**（写入 §3.1 或代次 §3.4，实现级）：
1. `mark_running` 前按 `generation` 比对：**代次已过期**（该 fiber 已进入卸载/新代激活）→ disposer **直接追加进当前 settle 的 tail 队列**（由正在排空的 settle 收账），而非 mark_running；
2. tail 收账本身幂等且有序（FIFO），过期 disposer 的入队位置按注册点先后自然正确；
3. 验收补一条：**"in-flight 步恰在卸载边界完成"**（测试 2 的极端化：drive 在 cancel 后、settle 开始前一步返回 Ok）→ 断言 disposer 被 settle 收账、无泄漏、无 double-run。

**阻塞性**：实现细节级；草案要求"程序正确性对齐"，故建议补记，不阻塞 Phase 0 骨架开工。

## 4. 次要建议（不阻塞）

- **O-2/O-3 保持开放**：settle 粒度、lifecycle observer hook 逃生口——正确列备。O-3 逃生口在本仓库**已有雏形**（`update_hook`/`retire_hook`，G1/G4），Phase 1 可低成本启用。
- **与现有代码的衔接备注**：
  - `Fiber::target_view()` 落地 = 在 `Fiber` 加 `pub fn target_view(&self) -> Option<View>`（borrow 克隆），零语义变化，与 reload 的 `guard_target` 同款。
  - **引用环借鉴**：本仓库 loader 的 `register_update_hook`/`register_retire_hook` 闭包捕获 `Rc<Loader>`，存在同类环（Loader→runtime→hook 闭包→Loader），目前未显式清 hook——草案 B 方案可作为后续小修参考（hook 闭包改捕获弱引用或条目级 Rc）。
  - 失败通道（自退役→disabled 写回→复活）与本仓库已实现的 G1（PR #29/#30：`update_fiber` 复活 + retire hook 写回）完全同构，可复用。

## 5. 建议动作

1. 补记评审点 H 到草案（或由其起草人采纳）；
2. Phase 0 开工：`cordis-async` crate 骨架 + I-1 单测 + 3 spike 门禁按草案 §9 执行；
3. 可选：将 `Fiber::target_view()` 与 loader hook 引用环小修并入既有工作线（约 1 commit）。
