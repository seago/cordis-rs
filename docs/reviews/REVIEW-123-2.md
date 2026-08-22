# 代码审查报告：backlog 清理线② Await 判据 v2 启用（Gate B 走查）

- **审查对象**：backlog ②（`Step::Await(Option<判据>)` 载荷 + `Runtime::poll_ready` + wasm 桥精确驱动）——commits `1beaa9e`（code）/ `1ea0885`（docs/THEORY-MAP）。
- **验证手段**：静态阅读 + `cargo +1.97.0 {fmt --check, clippy --workspace --all-targets -- -D warnings, test --workspace, doc --workspace --no-deps}`（GOCACHE 设 target/gocache）。
- **范围**：计划 `docs/cordis-BACKLOG-123-PLAN.md` §② ↔ 交付 ↔ 门禁；判据与挂起上下文一致性 / wasm 多 fiber 独立 / wit·ABI 零改动 / THEORY-MAP 行格式 / EXIT 声明复核。

---

## 总体结论

**❌ FAIL（阻断）**——机制设计正确、测试全绿，但 **Gate A 被破坏：`clippy --workspace --all-targets -- -D warnings` 失败**（`clippy::type_complexity`，`crates/cordis-core/src/effect.rs:98`）。EXIT §2（`docs/cordis-BACKLOG-123-EXIT.md`）声称"clippy `-D warnings` 0"与实际不符；该 lint 由本线新增的 `try_execute_with` Err 三元组判据成员引入（`<judge>` 使返回类型越过复杂度阈值）。必须修复后方可判定出口成立。

- **Must-fix（阻断）**：1 —— clippy `-D warnings` 失败（type_complexity）。
- **Minor**：2 —— M-1 EXIT 门禁声明不准确 / M-2 判据运行期借用纪律仅文档化（见下）。
- **Nit**：2 —— judge 单次评估不重入复核 / wasm 桥 `Next` 守卫 `awaiting` 分支与 `remote_joins` 判定的时序。

## 逐项验证

### 1. core（crates/cordis-core/src/{effect,fiber,context,runtime}.rs）—— 全对

- **Step 载荷形态**：`Await(Option<Box<dyn Fn() -> bool>>)`（effect.rs:52），文档注明"纯就绪检查（不得调度）、随挂起上下文存亡"✅。
- **try_execute_with Err 三元组**：`Result<Disposer, (Box<dyn EffectIter>, Vec<Disposer>, Option<Box<dyn Fn() -> bool>>)>`（effect.rs:98–104）✅ 形态正确。
- **Resumable 三元组**：`(Box<dyn EffectIter>, Vec<Disposer>, Option<Box<dyn Fn() -> bool>>)`（fiber.rs:146–150）✅。
- **advance 三路径**（runtime.rs:313–343）：
  - `Ok(disposer)` → 折叠逆入 `dispose` + notify（330–334）✅；
  - `Err((iter,acc,judge))` → 判据随本次 Await **更替**写入 resumable（336–341）✅；
  - 旧判据经 `.take()` 解构即释放（317 `_judge` drop）——advance 进入即无 stale judge ✅。
- **advance 判据更替正确性**：再挂起用新 Await 的 judge；无"旧判据残留窗口"✅。
- **reload/unload 判据释放**：reload 用全新 `apply` 迭代器（571）——先经 unload 收账 resumable 的保证成立（见下）；unload `resumable.borrow_mut().take()`（628）释放 judge、acc 补入 dispose（LIFO）✅。L-Raise 路径（reload 576–588 捕获 → `self.unload(fiber)` 584）在 Inactive 前经 unload 收账 → 无泄漏 ✅。
- **更新路径保证 reload 时 resumable=None**：Active 纤维 refresh（runtime.rs:457–461）先 L-Unload → unload（628 take resumable）→ `Some(_)` 惯性 reload（649）；Inactive 纤维本无 resumable；`entry` 只驱动快照成员且 `_=>false` 兜底（390–392）✅ —— m-2 注记（runtime.rs:320–322）成立，无重复判据/丢挂起 acc。
- **poll_ready 语义**（385–398）：`is_some_and` 内借 `resumable` 评估 judge（388–392），**借用随闭包返回释放后才 `advance`**（394–395）✅；`Await(None)`/缺 fiber → `_=>false` 不受驱动 ✅；快照单遍、无多余语义（advance 自身处理 guard/再挂起）✅。
- **PushingIter 判据透传**：`Await(judge) => Await(judge)`（context.rs:746）✅。

