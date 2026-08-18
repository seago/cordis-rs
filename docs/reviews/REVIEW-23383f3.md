# 代码审查报告：commit `23383f3`（M0.5 门面完善，Phase 0）

- **审查对象**：`23383f3dca09e192c4e814a444710657f29bd78d` — `feat(async): M0.5 门面完善——retire/update 门面 + 测试 8（代次更新）/9（无环关停）/10（H 竞态恰一次 + Failed slot 留空 + shutdown 补收账）（Phase 0）`
- **审查日期**：仓库时区（2026-08-18）
- **审查人**：independent-review-agent
- **审查范围**：`git show 23383f3`（`crates/cordis-async/src/lib.rs` +15 / `crates/cordis-async/tests/protocol.rs` +348/-3），对照 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§3.1/§5/§9（测试 8/9/10）、`docs/cordis-async-PHASE0-PLAN.md` §Step 4（M0.5）。上一里程碑结论：REVIEW-596125d（PASS WITH NITS，0 Major/0 Minor）。
- **验证手段**：静态阅读 + 实际运行工程门禁命令（见「验证记录」）。审查对象超出 M0.5 范围的既有结构（reviewer 点 B 防环、H 竞态 slot liveness、C-6 逆契约）仅作一致性上下文，不在本次判定核心。

**改动统计**：2 文件，+360/-3。
- `lib.rs` +15：新增 `AsyncRuntime::retire(&Rc<Fiber>)` 与 `AsyncRuntime::update(&Rc<Fiber>, config)` 两个门面方法——分别转发 `core Fiber::retire` 与 `core update_fiber`（契约 C-4：生命周期变更走门面）。
- `tests/protocol.rs` +348/-3：新增 `m05` 模块 4 个测试（测试 8 代次更新 / 测试 9 无环关停 / 测试 10 H 竞态 + shutdown 补收账），`m04::FailOnceBehavior` 升 `pub(super)` 供 m05 复用（同步改动 3 行可见性）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：3（草案 §9 测试 8 与计划 Step 4 的「旧尾巴先 settle」措辞 vs 实际 run:1→run:2→rev:1 实现序；测试 9 名为 shutdown 实走 retire+settle；yield 轮询基数约定在 m05 未统一注释/未在测试 8 用 break-on-condition）

M0.5 门面完善（retire/update + 测试 8/9/10）与草案 v1.4 §5/§9、计划 §Step 4 逐条对齐，5 个核查要点全部核实通过：测试 8 直证 update 闭环（代次递增 gen=2、fiber 身份保留、旧尾巴 settle 排空 rev:1 恰一次——时序 run:2→rev:1 直证「旧代尾巴在新代激活后收尾且新代未收账」）；测试 9 真正覆盖「AsyncRuntime 可完整释放」（`Rc::downgrade + drop → upgrade None` 即强计数归零，非仅编译通过）且 core 侧 `is_quiet` 合理；测试 10 竞态窗口**真实覆盖**「drive 恰在 cancel 后、settle 排空前完成 Ok」（先等 `a:pending` 落盘证明 drive 在途 → 再 `retire` → 再 `tx.send` 放行 → 再 settle 恰一次 take），Failed 路径 slot 留空无 disposer 残留，shutdown 对在途尾巴补收账成立；门面方法转发与 C-4 纪律一致、语义与既有 use_component/settle/shutdown 无冲突；回归无新脆弱点（liveness 链三持有、take_session 幂等、mark_running 代次防串代均复用于 m05 且成立）。工程门禁（fmt/clippy -D warnings/doc 0 告警/test 14/14/workspace 全绿）全部通过，无 unsafe，命名与草案术语一致。3 项 Nit 均为文档一致性/稳健性观察项，不阻塞合入。

---

## 发现

### Major：无

### Minor：无

未发现必须修复才可合入的问题。

### Nit-1（低）：草案 §9 测试 8 与计划 Step 4 的「旧尾巴先 settle」措辞与实际实现的 run:1→run:2→rev:1 序不一致，未文档注记

