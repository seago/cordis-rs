# 代码审查报告：commit `83c254a`（M0.3 生命周期核心，Phase 0）

- **审查对象**：`83c254aa91eb5d20b26326ad630b16304f1203b4` — `feat(async): M0.3 生命周期核心——AsyncRegistrar/条目/settle + I-3/drain 重入单测（Phase 0）`
- **审查日期**：仓库时区（2026-08-18）
- **审查人**：independent-review-agent
- **审查范围**：`git show 83c254a`（仅 `crates/cordis-async/src/lib.rs` +393 / `crates/cordis-async/tests/protocol.rs` +409），对照 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§2/§3、`docs/cordis-async-PHASE0-PLAN.md` §Step 2。
- **验证手段**：静态阅读 + 实际运行 `cargo test -p cordis-async`、`cargo test --workspace`、`cargo clippy -p cordis-async --all-targets -- -D warnings`、`cargo fmt --check`、`cargo doc -p cordis-async`（用 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`）。审查对象超出 M0.3 范围的 `spawn_remote`/`retire`/`update`/`shutdown`/`is_quiet`（分属 M0.4–M0.6）不在本次判定范围，仅在 traceability 段备注。

**改动统计**：2 文件，+799/-3。
- `lib.rs` +389：`CancelFlag`、`AsyncCx` 补齐（get_cloned/set/cancellation/fiber/generation）、`AsyncBehavior`、`AsyncFiberState`、`TailQueue`+`settle`（FIFO 排空 + 64 轮守卫）、`AsyncFiberEntry`、`AsyncRegistrar`（sync 包装）、`AsyncRuntime`（new/use_component/settle）。
- `tests/protocol.rs` +409：`m03` 模块 3 个单测（I-3、drain 重入、drain 自再生守卫 panic）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（rustdoc 告警：公开 doc 链接私有 const `MAX_DRAIN_ROUNDS`）
- **nit**：3（I-3 激活同步用自旋而非可 await 条件；计划 vs 代码的 `spawn_remote`/`new`/`use_component` 签名 traceability 偏差；`entries` 映射桶无淘汰）

M0.3 生命周期核心与草案 v1.4 §2/§3、计划 §Step 2 逐条一致，10 个核查要点全部核实通过；两阶段卸载（I-3）、drain 重入、自再生守卫（§3.4）、评审点 B/H/C-6/A/E、slot 三处持有 liveness、单线程纪律（C-3）均成立。测试直证充分、决定论成立，工程门禁（fmt/clippy -D warnings/test/doc）全部绿，workspace 无回归。修正 1 项 rustdoc 告警即可达满分，不阻塞合入。

---

## 发现

### Minor-1（建议修复）：公开 doc 链接到私有常量，产生 rustdoc 告警

- **位置**：`crates/cordis-async/src/lib.rs` —— `TailQueue` 结构 doc（行 230-231）与 `TailQueue::settle`（行 251-252）的 intra-doc 链接 `` [`MAX_DRAIN_ROUNDS`] ``；被链接对象 `const MAX_DRAIN_ROUNDS: u32 = 64`（行 232）无 `pub`。
- **问题**：`TailQueue` 与 `settle` 均为 `pub`，其 doc 中引用私有 const，`cargo doc -p cordis-async` 报 `public documentation for settle links to private item MAX_DRAIN_ROUNDS`（实际验证输出见「验证记录」）。本 crate 仅 `deny(missing_docs)`，未 `deny(rustdoc::broken_intra_doc_links)`，故**不阻塞编译/门禁 A**，但属真实文档缺陷，会污染后续文档型 `-D` 检查——与仓库 M0.1 nit-1（broken rustdoc links 归零）的既定纪律呼应，应收口。
- **草案/计划依据**：计划 §5 纪律「`#![deny(missing_docs)]`」「门禁 A：`cargo fmt --check` + `clippy -D warnings` 全绿」；rustdoc 告警虽未入 `-D`，但与仓库「broken rustdoc links 归零」的既有收口目标冲突。
- **建议**：将 `const MAX_DRAIN_ROUNDS` 改为 `pub(crate)`（仍满足 `pub fn settle` 文档链接；保留内部可见性语义），或把 doc 中链接改为纯文本/带值注记。改动一行，`cargo doc` 告警归零。

