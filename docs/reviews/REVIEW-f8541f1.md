# 代码审查报告：commit `f8541f1`（M1.3 订阅即效应集成，Phase 1）

- **审查对象**：`f8541f13bdb90f56f470a72f8d2055c27a879f01` — `feat(events): M1.3 订阅即效应集成——subscribe/subscribe_waterfall/subscribe_serial/subscribe_bail（ctx.effect 落账 + 卸载自动退订）+ 验收 #3 与 #2 双路径 armed（Phase 1）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show f8541f1`（`crates/cordis-events/src/lib.rs` +82 / `crates/cordis-events/tests/events.rs` +104），对照 `docs/cordis-events-protocol-draft.md` v0.3.1（冻结）§3.1 与执行计划 `docs/cordis-events-PHASE1-PLAN.md`（M1.3，验收 #3/#2）。
- **验证手段**：静态阅读 + 实际运行 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 test -p cordis-events`（12/12）、`clippy --all-targets -- -D warnings`、`fmt --check`、`doc --no-deps`（0 告警）。

**改动统计**：2 文件，+186。
- `lib.rs` +82：`subscribe`/`subscribe_waterfall`/`subscribe_serial`/`subscribe_bail` 四个便捷订阅（取总线克隆 Arc、释放借用 → `ctx.effect` 注册 → 返回幂等 disposer）；`on_serial` doc 补 n-3'' 注记（serial 与 bail 的 R 相互独立）。
- `tests/events.rs` +104：`m13` 模块——`Subscriber` 组件（apply 内 subscribe + 步逆 = 订阅 disposer）、`#3 卸载自动退订`（emit 送达 → retire → 不再到达）、`#2 双路径 armed`（手动 disposer + ctx.dispose_all 共享 armed，不 double）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：3（四个便捷订阅中仅 `subscribe`（emit）有 m13 集成直证，其余三者同构无专属测试；`d as Disposer` 冗余强转；`subscribe*` 的 ctx 参数语义未言明「ctx 应为订阅所属上下文」）

M1.3 订阅即效应实现与草案 v0.3.1 §3.1 一致：订阅经 `ctx.effect` 落账（步逆进 fiber ctx 累加器，随卸载自动退订）、返回幂等 disposer（与累加器逆共享步 armed，双路径至多一次）、取总线克隆即释放借用、`R` 带上 `Send+Sync` 上界（Minor-1 修订延续）。验收 #3（fiber 退役自动退订）与 #2（手动 + dispose_all 双路径 armed 不 double）均被 m13 测试直证，无脆弱点。工程门禁全绿（test 12/12、clippy -D warnings、fmt、doc 0 告警）。可放行进入下一里程碑 **M1.4（waterfall/重入/空集精化，验收 #4/#7）**。

---

## 发现

### Major：无

### Minor：无

未发现必须修复才可合入的问题。

### Nit-1（低）：四个便捷订阅中仅 `subscribe`（emit）有 m13 集成直证，`subscribe_waterfall`/`subscribe_serial`/`subscribe_bail` 无专属自动退订测试

- **位置**：`tests/events.rs` `m13`——`#3` 与 `#2` 均只用 `subscribe::<Done>`（emit 形态）；waterfall/serial/bail 三个便捷入口无 m13 直证。
- **问题**：四个 `subscribe*` 实现完全同构（取总线 → `ctx.effect(once(bus.on_xxx))` → 返回 disposer），差异仅在 `bus.on_waterfall/on_serial/on_bail`（其语义由 M1.2 bus 层测试覆盖）。故风险很低、不构成缺陷；但「便捷入口 + 自动退订」的组合对 waterfall/serial/bail 未做端到端直证。
- **草案/计划依据**：计划 M1.3 =「订阅即效应集成（…验收 #3）」；草案 §3.1 `subscribe_waterfall`/`subscribe_reply` 为后续便捷入口。
- **建议**（可选，M1.4 或后续补）：补一条 `subscribe_serial`（或 `subscribe_bail`）的「挂载 → emit/派发收集 → retire → 不再可达」集成直证，把「便捷入口 + 自动退订」覆盖到全部四种模式。

### Nit-2（低）：`let d = bus.on::<P>(listener); d as Disposer` 中 `as Disposer` 冗余

- **位置**：`lib.rs` `subscribe`/`subscribe_waterfall`/`subscribe_serial`/`subscribe_bail` 的 once 回调（`d as Disposer` × 4）。
- **问题**：`bus.on::<P>(...)` 返回类型即 `Disposer`（`--all-targets` 无类型告警），`d as Disposer` 为恒等强转、无实际作用。
- **建议**：直接 `bus.on::<P>(listener)` 或去掉 `d` 绑定（`Box::new(cordis_core::once(Box::new(move || bus.on::<P>(listener))))`）。纯可读性清理。

### Nit-3（低）：`subscribe*` 的 `ctx` 参数语义未言明「ctx 应为订阅所属上下文（其卸载触发自动退订）」

