# 代码审查报告：commit `ad857b3`（PR #5 fiber 生命周期状态机 + registry）

- **审查对象**：`ad857b315a053fb107f7d5ba05515e35c760a39f`（相对 `6403785`），9 文件，+1136/-102 行
- **审查日期**：2026-08-16（仓库时区）
- **核心代码**：`runtime.rs`（783 行新增：register/refresh/reload/unload/notify_fibers）、`fiber.rs`（Fiber 生产版 + 四状态 `FiberState`）、`context.rs`（fiber 归属 + 供给纪律 + use_component）、`store.rs`（绑定携带 provider）
- **验证手段**：`cargo test -p cordis-core` **51/51 全绿**（新增 12）；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 零警告

---

## 🔴 必须修复（major）

### M1. 隔离场景下通知匹配失配——`notify` 载荷不一致（用户键 vs realm）+ `notify_fibers` 按用户键匹配
**位置**：`runtime.rs`（`notify_fibers` 的 `f.inject.contains(*k)` 匹配）、`context.rs`（`set` 内 `ctx.notify(&[key])` 传**用户键**）、`runtime.rs`（`reload`/`unload` 内 `fiber.ctx.notify(&provided)` 传 **realm**，`provided_of` = `store.realms_with_provider`）

**事实推演**（未隔离场景 realm=键，全部测试通过掩盖了该缺陷）：
1. `Context::set` 的 notify 载荷 = 用户键（`Symbol::intern(K::SYMBOL)`）；
2. `Runtime::reload`/`unload` 的 notify 载荷 = **realm**（绑定表中 `bind(realm, ...)` 的第一个参数）；
3. `notify_fibers` 的匹配条件 `f.inject.contains(*k) && f.ctx.resolve_realm(*k) == ctx.resolve_realm(*k)`——第一个条件用**载荷符号直接比对注入键**。
4. 隔离场景（`isolate(key, realm)` 后 use 组件）：组件 `set::<K>` 解析到 realm `r ≠ key`；其激活/停用时 `notify(&provided)` 传 `[r]`；依赖者 `inject` 含用户键 `key`，`f.inject.contains(r)` = **false** → **依赖者收不到激活/停用通知，级联断裂**。
5. 多键同 realm（`isolate(k1, r)` + `isolate(k2, r)`）时该缺陷同样触发，且 realm→用户键方向不可逆（无法在载荷层修复）。

**为什么是问题**：isolate（Def 29，多租户/沙箱隔离）与 fiber 生命周期（PR #5 核心）的组合场景——正是 PR #4/#5 功能交接处的正确性缺陷。当前测试无交叉覆盖（PR #4 隔离测试止于 context 层，PR #5 依赖测试全部未隔离）。

**建议**：① 匹配逻辑改为 realm 语义（一行修复）：`keys.iter().any(|r| f.inject.iter().any(|k| f.ctx.resolve_realm(k) == ctx.resolve_realm(*r)))`；② 统一 notify 载荷为 realm（`set` 内也传 `&[realm]`），并在 `Reactor`/`notify` 文档说明载荷语义；③ 补交叉测试："隔离提供者 → 依赖者级联激活/停用"与"多键同 realm"。

---

## 🟡 建议修复（minor）

### m1. `notify` 事件载荷不统一（同一通道两种语义），用户反应器无法正确筛选
**位置**：`context.rs`（set → 用户键）/`runtime.rs`（reload/unload → realm）

与 M1 同源：`on_notify` 注册的用户反应器收到的 `keys` 有时是用户键、有时是 realm，文档未说明。M1 修复（统一 realm）时一并解决，并在 `Reactor` 类型文档注明。

### m2. `remove_fiber` 后 fiber 对象仍存活（注册回调闭包持有）——"幽灵 fiber"语义未文档化
**位置**：`runtime.rs`（`register` 中 `drop(ctx.effect(...))` 的注册回调）+ `remove_fiber`