### 2. 关键语义（判据与挂起上下文一致性）—— 全对

- advance 完成/再挂起判据正确更替 ✅；unload 收账释放 ✅；L-Raise 经 unload 无泄漏 ✅；poll_ready 对"满足但 advance 后 guard 失效/再挂起"无多余语义（advance 自处理）✅。
- **M-2（Minor，文档化纪律）**：judge 在 `resumable.borrow()` 持借期间运行（runtime.rs:388）——judge 不得重借本 fiber resumable / 不得对自身 advance（否则 RefCell panic）。wasm judge 只读组件 `inflight/state/store`（lib.rs:789–796），不触 fiber 内部，故实际安全；但此约束仅以注释纪律、无编译期强制，属可接受的文档化边界（与既有 `set_with_check` 重入注记同型）。

### 3. wasm（crates/cordis-wasm/src/lib.rs）—— 全对

- **inflight 登记**：`pump_remotes` 只登记 `drive_pump_remote` 返回的 `submitted`（646）——仅 `(Some(op),Some(remote))` 分支入 submitted（683），**立即 err 的 `(None,_)`/`(_,None)` 分支直接写 results 不登记**（686–698）✅。
- **判据剪枝**：judge `inflight.retain(|rep| !results.contains_key(rep))` + `inflight.is_empty()`（794–795）——落位即淘汰 ✅。
- **awaiting 分支构造判据**：`Wait` 且 `remote_joins` 非空 → `Await(Some(judge))`（778–797）✅；`poll_and_advance` = `poll_remotes` + `poll_ready`（441–444）✅。
- **同组件多 fiber 互不阻塞**：inflight 在 `apply` 内每激活新建（513），即每 fiber 独立追踪，无跨 fiber 阻塞 ✅。
- **wit/ABI 零改动**：`1beaa9e` 仅动 core（context/effect/fiber/runtime）+ wasm lib.rs，无 `.wit`/`wit/` 改动（`git diff f40fff9..1ea0885 -- '*.wit' 'wit/*'` 为空）；判据载荷在宿主翻译层构造不经 wit ✅。
- **Nit（wasm `awaiting` 守卫时序）**：`awaiting` 在 step 后判定 `remote_joins` 非空（780）——若本步同时提交新远端 join 又到 `Wait`，则进入 Await(判据)，正确；仅在 `Wait` 且已有在途 join 时挂起。判据评估阶段 poll_ready 用 `poll_remotes` 先回填，故 awaited 判据能看到落位 —— 时序闭环 ✅（Nit：单行内联首次访问 `remote_joins` 与 pump 顺序依赖 step 语义，注释已点明）。

### 4. 门禁实测（GOCACHE=target/gocache）

- `cargo +1.97.0 fmt --check` ✅ 0
- `clippy --workspace --all-targets -- -D warnings` ❌ **失败**——`error: could not compile cordis-core (lib)`，`warning: very complex type used. Consider factoring parts into type definitions`，位置 `crates/cordis-core/src/effect.rs:98:6`（`try_execute_with` 返回类型）。
- `cargo +1.97.0 test --workspace` ✅ **CARGO_EXIT=0**，`FAILED` 计数 0；52 个套件全 ok（cordis_core 62、a2_e2e 3、wasm_agent 2、go_guest 2、full_stack 1、dual_backend 2、remote_e2e 2、loader 60、events 14、hmr 10、protocol 21、spikes 3 等）。
- `cargo +1.97.0 doc --workspace --no-deps` ✅ 0（grep warning/error 无匹配）。

### 5. THEORY-MAP backlog ② 行（docs/THEORY-MAP.md:172）—— 格式正确