- **位置**：`lib.rs` `subscribe` 等四个 doc「订阅随 fiber 卸载自动退订」。
- **问题**：自动退订由「`ctx.effect` 把步逆推入 **ctx 的累加器**」实现——若调用方传入**根 ctx**（如 #2 测试所示），订阅落根 ctx 累加器，**不随任何 fiber 卸载**（根 ctx 由 app 显式 dispose_all）。doc 与草案均未言明此分界，读者可能误以为「只要用 subscribe 就自动退订」。
- **草案/计划依据**：草案 §3.1（`subscribe(ctx, ...)`）与 §4.3（scope 总线 = realm 隔离实例）隐含「ctx 决定退订归属」；E-3（经 ctx.effect 落账）为语义基础。
- **建议**（可选，doc 精确性）：在 subscribe* doc 补一句「`ctx` 应为订阅所属的上下文（组件 apply 的 fiber ctx 或 scope 总线 ctx）——其卸载触发自动退订；传入根 ctx 的订阅由 app 层负责 dispose_all」。

---

## 核查通过项（逐条确认）

- **订阅即效应（E-3 / 草案 §3.1）—核实通过**：`subscribe` 先 `ctx.get::<EventsKey>()` 克隆 `Arc<EventBus>`（`Arc::clone(&*ref)`，Ref 语句级 drop → 借用释放），再 `ctx.effect(move || once(...))`——ctx.effect 立即 execute 回调 → `bus.on` 订阅立即生效，订阅步逆（退订 disposer）经 core `PushingIter` 推入 ctx 累加器（`dispose_all` 时执行 = 自动退订）；返回的复合 disposer 与累加器逆**共享步 armed**（core `Context::effect` 保证"至多生效一次"）。E-3 的分界（裸订阅不经 ctx 不自动退订）由实现结构天然满足。
- **双路径 armed（#2 / REVIEW-a0963ab nit-2）—核实通过**：`d()`（手动）= ctx.effect 复合逆的 armed 执行 → 置 false + 执行订阅步逆（退订）；随后 `ctx.dispose_all()` 的累加器逆经同一 armed 跳过——**不 double、不 panic**。m13 `manually_dispose_and_ctx_dispose_all_share_armed` 直证：`d() → dispose_all → emit(2) 不达`。bus.on 自身又是幂等 disposer（M1.2），下层再加保险。
- **#3 卸载自动退订—核实通过**：`Subscriber.apply` 内 `subscribe(&fiber_ctx, ...)`（订阅落 **fiber ctx** 累加器；步逆 = 订阅 disposer 一并进 fiber.dispose）→ `emit(1)` 送达 → `sub.retire()`（unload：fiber.dispose 执行 d() 退订；累加器逆 arm ed 跳过）→ `emit(2)` 不再到达。直证链完整。
- **EventsProvider 复用—核实通过**：`#3`/`#2` 均以 `ctx.use_component(EventsProvider, ...)` 绑定总线（M1.1 组件），`subscribe*` 经 `ctx.get::<EventsKey>()` 取总线；`EventsProvider` 保持 core 原生 once（m-2 零依赖定位）未变动。
- **R 上界（Minor-1 延续）—核实通过**：`subscribe_serial`/`subscribe_bail` 的 `R: Send + Sync + 'static` 与草案 v0.3.1（O-5 修订）一致。
- **n-3''（serial/bail R 独立）—核实通过**：`on_serial` doc 补注（`on_serial<P,u32>` 与 `on_bail<P,String>` 可共存），与 modes 表按 (Symbol, Mode) 分键记各自 R 的自洽语义对齐。
- **测试质量—核实通过**：#3/#2 均直证草案语义（卸载自动退订 / 双路径 armed 恰一次）；使用日志（`Log = Arc<RwLock<Vec<String>>>`）满足 Send+Sync（监听器上界）且在单线程测试下无竞争；无脆弱时序（纯 sync，无 yield/spin）。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `cargo test -p cordis-events` — **PASS**，12/12（events.rs 集成 12 条，含 m13 的 #3/#2 两用例；lib/doctest 0）。
2. `cargo clippy -p cordis-events --all-targets -- -D warnings` — **PASS**，exit 0，无告警。
3. `cargo fmt --check -p cordis-events` — **PASS**，exit 0。
4. `cargo doc -p cordis-events --no-deps` — **PASS**，0 告警。

---

## 结论

M1.3（Step 2 订阅即效应集成——订阅经 ctx.effect 落账 + 卸载自动退订 + 双路径 armed）实现与草案 v0.3.1 §3.1、计划 M1.3 完全对齐：验收 #3（fiber 退役自动退订）与 #2（手动 + dispose_all 双路径 armed 不 double）均被直证；订阅步逆进 ctx 累加器（随卸载自动退订）与返回 disposer（共享 armed）的双路径语义由 core `Context::effect`/`StepGuard` 保证正确。工程门禁全绿，无逻辑缺陷。

**建议放行进入下一里程碑 M1.4**（Step 3：waterfall around/短路/terminal + 重入快照/空集精化，验收 #4/#7，草案 §2.2 E-1/E-2）。

通过前无必须修复项（Major 0 / Minor 0）。3 项 Nit（subscribe_waterfall/serial/bail 集成直证补足、`d as Disposer` 清理、subscribe* ctx 语义 doc 注记）可在 M1.4 或后续小修中处理，不阻塞合入。
