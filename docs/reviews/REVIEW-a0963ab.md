# 代码审查报告：commit `a0963ab`（M1.2 订阅/派发核心，Phase 1）

- **审查对象**：`a0963ab6c1b1010bd7d8ca8c817114ba37b963ae` — `feat(events): M1.2 订阅/派发核心——on/on_waterfall/on_serial/on_bail + emit/waterfall/serial/bail + 冲突检测四规则 + release-then-invoke + 幂等 disposer + tests/ 验收 #1/#2/#5/#6/#8（Phase 1）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show a0963ab`（`crates/cordis-events/src/lib.rs` +441 / `tests/events.rs` +260），对照冻结草案 `docs/cordis-events-protocol-draft.md` v0.3.1（§1/§2/§3.2/§6）与计划 `docs/cordis-events-PHASE1-PLAN.md` Step 1。
- **验证手段**：静态阅读 + 实际运行 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 test -p cordis-events`、`cargo +1.97.0 doc -p cordis-events --no-deps`、`cargo tree -p cordis-events`。clippy/fmt 由委托方声明本地已绿（本审查不重复）。
- **改动统计**：2 文件，+704/-46。lib.rs：实现注记（Arc 存储 + tombstone）、`Mode`/`ModeSpec`/`ModeRecord`/`ListenerEntry`/`EventBus{modes,listeners}`、`on/on_waterfall/on_serial/on_bail`（统一 `register` + 四规则冲突检测）、`emit/waterfall/serial/bail`（release-then-invoke + `check_dispatch_r` + `snapshot_reply` + `waterfall_link`）、`disposer`（alive AtomicBool 幂等）；tests/events.rs 10 测试（#1/#2/#5/#6×4/#8/waterfall 基础/Arc 捕获）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（E-1「派发中退订本轮不再调用」的 alive **调用期**检查未实现——快照仅 collect 期过滤、不带 alive 标志，与冻结稿 §2.2 E-1 及验收 #7 冲突；不影响本里程碑验收（#1/#2/#5/#6/#8 全过），但 **M1.4 落 #7 前必修**）
- **nit**：2（`check_dispatch_r` 以 `r_name != "()"` 字符串哨兵判断「模式有无 R」（`R=()` 边缘误判）；验收 #2 双重 dispose 受 Rust `FnOnce` 局限未直测——注释已诚实声明，M1.3 以双路径 armed 直证）

M1.2 订阅/派发核心与冻结稿 v0.3.1 整体一致：四订阅 API（方案 2 拆分 + R 上界 `Send+Sync`）、四派发（序/返回/短路/E-2）、冲突检测四规则（含跨模式载荷一致性 Minor-2、双方类型名诊断）、release-then-invoke + tombstone 复用、`RwLock` 单表 + `Arc<dyn Fn+Send+Sync>` 存储（Send+Sync 成立链完整）、零第三方（`cargo tree` 仅 cordis-core）。实现注记对 `Arc` 存储与冻结稿 `Box` 示意的偏离给出了充分理由（release-then-invoke 快照需可复制句柄）。测试直证充分、注释诚实。

---

## 发现

### Major：无

### Minor-1（建议，必修于 M1.4 前）：E-1「派发中退订本轮不再调用」的 alive **调用期**检查缺失——快照不带 alive 标志

- **位置**：`crates/cordis-events/src/lib.rs` `emit`（快照 `filter(|e| e.alive())` + `filter_map` 收集 `Arc::clone(f)`）、`snapshot_reply`（同）、`waterfall`（同）——快照只存 **闭包 Arc**，不存 alive 引用。
- **问题**：草案 §2.2 E-1（冻结稿第 132-135 行）明示「派发**开始**时快照监听器集合，**并附各监听器的 alive 标志**」且「派发中退订的监听器本轮**不再调用**（alive 检查，评审 m-3 明确）」。当前实现仅在 **collect 期**按 alive 过滤（collect 前已退订者不收录）；collect **之后**（派发进行中、其他监听器闭包内 dispose 某已入快照的监听器）被退订者**仍会被调用**——与草案「派发中退订本轮不调用」冲突。验收 #7（「派发中退订的监听器本轮跳过、后续不再触发」）据此**无法成立**（M1.4 计划验收）。
- **草案/计划依据**：草案 §2.2 E-1（附 alive 标志 + alive 检查）、验收 #7；计划 Step 3（M1.4）以 #7 为验收。
- **建议**：派发快照改为收集 `(alive: &Arc<AtomicBool>, f: 闭包 Arc)` 的元组，**调用前** `if alive.load(Ordering::SeqCst) { f(..) }`（或 collect 时检测 + 派发中途若持有 Arc<AtomicBool> 引用亦检查）。此改动局部（emit/waterfall/snapshot_reply 三处 snapshot 收集 + 派发循环），建议**就地修入本里程碑**或 M1.4 前置清单显式标注（#7 依赖）。本里程碑验收（#1/#2/#5/#6/#8）不受影响，故不判 Major。

### Nit-1（低）：`check_dispatch_r` 以 `r_name != "()"` 字符串哨兵判断「模式有无 R」

- **位置**：lib.rs `check_dispatch_r`（`rec.r_name != "()"`）。
- **问题**：以 unit 类型名 `"()"` 作"无 R"哨兵脆弱：若用户合法订阅 `on_serial::<P, ()>`（R = unit，`r_name == "()"`），则该模式被误判"无 R"而跳过派发侧校验——订阅 `()` 而派发 `u32` 时不能触发优雅的「派发 R 不符」panic（退化为 downcast `expect` panic，诊断信息不同且歧义）。R = unit 场景罕见，但哨兵设计不干净。
- **建议**：`ModeRecord` 以显式 `has_r: bool`（或 `r_type_id: Option<TypeId>`）标记 Emit/Waterfall 无 R、Serial/Bail 有 R；`check_dispatch_r` 依标志决定是否校验。一行级改动。

### Nit-2（低）：验收 #2「双重 dispose」在 `Box<dyn FnOnce>` 帧下未直测

- **位置**：tests/events.rs `disposer_idempotent_double_dispose`。
- **问题**：core `Disposer = Box<dyn FnOnce()>` 单路径下，`on()` 返回的 disposer 调用一次即 move，无法编译第二次调用——「重复 dispose 无害」在 Rust 的等价语义（**多路径等价闭包共享 armed 句柄**）需 M1.3（`ctx.effect` 注册逆 + 手动 disposer 双路径）才能真测。测试注释已诚实声明此局限与后续直证路径。
- **建议**：记录即可，不要求本里程碑补（M1.3 验收 #3 附带覆盖 armed 双路径）。符合草案「自研 armed 同款语义」的实现方向（`alive: AtomicBool` 置位型幂等已实现）。

### 未发现问题的核查点（逐条确认）

- **§2.1 订阅 API**：`on`/`on_waterfall`/`on_serial`/`on_bail` 签名与草案逐字一致（方案 2 拆分；R 上界 `Send+Sync+'static`；监听器闭包 `Send+Sync`）；返回 `Disposer` ✓。
- **§2.2 四派发**：emit 注册序无返回 ✓；waterfall `&mut` 载荷 + `terminal` + `waterfall_link` 递归（around、短路、最内层 terminal）✓；serial 收集 `Vec<R>` 序 ✓；bail 首个 `Some` 即停、全 `None` 得 `None` ✓；E-2 空集四断言 ✓。
- **派发侧 R 一致性（m-3'）**：`check_dispatch_r` 对 Serial/Bail 校验、无订阅视为空集不 panic ✓（Nit-1 除外的哨兵实现）。
- **§3.2 结构**：`RwLock` 单表 `modes`（含类型名）+ `listeners`（注册序 Vec）✓；`Arc<dyn Fn+Send+Sync>` 存储理由充分（release-then-invoke 快照可复制）✓；`alive: AtomicBool` 幂等退订 + tombstone 复用（表 ≤ 峰值活跃订阅）✓；Send+Sync 成立链（闭包→ListenerEntry→maps→RwLock→EventBus→Arc）完整 ✓；锁序（modes.write→listeners.write；派发只见 listeners.read/modes.read 独立）无死锁 ✓。
- **冲突检测四规则**：同名同模式载荷 ✓；同名同模式异 R ✓；**跨模式载荷一致性（Minor-2）** ✓（modes 的 None 分支遍历同 Symbol 全键）；派发侧 R ✓；诊断含双方类型名（`ModeRecord` 携 `type_name`/`r_name`）✓。
- **release-then-invoke + E-1 注册侧**：派发快照在锁内 collect、释放锁后调用闭包（闭包内重入 on/emit 不死锁）✓；派发中注册本轮不触发（快照已 collect）✓；**唯派发中退订的调用期检查未实现（Minor-1）**。
- **实现注记/偏差**：`Arc` 存储（vs 草案 Box 示意）、`ModeSpec`/`ModeRecord` 类型名携、内部 `EmitAnyFn`/`WaterfallAnyFn`/`ReplyAnyFn` 擦除——均为必要实现细节且在 crate doc 注记，非偏离冻结稿语义 ✓；`Default::default() = new()` 语义保留（REVIEW-85d2379 nit-1 已处理）✓；waterfall `terminal` 即时传入（无 Send+Sync 约束，不进存储）与草案一致 ✓。
- **测试直证性**：#1 序 ✓；#5 serial/bail 全语义 ✓；#6 四规则各 `should_panic`（含跨模式、派发 R）✓；#8 E-2 四断言 ✓；waterfall 基础链（A→B→terminal + around）为 #4 预告 ✓；Arc 捕获验证 §0 义务 ✓。直证充分。
- **工程门禁**：`#![deny(missing_docs)]` 生效（全部 pub 项有 doc）；无 unsafe；`cargo tree` 仅 `cordis-core`（零第三方，计划 §5 纪律）✓。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`。

1. `cargo test -p cordis-events` — **PASS**，10/10（integration tests/events.rs：emit_order_and_payload、disposer_idempotent_double_dispose、serial_collects_all_and_bail_stops_on_first_some、conflicts::{same_name_different_payload,same_mode_different_r,cross_mode_different_payload,dispatch_r_mismatch}、empty_listeners_e2、waterfall_basic_link_order、listener_captures_arc_not_rc）。
2. `cargo doc -p cordis-events --no-deps` — **PASS**，0 告警。
3. `cargo tree -p cordis-events` — 仅 `cordis-core`（零第三方确认）。

---

## 结论

M1.2（Step 1：订阅/派发核心）与冻结稿 v0.3.1 §2/§3.2、计划 Step 1 整体一致，冲突检测四规则、release-then-invoke、tombstone 复用、Send+Sync 成立链、零第三方等关键点全部核实通过，无逻辑/编译缺陷，M1.2 范畴验收（#1/#2/#5/#6/#8）直证充分。

**建议放行进入下一里程碑 M1.3**（Step 2：订阅即效应集成，验收 #3），但须将 **Minor-1（E-1 alive 调用期检查，阻塞 #7）** 纳入跟进：建议**就地修入 M1.2 收尾**或明示列入 **M1.4 前置必修清单**（M1.4 验收 #7 依赖其成立）。Nit-1/Nit-2 记录在案，不阻塞。

通过前建议：处理 1 项 Minor（快照带 alive + 调用前检查，一行级）+ 1 项 Nit（`has_r` 显式标志替代字符串哨兵）= 可选合一 commit；均不阻塞合入。