- 原始管道计数 8，但 2 个来自代码 span 内转义 `\|_\|`（`advance_suspended(\|_\| true)`）；按"忽略转义 `\|` 后分割"得 **6 逻辑管道**（=表头 line 8 与相邻 B-A1/P-3 行 line 169/170 的 6）——**与相邻行同列数、单元格对应表头 5 列**，无表格断裂 ✅（REVIEW-123-13 曾修的 P-3/P-7 行断裂，本行未复现）。
- 行内容：`Step::Await` 判据载荷 + `poll_ready` + `Await(None)` 兼容 + wasm 桥接线，授权"用户 2026-08-22 backlog 决策 ②-1/②-2"，§4.3.3/Def 51，扩展（产品层，授权），记录 —— 与 B-A1/P-3 行风格一致 ✅。

## 发现

### Must-fix（阻断，Gate A）

**Must-1：clippy `-D warnings` 失败（type_complexity）**
- 位置：`crates/cordis-core/src/effect.rs:98`（`try_execute_with` 返回类型）。
- 根因：backlog ② 给 Err 三元组新增判据成员 `Option<Box<dyn Fn() -> bool>>`，使 `Result<Disposer, (Box<dyn EffectIter>, Vec<Disposer>, Option<Box<dyn Fn() -> bool>>)>` 越过 `clippy::type_complexity` 阈值（`-D clippy::type-complexity` implied by `-D warnings`）。
- 修复：按代码库既有风格（`pub(crate) type Resumable = (...)`）引入**类型别名**，如 `type AwaitErr = (Box<dyn EffectIter>, Vec<Disposer>, Option<Box<dyn Fn() -> bool>>);`（或别名 `type AwaitJudge = Box<dyn Fn() -> bool>;` 供 Resumable/try_execute_with 复用），解除复杂度告警；避免 `#[allow(type_complexity)]`（掩蔽式）。

### Minor

**M-1（EXIT 门禁声明不准确）**：`docs/cordis-BACKLOG-123-EXIT.md` §2 声称"clippy `-D warnings` 0"——实际失败（Must-1）。须在判据修复后复核并将实测写入 EXIT（同 P-6 M-2 处理方式）。

**M-2（判据运行期借用纪律仅文档化）**：judge 于 `resumable.borrow()` 持借期运行（poll_ready 388），judge 不得重借本 fiber / 对自身 advance —— 仅注释 + wasm judge 只读外部表；建议在 effect.rs:51 或 poll_ready 文档中明确"judge 不得触本 fiber resumable"的禁止面（现仅"纯检查不调度"措辞）。

### Nit

- **N-1**：judge 单遍快照评估，未对"评估后到 advance 前判据由真转假"重入复核——poll_ready 只比一次，正确性依赖 advance 内部 guard（target.is_some()），非判据；与文档"advance 自身处理"一致，属可接受简化。
- **N-2**：wasm `awaiting` 判定（lib.rs:780）内联访问 `remote_joins` 与 pump 顺序依赖 step 语义；注释已点明，非缺陷。

## 最小修复清单（委派方，未自行改动代码）

1. **Must-1（阻断）**：给 `crates/cordis-core/src/effect.rs` 的 `try_execute_with` Err 类型引入类型别名（或复用 `AwaitJudge`/`AwaitErr`），使 `clippy --workspace --all-targets -- -D warnings` 归 0；同步核对 `Resumable` 三元组一致性。
2. **M-1**：修复后复核 clippy 实测为 0，更新 `docs/cordis-BACKLOG-123-EXIT.md` §2 门禁声明。
3. **M-2（可选）**：`effect.rs` 判据纪律措辞补"不得触本 fiber resumable / 不得对自身 advance"。

**总结**：backlog ② 的机制实现、语义一致性、wasm 桥接线、THEORY-MAP 行格式均正确，测试全绿；**唯一阻断为 Gate A 的 clippy `-D warnings` 失败**（Must-1，本线引入的 type_complexity）。修复 Must-1 并更新 EXIT 门禁声明后，本线可判定出口成立（产品验证线 §3 的 backlog ②"判据 v2 启用"验收项方可闭合）。