- **位置**：`tests/protocol.rs` `m05::update_bumps_generation_and_settles_old_tail` 的断言序（`run:1`→`run:2`→`rev:1`）；对照草案 §9 测试 8「旧尾巴在新代激活前 settle」与计划 §Step 4「旧尾巴先 settle」。
- **问题**：草案/计划字面要求「旧尾巴**在**新代激活**前** settle」；实际语义是 `retire/update` 的 core 侧 `update_fiber` **同步**完成 unload（旧代逆 cancel + 旧尾巴入队）→ 链式 reload（新代 spawn），`settle().await` 是编排方随后的独立调用——因此新代 drive 先落地（`run:2`），旧尾巴才由 settle 排空（`rev:1`），日志序必为 run:2→rev:1。这与草案「前」字面冲突，但**符合**本次审查任务给定的期望序（`时序 run:1→run:2→rev:1 直证「旧代尾巴在新代激活后收尾且新代未收账」`）与 I-2/H 语义（新旧代尾巴独立收账，无串代）。差异根因：core 冻结零改动下 update 是 unload+reload 原子同步实例，async 层无法在两者间插 settle 边界——草案措辞是理论/祈愿态，非可达成语义。属于**未文档化的措辞-vs-实现偏离**。
- **草案/计划依据**：草案 §9 测试 8；计划 §Step 4 任务 2「旧尾巴先 settle」。审查任务规约明示期望序为 run:1→run:2→rev:1。
- **建议**：在测试 8 注释（或 lib.rs `update` 门面 doc）补一句注记「core `update_fiber` 原子完成 unload+reload，新代 spawn 与旧尾巴 settle 之间无同步边界；旧尾巴由编排方随后 settle 排空（run:2→rev:1）」，把草案「前」措辞的不可达性与实际 I-2 独立收账序对齐，避免未来按草案字面核验误判。可选。

### Nit-2（低）：测试 9 名为 `shutdown_releases_runtime_no_cycle` 但实际走 retire+settle 而非 shutdown()

- **位置**：`tests/protocol.rs` `m05::shutdown_releases_runtime_no_cycle` —— 挂载 → yield → `rt.retire(&fiber)` → `rt.settle().await` → `Rc::downgrade(&rt); drop(rt); weak.upgrade().is_none()`。
- **问题**：草案 §9 测试 9 措辞「**shutdown** 后 AsyncRuntime 可 drop」；本测试实际调 `retire + settle`，未调 `shutdown()`（后者在 fiber 已退役、tail 排空后可走 double-quiet 断言路径）。就「AsyncRuntime 强计数归零」的验证目标而言，retire+settle 与 shutdown 等价可达——`Weak::upgrade() is None` 严苛证明作用域内无任何残存强引用，若存在 `AsyncRuntime → core → fiber → registrar → entry → AsyncRuntime` 回边则必 upgrade Some 暴露。但**测试名的 shutdown 语义未经此用例走查**（shutdown 路径已有 m04 测试 11 双真用例覆盖，此处不重复也合理）。属测试命名与实现的轻微错位，非语义缺口。
- **草案/计划依据**：草案 §9 测试 9；计划 §Step 4 任务 3「shutdown 后 AsyncRuntime 可 drop（Weak 计数）」。
- **建议**：将测试名改为 `retired_settled_runtime_releases_no_cycle` 或在注释注明「走 retire+settle（definitive 释放在 M0.4 双真已覆盖）而非 shutdown() 路径」，消除命名歧义。可选。

### Nit-3（低）：yield 轮询基数约定在 m05 未统一注释，测试 8 仍用纯固定轮数（前 REVIEW-596125d nit-1 的约定注释未在 m05 复述）

- **位置**：`tests/protocol.rs` `m05` —— 测试 8 用 `for _ in 0..8 { yield_now().await }`（无 break-on-condition、无决定论注释）；测试 10 / shutdown 补收账用 `for _ in 0..64 { ... if log.any(...) break }`（break-on-condition，较健壮）；m05 复用 m04 的 `FailOnceBehavior`（其决定论注释在 m04，未在 m05 复述）。
- **问题**：`UpdateBehavior::UpdateIter::next()`（test 8）与 `FailOnceBehavior::next()`（test 10-B）均无中途 await、单次 poll 落盘，固定 8/64 次 yield 在单线程 LocalSet FIFO 下决定性成立、无现实 flaky。但决定论出自「固定 spin 基数 + next() 单 poll 完成」的巧合，test 8 未复用 test 10 的 break-on-condition 更稳健形态，也未以注释载明「next() 无中途 await」约定（m04 已按同一 Nit-1 建议补过注释，m05 未延续）。属稳健性/可读性观察，非语义错误。
- **草案/计划依据**：草案 §9 测试 8/10；m04 REVIEW-596125d Nit-1 同款风格；审查任务 §5「yield 轮询基数……是否在 m05 测试中保持一致并注释」。
- **建议**：test 8 的 8 次固定 yield 改为与 test 10 一致的 `for _ in 0..N { yield; if 就绪 break }` 模式，或在模块头部补一段 m05 决定论注释（「UpdateIter/FailOnceIter::next() 无中途 await、单 poll 完成；仅 PendingOnceIter 有意含 await 以构造在途步」），使约定在 m05 内自洽。可选。