### Nit-1（低）：I-3 测试的激活同步依赖自旋 yield 循环，非可 await 就绪条件

- **位置**：`crates/cordis-async/tests/protocol.rs` `i3_dependent_async_inverse_settles_first`（行 341-412）—— 检测 consumer `Active` 用 `for _ in 0..64 { yield_now().await; if consumer Active break }`，再补 8 次 yield 等 drive 填账。
- **问题**：consumer 激活依赖 provider 的 drive 任务在 LocalSet 内被轮询后**同步**完成 `cx.set::<KeyDep>` → notify → refresh → consumer reload → spawn 其 drive → 单 poll 内日志 `consumer:run`。决定性成立（单线程 LocalSet FIFO，`next()` 内无 `await`、一次 poll 即自启动到完成），当前实现下 `consumer:run` 在 consumer 被 spawn 后**下一次 yield 即落盘**，8 次额外 yield 头寸充裕，**无现实 flaky 风险**。但该确定性是"spin 基数 + 单 poll 完成"的巧合产物，对后续改动脆（例如在 `OneShotIter::next` 里加一个 `yield_now` 或改成多步，8 次就可能不够）。
- **草案/计划依据**：草案 §9 测试 3「断言消费者 async 逆 settle 先于提供者（日志序直证）」——测试确实直证，未违反。此为稳健性脆弱、非语义错误。
- **建议**：可引入一个 awaitable 的就绪条件（如共享 `Notify`/自旋改按需）替代固定轮数；不强求。仅建议在 `next()` 内保持"日志与步完成同步、无中途 await"的约定并在注释注明，避免日后改坏时静默 flaky。

### Nit-2（低）：计划 §Step 2 task 1 与实现的 `spawn_remote`/门面签名 traceability 偏差

- **位置**：计划 `docs/cordis-async-PHASE0-PLAN.md` §Step 2 task 1 列 `spawn_remote` 签名到 M0.3 的 AsyncCx；但 `AsyncCx`（lib.rs）未含 `spawn_remote`。另 `AsyncRuntime::new(&ctx)`（lib.rs）与草案 §5 `new() -> Self` 不同签名；`use_component -> Rc<Fiber>` 与草案 `Result<AsyncFiberHandle, RegistryError>` 不同。
- **问题**：
  1. `spawn_remote` 是完整 Remote 桥的一部分，草案/计划本身把完整桥列在 Step 5（M0.6）并明确「Remote 桥（草案 §2/§4）——spawn_remote」；Step 2 task 1 同时提 `spawn_remote` 属计划文本的重复/歧义。实现将其推迟到 M0.6 合理，但计划该行未标注"延迟"，产生 traceability 缺口。
  2. `AsyncRuntime::new(&ctx)` 将签名改为取 `&Rc<Context>` 以获取 core `Runtime` 并核对 LocalSet 对齐（C-3）——**合理且更安全**的偏离（草案 `new()` 无渠道取进程单例 Runtime）。`use_component` 返回 `Rc<Fiber>` 是 M0.5 引入 `AsyncFiberHandle` 前的合理临时形态。
- **草案/计划依据**：草案 §2/§5、计划 §Step 2/Step 5。均为"计划措辞/门面演进"偏差，非语义错误。
- **建议**：在计划 Step 2 task 1 对 `spawn_remote` 追加"延迟至 Step 5/M0.6"的注记；在 `AsyncRuntime` doc 注明 `new(&ctx)` 与草案 `new()` 的偏离理由（当前 doc 已述 C-3，可补一句签名差异）；`AsyncFiberHandle` 替换留 M0.5。

### Nit-3（低）：AsyncRuntime `entries` 映射桶只增不减（观察项）

