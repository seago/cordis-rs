# `cordis-async` 协议草案（v1.1 / v1.2 / v1.3）评审意见

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

---

# 增补：v1.2 核对（2026-08-18 追审）

**对象**：草案 v1.2（361 行，采纳评审点 H 处置 + 上次次要建议落点）。**结论：H 处置正确且彻底，无新引入阻塞问题；1 个 shutdown-core 小点建议 §5 明示。**

## 6. 评审点 H 采纳方案核对（§3.1 + settle + 测试 10）

修订把记账统一为 **drive 与 tail 共享的 `Rc<RefCell<Option<AsyncDisposer>>>` 槽** + settle 恰一次 take。核验两点（草案未显式写、但成立）：

1. **slot 的 Rc 生命周期成立**：slot 被三处持有——drive 闭包（写）、注册器**逆闭包**（move 捕获，卸载时 `Rc::clone` 记账）、settle 的 tail 条目。drive 写完释放自身 Rc 后，**逆闭包仍持有**（它是 `once` 产出的 Disposer，随 fiber ctx 累加器存活到卸载）→ slot 在卸载时必然仍活着，`enqueue_tail(..., Rc::clone(&slot))` 安全；settle take 后 tail 条目 drop → 计数归零 → 无泄漏。**Rc 双持正确**。

2. **恰一次 / 无泄漏 / 失败路径正确**：settle 对每条尾巴 `await handle` → `slot.take()` → `d?.await()`——slot 为 `Option`（take 后 None）、drive 只写一次、Failed 分支 slot 恒空 → **无 double-run、无残留**；`await handle` 兜住"drive 在 cancel 后、settle 排空前才完成"的竞态窗口。

`mark_running` 退化为纯状态标记（代次不匹配跳过、不影响记账）——**竞态消除彻底**，收账唯一通道是 settle 的 take。测试 10 直证（含 Failed slot 留空 + shutdown 补账）。**采纳正确。**

## 7. 次要建议采纳核对（准确）

- O-3 雏形（`update_hook`/`retire_hook` = G1/G4）——准确（已落地）；
- loader 引用环登记 + B 方案作后继参照——准确（`register_update_hook`/`register_retire_hook` 闭包捕获 `Rc<Loader>` 确为同类环）；
- O-1 落地形态（`pub fn target_view(&self) -> Option<View>`，borrow 克隆）——与 `target: RefCell<Option<View>>` 兼容 ✓。

## 8. 新小点（非阻塞，建议 §5 明示）

**shutdown 与 core 侧的退役关系**：草案 "shutdown：cancel 全部 + settle 到静止"，对仍 Active 的 fiber 补 enqueue 保证收账——但未明说这些 fiber 的 **core 侧是否一同退役**。若只 cancel+settle 而 core fiber 仍 Active，则 `AsyncRuntime.is_quiet()`（async 视图）与 `core Runtime.is_quiet()` 会不一致。建议 §5 明示：shutdown = **core retire-all + settle**（或明确二者一致性契约）。

## 9. 总判定

草案自 v1.0 → v1.2 三轮迭代（A–G → H → 收尾）已闭合：**可行性、正确性、与现有代码衔接均无阻塞**。Phase 0 出口标准（§9：10 协议单测 + 3 spike）门槛合理，建议按草案推进。遗留唯一待明示项 = §8 的 shutdown-core 关系（属实现细节，不阻塞开工）。

---

# 增补：v1.3 核对（2026-08-18 终审）

**对象**：草案 v1.3（371 行，采纳上轮 §8 shutdown-core 小点 → 契约 C-7；slot Rc liveness 注记；测试 11）。**结论：采纳正确、无新问题；1 条 release 加固建议（不阻塞）。**

## 10. C-7（shutdown 语义）核对 —— 设计正确

- **分工清晰**：core 侧退役由**编排方先行**（facade retire-all / loader teardown）；`AsyncRuntime::shutdown` **只兜底** cancel 残留 + settle 到静止，不代做 core 退役——职责不再含糊。
- **双真断言**（`core.is_quiet() ∧ async.is_quiet()`）正确固化两视图一致性契约；未退役即关停 = 调用方违约（debug_assert 暴露）。
- **"退役不污染持久化配置"衔接准确**：与仓库 G1/PR #30 的 retire-hook 过滤语义（loader 驱动退役不写回 disabled）一致。
- **定位正确**：双真断言只放 shutdown（终态）而非 settle（中间态允许延迟卸载的合法 tails）——未误伤正常批次。

## 11. slot Rc liveness 注记（§3.1）核对

三处持有（drive 闭包 / 注册器逆闭包 / tail 条目）写成显式 liveness 链——与核验结论一致。

## 12. 测试 11 核对

三条：编排方先行退役 → 双真；未退役即关停 → debug_assert 捕获；退役路径不触发 disabled 写回（零污染）——直击 C-7 全部承诺。

## 13. 一条加固建议（不阻塞，可记开放问题或留 Phase 1）

**release 下双真断言折损**：草案用 `debug_assert`——release 构建中"调用方未退役即关停"会**静默**产生 async 已收账、core fibers 仍挂的不一致。对**关停路径**（终态、频率低、审计价值高）建议至少一处 `assert`（正式断言），或明确文档化"release 下编排方对退役负全责"。中间 settle 继续用 debug 不必改。

## 14. 总判定

v1.0 → v1.3 四轮迭代（A–G → H → shutdown-core → 收尾）**全部闭合**：评审点逐条被正确采纳，无遗留阻塞。草案具备完整正确性论证 + 11 协议单测 + 3 spike 出口标准，**建议按 §9 进入 Phase 0**（建 `cordis-async` crate 骨架 + I-1 单测 + spike）。遗留唯一建议项 = §13 release 断言加固（实现细节）。