Rc 图：父 ctx 累加器 → 注册回调逆闭包 → `Rc<Fiber>` → fiber.ctx → Runtime。`remove_fiber` 仅从 registry 移除条目，fiber 对象仍被父 ctx 累加器持有直至父 `dispose_all`。功能上安全（retire 幂等、refresh 对已移除 fiber 无操作——已核对该路径），但"移除后对象仍活着"未在文档说明。建议在 `remove_fiber` doc 注明。

### m3. 同步级联的栈深度无防护——深依赖链激活/停用是同步递归
**位置**：`runtime.rs`（notify → refresh → reload → apply 内 set → notify → …递归链）

依赖链深度 N 的激活/停用级联会产生 N 层嵌套调用栈（每层含 reload/unload 全流程）。同步核心的设计选择（文档已声明），但无深度限制或尾递归/工作队列兜底——超深链（如数百层）有栈溢出风险。建议在 runtime 模块文档记录该边界（PR #6 async 化自然缓解）。

### m4. `Fiber::state()` 返回 `Ref<'_, FiberState>` 的借用陷阱未文档化
**位置**：`fiber.rs`（`state()`）

与 PR #4 审查 m-A 同型：持有 `state()` 返回的 `Ref` 期间调用 `retire()`（→ `refresh` → `borrow_mut(state)`）会 RefCell panic。`store()` 已有借用警告，`state()` 无。建议补文档警告。

---

## ⚪ 细节（nit）

1. **`register` 中 `config.as_ref() as &dyn Any` 的 cast 冗余**（`&dyn Any` 转 `&dyn Any` 恒等）——无害。
2. **`mark_unloading` 双调用**（refresh→unload 路径与 unload 内部各一次）——幂等无害，可简化为 unload 内部单次。
3. **`mark_unloading` 中 `unwrap_or_default()`**——防御式，实际 committed 恒 Some（调用点均有保证），可接受。

---

## 正面确认（实现到位、语义正确的点）

- **Thm 63 真实引擎验证**：`withdrawal_cascade_disposes_dependents_first` 的 teardown 检查逆断言依赖可读——unload 顺序（先 mark_unloading 排除 σγ → 依赖者级联 → 本 fiber 逆执行）正确实现"依赖者先撤、提供者绑定保持到依赖者停用"。
- **Thm 64 验证**：`target_change_mid_reload_chains_unload`——guard 步界中断 + 惯性链（Reloading 期间 refresh 推迟、完成时按新目标自链），已应用步骤全部恢复。
- **Def 47 注册回调级联**：子组件注册为父 ctx 可逆效应（应用=refresh、逆=retire），`parent_unload_cascades_to_children` 验证父卸载级联退役子。
- **惯性状态机**：`Reloading`/`Unloading` 标记 + 完成时自链（§4.3.3），`is_quiet` 对转换在途返回 false——与 oracle 语义对应。
- **供给纪律执行期检查**（Def 43/48）：组件越界写入 panic，`set_outside_provision_panics` 覆盖；根绑定（provider=None）不参与 σγ（Def 45 式 (40)）语义正确。
- **双路径幂等延续**：fiber.dispose（各次激活 recover）+ ctx 累加器（每步 acc）共享 StepGuard，unload 双跑安全。
- **文档纪律**：THEORY-MAP 全面转生产版标注 + 4 条已知偏差（同步适配、根绑定、relied 同步等价、纪律检查），定理覆盖更新。

---

## 总结

- **必须修复**：M1（隔离×fiber 交叉场景的通知失配——匹配逻辑改 realm 语义 + 载荷统一 + 补交叉测试）。
- **建议修复**：m1（载荷统一并入 M1）、m2（幽灵 fiber 文档化）、m3（级联栈深度记录）、m4（state() 借用警告）。
- **nit**：1–3 可忽略。

**置信度**：高——M1 由调用链直接推演（notify 载荷三处来源、`notify_fibers` 匹配条件、`provided_of` 返回 realm 的代码事实均核验），且当前 51 个测试无一覆盖 isolate×fiber 组合场景，缺陷被掩盖的机理明确；唯一未做的是运行时复现（隔离组合测试，未写代码验证）。