### 未发现问题的核查点（逐条确认）

- **测试 8（评审点 E / §3.1 update 闭环）—核实通过**：`rt.update(&fiber, 2u8)` → `core.update_fiber`（Active → `unload`：注册器逆 `take_session(gen=1)` + cancel + enqueue_tail 旧代尾巴；`dispose` drain 触发逆）→ target 未变 → 链式 `reload` → `fiber.apply` 重跑 → `AsyncRegistrar::apply` → `begin_activation()` gen=2 → 新 drive spawn（fiber 身份保留，`self_register` 以同 fid 写回）。日志 `run:1`→`run:2` 直证新代 drive 落地且旧逆未执行；`entry(fid).state()==Running{generation:2}` 直证代次递增 + 身份保留。settle 后 `rev:1` 恰一次、`run:1→run:2→rev:1` 直证「旧代尾巴在新代激活后收尾、新代未收账」；settle 后仍 `Running{generation:2}` 直证新代未被卸载。旧代 drive 在 update 前已写 slot（run:1 先落盘），`mark_running(1)` 因 generation.get()==2 跳过——防串代正确，不污染 gen=2 状态。
- **测试 9（无环关停 / 评审点 B）—核实通过**：`Rc::downgrade(&rt)` 后 `drop(rt)` → `weak.upgrade().is_none()` = **强计数归零的严格证明**（非「仅编译通过」）——若存在 `AsyncRuntime → core → fiber → registrar 逆闭包 → entry → AsyncRuntime` 回边，`drop(rt)` 仍无法释放、upgrade 必 Some。当前 entries 为 `Weak` 值、`AsyncFiberEntry.fiber` 为 `Weak<Fiber>`、registry 句柄 Weak、`ActiveSession` 持 `JoinHandle`（非 `Rc<AsyncRuntime>`）——无任何强回边，环不存在，drop 即释放，断言成立。配以 `ctx.runtime().is_quiet()`（退役+收账后 core 静止）合理。注：走 retire+settle 而非 shutdown()（见 Nit-2）。
- **测试 10-A（H 竞态 / 评审点 H）—核实通过**：竞态窗口**真实覆盖**「drive 恰在 cancel 后、settle 排空前完成 Ok」——先等 `a:pending` 落盘（证明 drive 在途、挂起于 `rx.await`），再 `rt.retire(&a)`（逆 `take_session` + cancel + **enqueue 在途 tail**，drive 仍未完成），再 `tx.send(())`（在途步放行 → `a:done` → drive 收 Ok → 写共享槽 → `mark_running`，I-2 语义：步界后 guard 不再查），最后 `rt.settle()`（`handle.await` 收尾驱动 → `slot.take()` 恰一次 → await 逆 `rev:a`）。`rev:a` 计数==1 直证**恰一次 take**（非先完成再退役的平凡路径——retire 时 drive 确未完成）。`a:done` 在 settle 前已落盘 + slot 恰一次 take 共同保证 rev:a 不重复、不遗漏。
- **测试 10-B（Failed 路径 slot 留空）—核实通过**：B 用 m04 `FailOnceBehavior`，首激活 `apply_async` 返回 `Failed` → drive 走 `Err` → `on_failed`（代次匹配）→ `Failed` state + `fiber.retire()`（sync 自退役）→ 注册器逆 enqueue tail（**slot 从未写——Err 路径不写槽**）。settle drain 该 tail：handle.await 收尾、`slot.take()` = None 跳过 → 无 disposer 可 await → `rev:b` 恒不出现。断言 `!log.any("rev:b")` + 前置 `entry(b.id()).state()==Failed` 直证「Failed 路径 slot 留空、tail 无残留」。
- **测试 10-C（shutdown 补收账）—核实通过**：C 在途（`c:pending`）→ `retire(&c)`（逆 cancel + enqueue 在途 tail，drive 未完成）→ `tx.send(())`（drive 完成 Ok 写槽）→ `rt.shutdown()`：枚举 entries 中仍 Active 的 fiber——C 已 Inactive，`take_session` 返回 None（会话已被逆取走，幂等不双记账，注册器逆未重跑）→ 其余无 Active → 兜底循环空 → `settle()` 排空在途 tail → `rev:c` 恰一次 + `is_quiet` true + shutdown 双真正式 assert 通过（C 已 Inactive 故 `async.is_quiet` 真）。直证「shutdown 对在途尾巴补收账（恰一次）+ 双真通过」。
- **门面方法（第 4 点）—核实通过**：`retire`/`update` 合法转发 core（`fiber.retire()` 与 `self.core.update_fiber(fiber, config)`），与 C-4 门面纪律一致（生命周期变更走门面、尾巴由 settle 记账）；doc 完整（契约 C-4 依据、§3.1 update 闭环分录、settle 排空语义）；签名取 `&Rc<Fiber>`（非草案 §5 的 `&AsyncFiberHandle`）——lib.rs `new` 的 nit-2 注记已声明为「AsyncFiberHandle 引入前的临时形态」，M0.5 起即保持 `Rc<Fiber>` 返回/参数形态，属已文档化偏离。与既有 use_component/settle/shutdown 无语义冲突（settle 幂等 drains、take_session 幂等防双收账）。
- **回归风险（第 5 点）—核实通过**：`FailOnceBehavior` 升 `pub(super)` + 字段 `pub(super)` 提升（log/attempts）为可见性收窄——m04 内原 `struct`/`field` 对外 m05 子模块可见，改动最小、无行为变化；`PendingOnceBehavior` 以 `Rc<RefCell<Option<Receiver>>>` 存放 oneshot rx，`apply_async` 经 `rx.borrow_mut().take()` 移出、单 iter 恰一次消费（第二次 apply 会 None → `expect("一次挂起")` 仅在有第二次激活时触发，本测试各 fiber 恰一次激活，不触发）。slot 三持有 liveness（drive 闭包/注册器逆闭包/tail 条目）与 take_session 幂等在 m05 复用路径全部成立，无新脆弱点。
- **工程门禁与文档（第 6 点）—核实通过**：见「验证记录」。`#![deny(missing_docs)]` 生效，无 unsafe（`unsafe_code=deny` 继承），命名与草案术语一致（AsyncFiberState::Running/Failed、settle/shutdown/is_quiet/update/retire）。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `cargo test -p cordis-async` — **PASS**，14/14（0 unit + 14 integration protocol.rs 含 2 个 `should_panic`：`i1`、`i2`×3、`m03`×2、`m04`×3、`m05`×4）。0 doc。
2. `cargo test --workspace` — **PASS**，exit 0，38 个 `test result: ok` 套件（cordis-async 14、cordis-core 55+多套件、cordis-loader 49、cordis-hmr 9、各 wasm/go 桥套件），全无 FAILED/`error[`/`warning:` 行。
3. `cargo clippy -p cordis-async --all-targets -- -D warnings` — **PASS**，exit 0，无警告。
4. `cargo fmt --check` — **PASS**，exit 0。
5. `cargo doc -p cordis-async --no-deps` — **PASS**，0 告警（grep warning/error 无命中）。