- **位置**：`crates/cordis-async/src/lib.rs` `AsyncRuntime::use_component`（行 356-357）`self.entries.borrow_mut().insert(fiber.id(), entry)`；M0.3 无 `remove`/淘汰路径。
- **问题**：每次 `use_component` 均向 `entries` 插入一个 `Rc<AsyncFiberEntry>`，M0.3 范围内不删除。因 `entries` 当前仅 `#[allow(dead_code)]`（M0.4 失败通道按 fiber 查条目用），不产生功能性泄漏（AsyncRuntime drop 时整体释放），且 entry → TailQueue 无回边、不构成强环（评审点 B 成立）。属"演进期待办"，非缺陷。
- **草案/计划依据**：草案例 §5 注册表、契约 C-5 记账纪律；与 M0.4 退役/remove 路径相关。
- **建议**：M0.4/M0.5 落实 retire/remove 时同步从 `entries` 淘汰对应条目（或改用 `Weak` 值），避免长驻进程挂大量已退役条目。本次不阻塞。

### 未发现问题的核查点（逐条确认）

- **评审点 B（所有权/引用环）——核实通过**：链 `Fiber → component(AsyncRegistrar) → entry → queue(TailQueue)`；`AsyncRuntime → {core(Runtime), tails(TailQueue), entries→entry→queue}`。`AsyncRegistrar` 不持 `Rc<AsyncRuntime>`，`AsyncFiberEntry` 无回边到 AsyncRuntime；所有路径终结于 `TailQueue`（其无回边）、slot 的 `Rc<RefCell<Option<AsyncDisposer>>>`（被 drive 逆/fiber.dispose 逆/队列 tail 持有，settle take 后归零）。guard 闭包持有的 `Rc<Runtime>` 仅 drive 存续期临时强引用，不构成环。**无环成立**。
- **契约 C-6（注册器逆）——核实通过**：逆闭包（lib.rs `once` 步内 `Box::new(move || { cancel.cancel(); entry.enqueue_tail(...) })`）仅两步，O(1)、不 await、不 panic、仅短暂借用 `queue.inner`（`enqueue_tail` 内部 `borrow_mut().push_back`），不碰 `entry.state`/`generation`、不借 slot。被 `fiber.dispose` 在 core unload 的 `dispose_all` 中同步执行（runtime.rs:522：先 `drain(..)` 释放借用再跑逆），无 RefCell 竞争、无重入冲突。**在 unload dispose 循环中安全**。
- **评审点 H（disposer 记账统一）——核实通过**：drive 完成闭包（Ok）把 disposer 只写共享槽 `*slot.borrow_mut()=Some(...)` 并 `mark_running(generation)`（代次匹配才置；代次不匹配＝旧代迟到，跳过，不影响记账——记账唯一通道是 settle 的 `slot.take()`）。Failed 路径 `on_failed` 留槽空。settle 逐条 `await handle → take → d?.await`，恰一次、无 double-run。**与 drive 完成/卸载时序无关**成立。
- **I-3（Async-Casc-Unload）——核实通过**：`unload`（runtime.rs:510-522）先 `notify(provided)` 同步级联依赖者（consumer 先 refresh→unload→dispose_all→注册器逆 cancel+入队），provider 自身 dispose_all 后执行 → 依赖者 tail 先入队；settle `drain(..)` FIFO（`VecDeque` push_back / drain(..).collect 保持序）排空 → consumer 逆先 settle。测试日志序直证 `consumer:inverse` 先于 `provider:inverse`。**顺序免费来自 core 级联入队序**成立。
- **drain 重入 + 守卫（§3.4）——核实通过**：收敛逆内 `use_component` 挂新组件 → 其 `retire()` → unload → 新 tail 入**下一代队列**（settle 先 `drain(..).collect` 取走本代批、await 期间 `RefCell` 空闲，收尾逆重入入队安全）；settle 循环排空至空。自再生测试：每轮逆挂"又一个自身"并 retired → 每轮入一尾 → 第 64 轮处理第 64 尾后入第 65 尾 → 下一排空 `rounds=65`，`assert!(65<=64)` 失败 → panic。**轮数语义正确（第 65 轮触发），`#[should_panic(expected="drain 自再生死循环守卫")]` 子串匹配断言消息**。
- **guard 构成（评审点 A/E 双保险）——核实通过**：`view` 在 `apply` 时经 `ctx.runtime().fiber(id).target_view()` 捕获。`apply` 仅由 `reload` 调用，`reload` 仅在 `refresh` 探测 `target=Some` 后进入（runtime.rs:367-369），故 apply 时 `target_view()` 必为 `Some`（`None=>false` 是防御臂）。guard 先查 `cancel.cancelled()`，再比 `runtime.fiber(id).target_view()==view`（`is_some_and`；fiber 移除后返回 None → guard false，防御正确）。target 变化 → core refresh/unload → 逆 cancel+入队 → drive 最近步界退场 → settle 收账，闭环成立。
- **slot Rc 三处持有 liveness ——核实通过**：drive 闭包（写，完成任务后释自身 Rc）、注册器逆闭包（移入 `fiber.dispose`，随 fiber 活到 unload）、settle tail 条目（enqueue 时移入）；逆闭包在 fiber 存续期恒保 slot，故 unload 时 `enqueue_tail` 必然可引用存活 slot；settle take 后 tail drop → 计数归零无泄漏。
- **单线程纪律（C-3）——核实通过**：全部 `use_component`/`settle`/`spawn_local` 在测试的 `LocalSet::run_until` 内；`spawn_local`（lib.rs `apply` 步内）非 LocalSet 上下文将 panic 诊断（契约 C-3 的"违反 = panic"）；settle `await JoinHandle` 在 LocalSet 内驱进度，不跨出组合线程。`CancelFlag` 用 `Rc<Cell<bool>>`（非 Send）注解"不跨线程"，约定正确。
- **测试质量——核实通过**：I-3 直证依赖序（provider 挂载后经 async 步内 `set::<KeyDep>` 绑定 + notify 激活 consumer，consumer `next()` 内日志、无中途 await → 单 poll 决定论）；`consumer:inverse`/`provider:inverse` 顺序由 settle 序断言覆盖。drain 重入用例真实覆盖"新一代队列被排空"（A 逆内挂 B + yield + retire 入下一代 → settle 排空 B）。守卫用例确定触发第 65 轮 panic（轮数语义核对见上），非提前失败。I-1/I-2 既有 5 用例回归绿。仅 robustness（Nit-1）与计划 traceability（Nit-2）为观察项。
- **工程门禁——核实通过**：`#![deny(missing_docs)]`；无 `unsafe`（`unsafe_code=deny` 继承）；命名与草案术语一致（AsyncRegistrar/AsyncFiberEntry/TailQueue/settle/AsyncCx 等）；`#[allow(dead_code)]` 均为 scoped 且带 M0.4/M0.5 理由注释，无 blanket allow。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`（`cargo +1.97.0` 是否存在本地 toolchain 已验证，fmt/clippy/test/doc 均成功 rc=0）。

1. `cargo test -p cordis-async` — **PASS**，7/7：
   - `i1_composite_disposer_runs_lifo` / `i2_guard_false_at_step_boundary_keeps_inflight_step` / `i2_guard_false_immediately_yields_empty_composite` / `i2_guard_flips_while_inflight_step_pending`（M0.2 回归）
   - `m03::i3_dependent_async_inverse_settles_first` / `m03::drain_reentry_next_generation_is_drained` / `m03::drain_self_regeneration_triggers_guard_panic`（should panic）
2. `cargo test --workspace` — **PASS**，全 workspace 测试绿，无 FAILED，无回归（cordis-async 7，cordis-core 49，其余各 crate 均 ok）。
3. `cargo clippy -p cordis-async --all-targets -- -D warnings` — **PASS**，exit 0，无警告。
4. `cargo fmt --check -p cordis-async` — **PASS**，exit 0。
5. `cargo doc -p cordis-async --no-deps` — **PASS（含 1 告警）**：`warning: public documentation for settle links to private item MAX_DRAIN_ROUNDS`（对应 Minor-1）。

---

## 结论

M0.3（Step 2：AsyncCx + 两阶段卸载 —— I-3 + drain 重入）实现与草案 v1.4 §2/§3、计划 §Step 2 完全对齐，10 个核查要点所有关键路径核实通过，无逻辑缺陷。**建议放行进入下一里程碑 M0.4**（失败通道 I-4 / C-7，草案 §3.3）。

通过前建议处理 1 项 Minor（把 `MAX_DRAIN_ROUNDS` 改 `pub(crate)` 使 `cargo doc` 告警归零，呼应仓库 broken rustdoc links 收口纪律）；3 项 Nit 记录在案，可在 M0.4 或后续小修中落地，不阻塞合入。
