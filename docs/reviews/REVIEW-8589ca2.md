# 代码审查报告：commit `8589ca2`（B 计划 A1 —— core 步进扩展 Await）

- **审查对象**：`8589ca24d23170e993b402192bca2545b62edb58` — `feat(core): B 计划 A1 核心步进扩展——Step::Await（添加性零回归）+ try_execute_with + Fiber.resumable + Runtime::advance + unload 挂起残留逆归账 + PushingIter 透传 + THEORY-MAP 授权偏离标注 + 单测`
- **审查日期**：2026-08-20
- **审查人**：independent-review-agent
- **审查范围**：`crates/cordis-core/{effect, fiber, context, runtime}.rs` + `docs/THEORY-MAP.md`，对照计划 `docs/cordis-core-AWAIT-PLAN.md` §1/§4 决策点 1–5（await 无载荷 / advance+panic=bug / 探测切换零回归 / core 额度 A1 一次授权 / THEORY-MAP 标注）。
- **验证手段**：静态阅读 + `cargo +1.97.0 test -p cordis-core`（58 lib + 集成全绿）；clippy/fmt/doc/workspace 由委托方本地验绿。

---

## 总体结论

✅ **通过（PASS WITH NITS）** — 放行 A2

- **major**：0
- **minor**：2（advance 恢复完成未再 notify 新提供集；advance guard `is_some` 与未来更新路径交互为观察项）
- **nit**：3

核心机制（`Step::Await` 添加性、`try_execute_with` 挂起/恢复带初始 acc、`resumable` 保留、`advance` + 违约 panic=bug、unload 挂起残留逆 LIFO 归账、PushingIter 透传、L-Raise 兼容）——逐项与计划一致，**添加性零回归成立**（激活路径 `try_execute_with` 的 `Ok` 分支与旧 `execute` 等值），THEORY-MAP 授权偏离标注到位。core **首次代码改动**在额度内（A1 投票面干净、不扩面）。2 项 Minor 为语义精化/观察，不阻塞 A2（wasm 桥接线将在 A2 真实验证这些交互）。

---

## 发现

### Major：无

### Minor

### m-1（建议）：advance 恢复完成后未再次 notify —— 恢复段新增绑定不广播

- **位置**：`Runtime::advance` 的 `Ok(disposer) => dispose.push` 分支（runtime.rs `advance`）。
- **问题**：激活时挂起分支已 `notify(&provided)`（当时提供的绑定 = K1）；advance 恢复段若产出**新的绑定集合**（挂起点之后 K2 形态），恢复完成只 `dispose.push`，**未再 notify** —— 依赖"恢复段新绑定"的依赖者不会被通知可激活（滞留到其它 notify）。
- **影响**：A2 场景（guest 恢复段 `context::set` 产物）依赖者感知可能延迟/缺失；当前 wasm 桥 producer 形态依赖者少，但**语义完整性**要求在"绑定集合变化"时广播。
- **建议**：advance `Ok` 分支恢复完成后再 notify（与激活 notify 同构；幂等——绑定的变化才触发，未变 NOP）；A2 端到端顺带断言"恢复段产出的绑定被依赖者感知"。

### m-2（观察）：advance guard = `target.is_some()` 弱于激活 guard —— 与未来更新路径的交互待复核

- **位置**：advance 的 guard 构造（`move || f.target.borrow().is_some()`）。
- **现状安全**：核心更新路径（refresh/重建）经由 unload（先 `mark_unloading` + resumable 残留逆归账）再新激活——advance 只作用于"仍挂起且未 unload"的 fiber，target 未变。
- **观察**：若未来出现"target 原地替换而不经 unload"的路径，`is_some` 可能放行旧 resumable 跑向新目标。A2/A3 在 wasm 桥接入 + update 场景复核；**当前不构成缺陷**。

### Nit

- **n-1**：`try_execute_with` 的 guard break 注释建议显式点出"acc（含 init_acc）折叠为 LIFO"已写，可再补"guard false 与 Await 的区分"一句（可读性）。
- **n-2**：commit message 偏长（惯例，非阻塞）。
- **n-3**：advance 的 `panic` 消息两处（未知 fiber / 未挂起）均清晰，建议 A2 单测补"advance 后 resumable 消费、state 仍 Active"的显式断言（现断言有 store contains + resumable None）。

---

## 通过项（逐条确认）

- **Step::Await 添加性**：effect.rs 新变体；同步 `execute` 对 Await panic（走错路径提示）——既有迭代器不产 Await → 零变化 ✓。
- **try_execute_with**：`Yielded` push 继续 / `Finished` 折叠 / `Await` → `Err((iter, acc))`（acc 含 init_acc，可连续恢复）/ guard break → `Ok` 折叠（含历史逆）——挂起/恢复闭环 ✓；单测（a→挂起→z→LIFO [z,a]）直证 ✓。
- **激活路径**：`register` 从 `execute` 改 `try_execute_with(iter, guard, Vec::new())`——`Ok(Ok)` 与旧 execute 等值（同循环同 guard 同折叠）✓；`Ok(Err)` = Await 挂起 → resumable 存 + state=Active{view=committed} + notify + return ✓；`Err(payload)` = L-Raise（FiberError→失败路径）**不误入挂起分支** ✓。
- **挂起语义**：挂起时第一步副作用（K1）已发生、fiber Active、resumable 保留 acc——单测断言（store k1、resumable Some、Active）✓；与论文"效应确定性一次性"的偏离经 THEORY-MAP 授权行显式标注（产品层扩展、ADR-0002 单线程 push 保持）✓。
- **advance**：take resumable（None→panic=bug，消息与测试匹配）✓；guard=target.is_some ✓；Ok→dispose.push（LIFO 跨激活保持）；Err→resumable 重放（可再 advance）✓；退役中（target None）→ guard break→Ok→残留逆收账 ✓。
- **unload 回收**：resumable acc append dispose → 下方 drain(..).rev() LIFO 执行——与完整执行折叠序一致 ✓；advance 已完成则 resumable None → 不重复收账 ✓（无 double）。
- **PushingIter 透传**：ctx.effect 记录迭代器产 Await 透传（push_step 延迟到 Finished——挂起期间未完成无记录，正确）✓；上层 try_execute_with 挂起（非炸）✓。
- **THEORY-MAP**：B-A1 行记录（扩展、授权、ADR-0002 保持、执行语义零变化声明）✓。
- **测试**：core 58 + 集成全绿；新增 effect 单测 + advance_resumes_suspended_fiber（K1/K2 LIFO 退役）+ advance_unresumed_panics（违约）✓；添加性（既有测试全过无需改）✓。

---

## 结论

A1（core 步进扩展 —— `Step::Await` + resumable + advance + unload 回收 + 透传 + 授权标注）与 `docs/cordis-core-AWAIT-PLAN.md` §1 一致，机制闭环、添加性零回归、core 首次代码改动在 A1 额度内且未扩面。单测直证挂起/恢复/LIFO/违约。**建议放行 A2**（wasm 桥接线：`WasmTaskIter` take 未就绪 → `Step::Await` + 宿主回填 → `advance` + guest 多步 take 端到端）；m-1（恢复完成 notify）建议在 A2 接线时一并落地并断言，m-2（advance guard 与更新路径）在 A2/A3 复核。