---

## 结论

M0.5（Step 4：AsyncRuntime 门面完善 —— retire/update + 测试 8/9/10，草案 §5/§9）实现与草案 v1.4、计划 §Step 4 完全对齐，5 个核查要点所有关键路径核实通过，无逻辑缺陷。测试 8 直证 update 闭环（代次递增 + 身份保留 + 旧尾巴 settle 恰一次排空、时序直证新代未收账）；测试 9 以强计数归零直证无环关停与可完整释放；测试 10 竞态窗口真实覆盖 H 时序（在途退役 → 放行 → settle 恰一次 take）、Failed slot 留空无残留、shutdown 对在途尾巴补收账恰一次且双真通过；门面方法转发与 C-4 纪律一致、语义无冲突；无回归新脆弱点。

**建议放行进入下一里程碑 M0.6**（Step 5：Remote 桥 spawn_remote，草案 §2/§4，计划 §Step 5）。

通过前无必须修复项（Major 0 / Minor 0）。3 项 Nit（草案测试 8 措辞 vs 实现序的文档注记、测试 9 命名 vs 实际 retire+settle 路径、m05 yield 轮询基数约定未统一注释/测试 8 未用 break-on-condition）记录在案，可在 M0.6 或后续小修中一并处理，不阻塞合入。
