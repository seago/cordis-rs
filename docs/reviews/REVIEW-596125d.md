# 代码审查报告：commit `596125d`（M0.4 失败通道，Phase 0）

- **审查对象**：`596125d3dbb5e8b50c2ce0022ac9bc1a60ea33f2` — `feat(async): M0.4 失败通道——I-4 自退役/disabled 写回/复活 + C-7 shutdown 双真 + 测试 4/11（Phase 0）`
- **审查日期**：仓库时区（2026-08-18）
- **审查人**：independent-review-agent
- **审查范围**：`git show 596125d`（`crates/cordis-async/src/lib.rs` +216 / `crates/cordis-async/tests/protocol.rs` +229 / `Cargo.toml` +1 dev-dep `cordis-loader`），对照 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§3.1/§3.3/§5/§9、`docs/cordis-async-PHASE0-PLAN.md` §Step 3（M0.4）与含「entries 桶淘汰（REVIEW-83c254a nit-3）」任务。纯 Cargo.lock 的 chore 提交 `fa40479` 不在审查范围。上一里程碑结论：REVIEW-83c254a（PASS WITH NITS）。
- **验证手段**：静态阅读 + 实际运行 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 test -p cordis-async`、`cargo +1.97.0 test --workspace`、`cargo +1.97.0 clippy -p cordis-async --all-targets -- -D warnings`、`cargo +1.97.0 fmt --check`、`cargo +1.97.0 doc -p cordis-async --no-deps`。审查对象超出 M0.4 范围的 `update`（M0.5 方完备）、`settle` 后续自再生守卫等仅作一致性上下文，不在本次判定核心。

**改动统计**：3 文件，+414/-32。
- `lib.rs` +216：`ActiveSession`（generation/cancel/handle/slot 三件套）、`AsyncFiberEntry` 增 `session`/`fiber(Weak)`/`registry` 字段 + `adopt_fiber`/`fiber_rc`/`install_session`/`take_session`/`self_register`/`on_failed`/`state`；注册器逆改经 `take_session` 取会话（幂等）；`on_failed` 代次匹配 → Failed 终态 + 自退役（`fiber.retire()`）；`AsyncRuntime.entries` 改 `Weak` 值 + apply 自登记（nit-3 落地）、`wrap_component`、`entry(id)`、`is_quiet`、`shutdown`（兜底 cancel+enqueue+settle + 双真正式 assert）。
- `tests/protocol.rs` +229：`m04` 模块 2 个测试（测试 4 即 I-4；测试 11 拆两用例——编排方退役双真 / 未退役违约捕获）。原 m03 的 `Shell`/`OneShotBehavior`/`async_inverse` 改 `pub(super)` 供 m04 复用。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：3（I-4 测试的 yield 轮询基数；`is_quiet` 仅过滤 `Active` 对 Reloading/Unloading 的宽松依赖 `&& core.is_quiet()` 合取；I-4 复活断言复用原 `fid` 属隐式依赖）

M0.4 失败通道与草案 v1.4 §3.3/§5、计划 §Step 3 逐条一致，8 个核查要点全部核实通过：I-4（Failed 静止终态 + 自退役写回 disabled + settle 恒完成 + is_quiet 真 + 复活）直证；C-6 逆契约演进（take_session + cancel + enqueue_tail，仍 O(1)/不 await/不 panic/不借他 RefCell，take 幂等、代次防串代）成立；C-7 shutdown 双真正式 assert（开放项 §4 决议落地）与测试 11 双用例直证成立（should_panic 确在断言处，非意外 panic）；entries Weak + 自登记（nit-3）落地且 apply 时序成立于 use_component/wrap_component 两路径；liveness 链改为「drive 闭包 + entry.session + tail 条目」三持有仍**无泄漏**；防环（Weak 值 registry、entries Weak、session 非 Rc<AsyncRuntime>）成立。工程门禁（fmt/clippy -D warnings/doc/test/workspace）全部绿，无回归，无 unsafe，命名与草案术语一致。上一里程碑的 Minor-1（`MAX_DRAIN_ROUNDS` rustdoc 链接）已收口（doc 当前 0 告警）。3 项 Nit 均为稳健性/可读性观察项，不阻塞合入。

---

## 发现

### Minor：无

未发现必须修复才可合入的问题。

### Nit-1（低）：I-4 测试的异步落地等待依赖固定 yield 轮数（72 次），对后续改动脆

- **位置**：`crates/cordis-async/tests/protocol.rs` `m04::i4_failed_settles_quiet_writeback_and_revive` —— 等失败写回用 `for _ in 0..64 { yield_now().await; if loader.entry_disabled=="Some(true)" break }`，复活后用另一 64 次 yield 等 `revive:run`。
- **问题**：与 REVIEW-83c254a Nit-1 同款风格。当前决定性成立：单线程 LocalSet FIFO，`FailOnceIter::next` 无中途 await、一次 poll 即自启动到完成（失败/复活日志在 spawn 后下一次 poll 即落盘，且 loader apply/retire hook 全部同步），64 次 yield 头寸充裕，无现实 flaky。但该确定性是「固定 spin 基数 + next() 单 poll 完成」的巧合产物；future 若在 `next()` 内插一个 `yield_now` 或改多步，64 轮可能不够。
- **草案/计划依据**：草案 §9 测试 4「`Failed` 后 settle 恒完成、`is_quiet` 为真；条目 disabled 写回；重启用后复活」——测试确实直证，未违反。属稳健性脆弱、非语义错误。
- **建议**：不强求；可引入 awaitable 就绪条件（共享 `Notify` / 按需不固定基数）替代固定轮数，并在 `next()` 内保持「日志与步完成同步、无中途 await」约定并注释注明，避免日后改坏时静默 flaky。

### Nit-2（低）：`is_quiet` 只过滤 core `Active` 态，对 `Reloading`/`Unloading` 一律视为静止——正确性依赖 `&& core.is_quiet()` 合取

- **位置**：`crates/cordis-async/src/lib.rs` `AsyncRuntime::is_quiet`（行 618-627）——`!matches!(*f.state(), FiberState::Active { .. })`。
- **问题**：core 视图把 `Reloading`/`Unloading`（转换在途）视为非静止（`Runtime::is_quiet` 对非 Active/Inactive 返回 false，runtime.rs:188）；async 的 `is_quiet` 却只对 `Active` 判非静，`Reloading`/`Unloading` 会返回 true。单看 `AsyncRuntime::is_quiet` 会漏报在途 core 转换。**当前不构成缺陷**：唯一消费点是 `shutdown` 的 `assert!(core.is_quiet() && self.is_quiet())`——AND 合取下 core 侧已排除 Reloading/Unloading，divergence 被合取消解；且 `i4` 测试的 `is_quiet()` 调用点均在 `settle().await` 后（静置）。doc 已明示此差异与一致性基础（行 614-617），属**有意的语义偏离**。
- **草案/计划依据**：草案 §5 `is_quiet`「async 视图静止判定（Failed 视为静止，I-4）」及 C-7 双真断言的一致性契约。偏离本身合理且被文档化。
- **建议**：`is_quiet` 的 doc 已足够，可再补一句「仅在 `&& core.is_quiet()` 合取下才是整体静止判定；单独使用时不排除 core 在途转换」，防止未来单独复用 `AsyncRuntime::is_quiet` 作为通用静止谓词时误用。可选。

### Nit-3（低）：I-4 复活断言复用最初的 `fid`，正确性依赖 `wrap_component` 复用同一 entry/registrar 实例

- **位置**：`crates/cordis-async/tests/protocol.rs` `m04::i4_...` —— 复活后 `rt.entry(fid).state() == Running{generation:2}` 用**失败前**捕获的 `fid`（loader rebuild 下的新 fiber id 未取用）。
- **问题**：`wrap_component` 只产一个 `AsyncRegistrar`（持同一个 `AsyncFiberEntry`），loader 重建复用它（新 fiber 也可能新 fid）→ 同一 entry 经 `self_register` 以新 fid 再登记，且原 fid 的弱引用仍 upgrade 到同一 entry，故 `rt.entry(原 fid)` 恒命中。断言成立且 `generation:2` 由 `begin_activation` 正确递推。属**隐式正确**、非缺陷——但对读者不透明（entry 在同一注册器实例自然共享这一点未注明）。
- **草案/计划依据**：草案 §3.3 复活「loader 重载 → core `update_fiber` 复活路径 → 注册器重 spawn drive（新代次）」；同一 entry 换代正确。
- **建议**：可在断言前注释「entry/registrar 经 `wrap_component` 实例恒共享，fid 换代不改变 entry 身份」，或显式取复活后新 fiber 的 entry 再查，增强可读性。可选。

### 未发现问题的核查点（逐条确认）

- **§3.3 I-4 失败路径——核实通过**：drive 任务内 `Err(e) => entry.on_failed(generation, e)`；`on_failed` 代次匹配（`generation.get()!=gen` 则跳过）→ `Failed(e)` 静止终态 → `self.fiber_rc().retire()` 自退役。`Fiber::retire()`（core fiber.rs:196-204）置 retired + 触发 retire hook → loader `register_retire_hook` 过滤（条目仍在且未 disabled）写回 `disabled=true`（G1 通道，loader lib.rs:452-481）；core 级联卸载依赖者（sync 部分）走 dispose → 注册器逆 take_session + cancel + enqueue。**时序核查**：`on_failed` 在 drive 任务 async 块内同步调用 `retire()`（retire 内部 refresh/unload/dispose 全部同步，不 await）——逆闭包同步执行 `take_session` 取到**当前任务自身**的 handle 并入队；随后 async 块返回、任务结束；settle 对该已结束 handle `await` 立即就绪，**无死锁**。失败 slot 恒空（drive 未写），settle take None 跳过，无 disposer 泄漏。
- **C-6 逆契约演进——核实通过**：注册器逆闭包（lib.rs:524-529）只 `entry.take_session(generation)` 后 `cancel.cancel()` + `entry.enqueue_tail(...)`——O(1)、不 await、不 panic、仅短暂借 `entry.session`（不借 state/generation/queue 的 RefCell，与 M0.3 C-6 同款纪律）；`take_session` 幂等（`session.take()` 先取者得，后取 None 跳过，`Option::is_some_and` 代次核对防串代）。**记账/防环性质保持**：session 单一持有者 = `entry.session`（`handle` 不可克隆）；逆/兜底均经 `take_session` 取走，`handle` 恰一次入队或已被取（兜底先取则逆跳过的路径仍被 settle 收账）——注销与兜底不双记账。
- **liveness 链（任务核心追问）——核实无泄漏**：slot 的 Rc 由「drive 闭包 + `entry.session`(ActiveSession) + settle tail 条目」三处持有（**逆闭包不再直接持 slot Rc**，改持 `Rc<AsyncFiberEntry>`）。替代关系成立：`entry` 被 Fiber→AsyncRegistrar（component 字段）强持有，且 AsyncRegistrar 持 `entry`；故 **entry（及其 session/slot）恰与所属 fiber 同存活**——逆闭包仅在 fiber 卸载瞬间执行（fiber 彼时存活），`take_session` 必然引用到活 slot；取走后 session 置 None、slot 移入 tail；settle `take` 后 tail drop → Rc 归零、无泄漏。drive 闭包在任务完成释放自身 Rc；任务完成晚于/s先于卸载均安全（评审点 H 机制不变）。fiber 释放后 entry 不再被强持（registrar/drive/逆随之 drop）→ entries 弱引用变 stale，`entry()`/`is_quiet` 惰性 upgrade None，**Weak 惰性淘汰正确**。
- **C-7 shutdown——核实通过**：`shutdown` 兜底——枚举 entries 中仍 `Active` 的 fiber，`take_session` 取会话 + cancel + enqueue_tail（不代做 core 退役）；settle 到静止；随后**正式 assert** `core.is_quiet() && async.is_quiet()` 双真（开放项 §4 决议「至少一处正式 assert」落地）。`is_quiet` 语义（`tails.is_empty()` 且无仍 Active 的 async 组件）与 core Active=静止的差异**合理且有文档**：core 视 Active（无在途转换）为静，async 要求关停后不再有任何运行 async 组件——整合进 `&&` 合取即得一致的整体静止判定。测试 11 `should_panic` 验证：**确在断言处失败**——未退役即关停时 async 兜底收账（cancel+enqueue+settle），core 侧 fiber 仍 Active → `core.is_quiet()=true` 但 `async.is_quiet()=false`（Active 组件存在）→ `&&` 为假 → 断言 panic（子串 `shutdown 双真断言` 匹配）——非别处意外 panic。
- **nit-3（entries Weak + 自登记）—核实通过**：`use_component` 不再显式 insert，条目在 apply 时经 `attach_registry` 句柄 `self_register(id)`（fiber id 此刻已知）；**apply 必在 use_component 返回前发生的时序成立**——core `use_component`/`register` 内同步注册并 `refresh` 激活 → 同步执行 `ctx.effect` 步（apply 内 self_register）；`wrap_component` 经 loader `apply` 同路径自登记。`self_register` 用 `Rc::downgrade`（Weak 值，无回边、惰性淘汰）。runner 弱 entry 由 fiber→AsyncRegistrar 强持，Weak 失效正确。
- **防环再验证——核实通过**：新增 registry 句柄（entry→entries map Rc）值为 Weak → 无强边；entry 的 `fiber: Weak<Fiber>`、`registry: Weak` 均无回边；session 持 `JoinHandle`（非 Rc<AsyncRuntime>）不引入环；entries map 持 Weak<entry>。评审点 B 的无环结论在 M0.4 扩展后仍成立。
- **测试 4（I-4）—核实通过**：全流程直证草案语义——`wrap_component` → `loader.register_component` → `apply`（disabled=false）→ 等失败写回 → `rt.entry(fid).state()==Failed` → `rt.settle()` 恒完成 + `is_quiet()` 真 → `apply(disabled=false)` 复活 → `revive:run` 落盘 + 同 entry `Running{generation:2}`。每断言直证一项 I-4 语义（静止、写回、settle、quiet、复活换代）。71 次 yield 头寸（先 64 后 64，实际复活仅在写回完成后触发）在给定实现下充裕、无 flaky（见 Nit-1）。
- **测试 11 双用例—核实通过**：① 编排方退役（`loader.apply(&[])` 驱动 teardown）→ `entry_disabled=="None"` 证明退役零配置污染（filter 语义：条目已移除 → writeback 过滤）；`shutdown` 收尾尾 + 双真通过。② 未退役即 `shutdown` → 兜底收账后双真断言失败 panic 捕获违约。两用例分别直证「编排方退役→双真」与「未退役→违约捕获」，且「零配置污染」由 ① 的 entry_disabled==None 断言直证。
- **工程门禁与文档—核实通过**：`cargo test -p cordis-async` 10/10 全绿（含 `should_panic` 两用例）；`cargo test --workspace` 全无失败（cordis-async 10、core 55、loader 49，跨度含 wasm/go 慢套件均 ok）；`clippy --all-targets -D warnings` exit 0 无告警；`fmt --check` exit 0；`doc -p cordis-async --no-deps` **0 告警**（上一里程碑 Minor-1 的 `MAX_DRAIN_ROUNDS` intra-doc 私链已改为反引号代码引用，收口）。`#![deny(missing_docs)]`；无 unsafe（`unsafe_code=deny` 继承）；`#[allow(dead_code)]` 均 scoped 带理由（CancelFlag::reset 标注「M0.4 复活路径使用」、generation()、Tail.generation）。命名与草案一致（ActiveSession/AsyncFiberEntry/AsyncRuntime::shutdown/is_quiet/wrap_component/on_failed/take_session）。M0.4 起重依赖 `cordis-loader`（仅 dev-dep，不污染 run 依赖，符合计划 §5 纪律）。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `cargo test -p cordis-async` — **PASS**，10/10（含 2 个 `should_panic`）→ 10 passed, 0 failed。0 unit，10 integration（protocol.rs），0 doc。
2. `cargo test --workspace` — **PASS**，全 workspace 无失败（cordis-async 10、cordis-core 55、cordis-loader 49、hi_cordis/cordis_hmr/hmr/macro/native/wasm 及各 wasm/go 桥套件均 ok；首轮完整跑秒差 38 个 `test result: ok` 套件）。
3. `cargo clippy -p cordis-async --all-targets -- -D warnings` — **PASS**，exit 0，无警告。
4. `cargo fmt --check -p cordis-async` — **PASS**，exit 0。
5. `cargo doc -p cordis-async --no-deps` — **PASS**，0 告警（grep warning 无命中）。

---

## 结论

M0.4（Step 3：失败通道 —— I-4 + C-7 shutdown，草案 §3.3/§5）实现与草案 v1.4、计划 §Step 3 完全对齐，8 个核查要点所有关键路径核实通过，无逻辑缺陷。I-4 自退役/disabled 写回/复活、C-6 逆契约演进（take_session 幂等 + 代次防串代）、C-7 shutdown 双真正式 assert、entries Weak+自登记（nit-3 落地）均成立；liveness 链收窄后（逆不再直接持 slot Rc，改经 entry.session）经逐持有者析仍**无泄漏**；防环、测试直证、工程门禁、文档完整均核实通过。

**建议放行进入下一里程碑 M0.5**（Step 4：AsyncRuntime 门面完善 —— update/代次 + 测试 8/9/10，草案 §5）。

通过前无必须修复项（Major 0 / Minor 0）。3 项 Nit（测试轮询基数、`is_quiet` 宽松语义的文档注记、I-4 fid 复用说明）记录在案，可在 M0.5 或后续小修中一并处理，不阻塞合入。
