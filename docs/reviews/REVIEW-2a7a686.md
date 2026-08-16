# 代码审查报告：commit `2a7a686`（PR #11 WasmComponent 接入 cordis-core）

- **审查对象**：`2a7a686903e7465c7ea2d2d9a7b110af7f9831b3`（core 符号级动态绑定 `set_dyn`/`bind_value`/`unbind_value`；cordis-wasm 的 `WasmComponent: Component` 桥接、pending 转发、核心逆衔接）+ docs `bd017a7`
- **审查日期**：2026-08-16（仓库时区）
- **验证手段**：guest 构建后 `cargo test --workspace` **69 测试全绿**（含 wasm 3 个）；clippy `-D warnings` 干净
- **上游**：CI blocker（B1）由 PR #10 `73314ba` 引入，见 `REVIEW-73314ba.md`；本报告不含该条

---

## 🟡 建议修复（minor，无 major）

### m1. `set_dyn` 供给纪律按 realm 判定，与 `set` 的键判定不对称——isolate 场景误判
**位置**：`crates/cordis-core/src/context.rs`（`set_dyn` 内 `f.provide.contains(realm)`）；对比 `set` 的 `f.provide.contains(key)`

**事实**：`provide` 声明的是**键**符号；`set_dyn` 接收**realm**。未隔离时 realm=键 ✅；组件 ctx 经 `isolate(key, r)` 后，guest set 落在 r，`provide.contains(r)` = false → **合法写入被 panic 误杀**（或声明了与 realm 同名的键时放过未声明写入）。M1 起步测试无 isolate × wasm 覆盖。

### m2. `WasmTaskIter` 未做 `ρ` 解析——guest 键直接当 realm 传 `set_dyn`，隔离语义在 wasm 桥接缺失
**位置**：`crates/cordis-wasm/src/lib.rs`（`forward_pending` 内 `Symbol::intern(&set.key)` 直接作 realm）

**事实**：核心 `set`（typed）走 `resolve_realm`（`ρ(k)`）；wasm 桥接把 guest 的键**未经解析**传 `set_dyn`。若 fiber.ctx 有 isolate 映射，guest 的键应解析到 realm 而实际绑定在键本身。且 `resolve_realm` 是 `pub(crate)`——**cordis-wasm 作为外部 crate 无解析能力**，是真实桥接缺口。与 m1 同属"isolate × wasm 组合语义未定义"。

**建议（m1+m2 一并）**：明确 M1 阶段"isolate × wasm 未支持"并在文档声明（或 core 提供公开 realm 解析 API、`set_dyn` 内部解析）；补交叉测试或记录处置。

### m3. `HostInverse::drop` 退化为 no-op——PR #10 的槽清理在 PR #11 丢失
**位置**：`crates/cordis-wasm/src/lib.rs`（`impl HostInverse for Host` 的 `drop` 空实现）；`InstanceState::core_inverses` 与 `next_rep` 只增不减

**事实**：PR #10 的 `drop` 曾清槽（`self.inverses[rep] = None`）；PR #11 把核心逆迁入 `InstanceState` 后，`drop` 变 no-op（Host 拿不到 InstanceState）。正常路径经 `run_inverse` 的 `take` 释放内容 ✅，但槽位与 `next_rep` 空间不回缩，且 guest 释放 inverse 资源无任何宿主侧清理——防御性退化。建议至少在文档记录"rep 空间单调增长属已知边界"。

### m4. `inverse.run` 对 guest 调用静默 no-op——wit 接口语义与实现不符
**位置**：`crates/cordis-wasm/src/lib.rs`（`HostInverse::run` 空实现）；`wit/cordis.wit`（`run: func()`）

**事实**：wit 接口向 guest 导出了 `inverse.run`；guest 若调用（语义上"撤销我的绑定"）→ 宿主 no-op，guest 侧镜像不清理、核心绑定不撤销——静默失败。当前设计是"宿主驱动撤销"（run 供宿主侧调用），但接口层面 guest 可见可调。建议：wit 侧不给 guest 暴露 `run`（或改名为宿主专用并文档警示 guest 不得调用）。

---

## ⚪ 细节（nit）

1. **anyhow 引入 cordis-wasm**（`WasmComponent::load` 返回 `anyhow::Result`）——非 core，合理。

---

## 正面确认（架构决策正确的点）

- **`Host`（Send，WasiView 约束）与 `InstanceState`（非 Send，`Rc<RefCell>`）分离**——干净解决 `WasiView: Send` 与核心逆（捕获 `Rc<Context>`）的矛盾；借用边界（instance 不可变 + store/core_inverses 各自 RefCell）设计清晰。
- **pending + 转发模型**：guest 的 set 在 step 期间累积 pending，step 返回后统一转发核心 store——避免跨边界调用中途写核心状态的脏中间态，设计正确。
- **镜像先行/逆时清理**：guest `get` 在 step 内立即可读；`run_inverse` 执行核心 Disposer 后清理镜像 ✅。
- **双路径幂等延续**：核心逆进入累加器后与 `PushingIter` 共享 `StepGuard`——wasm 桥接与原生路径的撤销语义统一。
- **core 的 `set_dyn`/`bind_value`/`unbind_value`**：与 typed 版本平行（前置条件、notify、逆 = 撤销绑定语义一致），文档明确类型纪律（调用方约定 + 不匹配读报 `TypeMismatch`）。
- **测试覆盖**：`bridge_core.rs`（激活 → 核心 store 绑定 → σγ 计入 → retire 级联清除 + 镜像断言，2 测试）。

---

## 总结

- **必须修复**：无。
- **建议修复**：m1+m2（isolate × wasm 语义缺口——声明不支持或补齐解析）、m3（drop 槽清理退化记录）、m4（`inverse.run` guest 侧语义）。
- **nit**：1 可忽略。

**置信度**：高——m1/m2 由 `provide.contains(realm)` 与 `resolve_realm` 可见性直接核验；m3/m4 由 PR #10→#11 diff 对比确认。
