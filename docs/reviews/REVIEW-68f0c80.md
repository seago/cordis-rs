# 代码审查报告：Phase 0 出口（协议单测补齐 + Spike S1–S3 + 出口判定）

- **审查对象**（3 个提交，按顺序）：
  1. `da2e795` — `test(async): 协议单测补齐——测试 6（panic 隔离）/7（快照纪律）`（仅 `crates/cordis-async/tests/protocol.rs` +263/-2）
  2. `68f0c80` — `feat(async): Phase 0 出口 spike S1-S3——事件总线自动退订 / tokio 服务 sync 壳 / agent loop flush（草案 §9）`（`crates/cordis-async/src/lib.rs` +11 / `tests/spikes.rs` +418）
  3. `ac7c52a` — `docs: Phase 0 出口判定——11 条协议单测 + 3 spike 全过，进入 Phase 1 决策`（`docs/cordis-async-PHASE0-EXIT.md` +49）
- **审查日期**：仓库时区（2026-08-18）
- **审查人**：independent-review-agent
- **审查范围**：协议单测补齐（测试 6 panic 隔离 / 测试 7 快照纪律）、3 项 spike 原型、Phase 0 出口判定文档；对照 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§1/§2/§3/§6/§8/§9、`docs/cordis-async-PHASE0-PLAN.md` §Step 6。上一里程碑结论：REVIEW-4f1e555（PASS WITH NITS，0 Major/1 Minor 于 `6247ff7` 落地）。
- **验证手段**：静态阅读（协议/计划/EXIT 蓝图 + 测试源码 + lib.rs 实现） + 实际运行全部工程门禁命令（见「验证记录」）。

**改动统计**：3 文件（code 2 + docs 1），+429/-2（不含 doc）。
- `protocol.rs` +263：新增 `m07` 模块——`PanicBehavior`/`PanicIter`（测试 6）、`SnapProviderBehavior`/`SnapConsumerBehavior`/`once_finished_async`/`async_inverse_snap`（测试 7）+ 2 个 `#[tokio::test]`；同时把 `m03::Shell::inject_only`/`provide_only` 及既有 `OneShotBehavior` 由 `fn` 升 `pub(super)`（`Shell::empty()` 已 pub(super)），供 `m07` 跨模块复用。
- `spikes.rs` +418（新文件）：`EventBus` 原型（`Symbol` 事件名 + `emit` 快照迭代 + id 退订）+ `SubBehavior`/`SubIter`（S1）；`SpikeShell` + `LlmShellBehavior`/`LlmIter`（S2）；`AgentBehavior`/`AgentIter`（S3）+ 3 个测试。
- `lib.rs` +11：`AsyncCx::effect` 转发 `Context::effect`（doc 注明 S1 订阅模式）。
- `docs/cordis-async-PHASE0-EXIT.md` +49：11 条协议单测 + 3 spike 出口对照 + 门禁记录 + 出口判定。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：3

Phase 0 出口的 3 个提交与草案 v1.4 §9、计划 §Step 6 逐条对齐。7 个核查要点全部核实通过：
- **测试 6（panic 隔离）**：`PanicIter` 的 future 内 panic —— 断言 `A` 条目状态保持 `Idle`（drive panic 不走 `on_failed`，`mark_running` 不会被调，故非 `Failed`）、邻组件 B 仍 `Active`（不级联）、测试继续执行（进程存活）；settle 后 `tail.handle.await` 得 `Err(JoinError)` → `eprintln!` 记录诊断 → slot 为空（panic 路径无 disposer）正常排空；最后 retire 两组件 + settle 断言 B 逆 `b:inverse` 正常收账。与 §3.3「panic = 宿主 bug，不进失败通道、不级联」、`lib.rs` settle 的 JoinError 诊断分支逐字吻合。
- **测试 7（快照纪律，评审点 C）**：消费者在**步执行期（Running）**经 `cx.get_cloned::<SnapKey>()` 读活 store、立即克隆 `Arc<String>` 释放借用、捕获 `snap` 进逆闭包；逆只读 `&**snap`、经 `yield_now` 后断言 `== "v"`，完全不触活 store。退役 `P`（级联卸载消费者）→ settle → 日志 `[p:run, c:run, snap:ok, p:rev]` —— `snap:ok`（卸载后从捕获 Arc 读到值）**先于** `p:rev`（提供者逆），I-3 序顺带直证。与 §3.2/§3.3「尾巴的数据访问（C-1' 快照纪律）」、`get_cloned` doc 的 Running 期语义一致。
- **S1 事件总线**：`cx.effect` 转发准确嵌入 core fiber ctx 累加器路径（`Context::effect` → `PushingIter` → `push_step` 把退订器推入本上下文 `dispose` 累加器 → fiber 卸载 `dispose_all` 运行 → 自动退订）；`emit` 快照迭代（Ref 借用以局部向量持有，handler 内退订/重入安全）+ async 监听器经 `spawn_local` 投递（不阻塞派发）。测试直证：`tick:hello` 收到、卸载 + settle 后 `tick:again` 不再收到。
- **S2 服务壳**：`#[test]` + 手动 `combo.block_on` + `LocalSet.run_until`（规避 async 上下文 drop runtime panic，注释明示）；`set_remote(TokioRemote)` → 同步壳 `LlmIter` 内 `spawn_remote` 提交 mock LLM 到 worker blocking pool → `join.await` 回灌 → `downcast::<String>` 断言；卸载 `retire` + `settle` 断言 `llm:rev` 收账；worker runtime 在 `block_on` 返回后 drop（此时远端句柄已释放，安全）。
- **S3 agent loop**：注册器模式一步 = 长驻循环，逆 = flush session（草案 §6）。循环在**每次 token 前检查 `cx.cancellation().cancelled()` 检查点**；mock SSE 流逐 token + `tool:` 前缀识别工具调用。卸载 `retire`（cancel）→ 检查点退出（`loop:exit@cancel`）→ 逆 flush（`flush:session` await 收尾）→ `is_quiet()` 真（无泄漏）；断言「flush 在末尾」「取消后不消费剩余 token `token:tool:send_email`」。「长驻行为经注册器模式表达为有限步」的草案 §6 语义完整落地。
- **出口判定文档**：11 条协议单测（§9#1–11）与代码一一对应（无夸大、无遗漏，逐项核名见下）；3 项 spike 各对应唯一测试；工程门禁记录与实际运行一致（21 条 = protocol 18 + spikes 3；workspace core 55+28、loader 49、hmr 9、wasm 全套）。
- **工程门禁**：全部实际运行通过（见「验证记录」），无 unsafe、`deny(missing_docs)` 全程生效、doc 0 告警。

---

## 发现

### Major：无

### Minor：无

### Nit-1（低）：EXIT 文档 S1 的「级联退订」措辞超过测试直证范围

- **位置**：`docs/cordis-async-PHASE0-EXIT.md` §2 的 S1 对比行「通过标准 = 订阅/退订/**级联退订**原型跑通」，以及 §4 结束语「S1 的 `AsyncCx::effect` 订阅入口……已获原型验证」；对应测试 `tests/spikes.rs::spike_s1_event_bus_subscription_auto_unsubscribes_on_unload`。
- **问题**：核查要点第 3 条要求核对 S1 「订阅/退订/级联退订」的直证性。实际测试只演示了**单 fiber** 的「订阅 (`sub:ok`) → emit 投递 (`tick:hello`) → 卸载自动退订 (`tick:again` 不再收到)」闭环，未显式覆盖「依赖图上的级联退订」（即当提供者退役、其依赖者随 Thm 63 级联卸载时，多个 fiber 的订阅各自退订）。机制层面二者共享同一条 `ctx.effect` → fiber ctx 累加器 teardown 路径，故**功能上不构成缺失**；但 doc 的「级联退订原型跑通」表述超出了该测试**直接**证明的字面范围。
- **草案依据**：草案 §8（订阅经 `ctx.effect` 注册 = 随 fiber 卸载自动退订）、计划 §Step 6 S1 通过标准（「订阅/退订/级联退订原型跑通」）。草案/计划本身即含此措辞，故是蓝图沿袭，非 EXIT 独有偏差。
- **建议**（可选，非阻塞）：EXIT（及可选地计划 §Step 6）把「级联退订」改为「订阅/卸载自动退订（级联路径共享同一 teardown 机制）」，或补一句「级联退订与单 fiber 退订共用累加器路径，spike 以单 fiber 直证、级联复用既有级联测试（如 m03 drain / m07 I-3）」。

### Nit-2（低）：S1 `drop(cx.effect(...))` 丢弃返回值的原因未在 spike 注明

- **位置**：`tests/spikes.rs` `SubBehavior::apply_async`（`drop(cx.effect(move || ...))`）；`lib.rs` `AsyncCx::effect`（返回 `Disposer`、doc 已注明 S1 订阅模式）。
- **问题**：`Context::effect` 返回一个 `Disposer`，手动调用它会 `armed.set(false)` + run composite。这里有意 `drop` 而不调用——因为「订阅」是一步 `once` 效应，其逆（`bus.unsubscribe(id)`）已经由 `ctx.effect` 内部的 `PushingIter` → `push_step` 推入 fiber ctx 累加器，卸载时由 `dispose_all` 执行，返回的 `Disposer` 只覆盖「手动撤销」路径，故不调用、直接 drop 属正确用法。但 spike 代码仅以中文注释「订阅经 cx.effect 注册：订阅立即生效，逆（退订）随 fiber 卸载执行」说明行为，**未解释为何丢弃返回的 `Disposer` 仍能退订**（累加器 vs 手动撤销双路径的关系）。对读者可能误以为退订依赖被 drop 的返回值。
- **草案依据**：草案 §8（订阅 = `ctx.effect` = 随 fiber 卸载自动退订）；core `Context::effect` doc（「返回 disposer 撤销本效应全部步骤」）。
- **建议**（可选）：在 `drop(cx.effect(...))` 处补一句注释：返回的 `Disposer` 会撤销整个效应，此处**有意不调用**——单步 once 的逆（退订）已推入 fiber ctx 累加器，卸载由 `dispose_all` 执行；drop 返回值仅使其不可手动调用，符合「自动退订、无需手工清理」的 DX 目标。

### Nit-3（低）：S3 `is_quiet()` 泄漏检查为注册器级静止判定，未断言 mock 会话态清空

- **位置**：`tests/spikes.rs` `spike_s3_agent_loop_flushes_session_on_unload` 末尾 `assert!(rt.is_quiet(), "S3：收账后静止（无泄漏）")`。
- **问题**：核查要点第 5 条要求核对「无泄漏（is_quiet）」。`is_quiet()` 判定的是「尾部队列空 ∧ 无仍 Active 的 async 组件」（注册器级）。它证明注册器角度无残留尾巴、无仍运行组件，是合理的「无泄漏」代理；但它**不检查** mock 内部是否还有未清空的 session/订阅状态（本例 `AgentBehavior` 无 bus，仅记日志，无实例化 session 容器，故实际无残留——断言与实现的「无泄漏」结论一致）。对 spike 原型足够，但可与 Nit-1 一并注明「is_quiet 是注册器级静止判定，非业务对象级清理断言」。
- **草案依据**：草案 §5 `is_quiet`（async 视图静止判定）、§9 spike 3 通过标准（「卸载时 flush 完整、无泄漏」）。
- **建议**（可选，非阻塞）：保持现状（spike 语义足够）；或补一句测试注释说明 `is_quiet` 的注册器级语义，避免日后误当业务级泄漏全量断言。

---

## 验证记录

以下命令均在 `/usr/local/work/cordis-rs` 实际运行：

| # | 命令 | 结果 |
|---|---|---|
| 1 | `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 test -p cordis-async` | ✅ `protocol.rs` **18 passed**（含 `m07::async_panic_is_isolated_not_failed_not_cascaded`、`m07::dependent_tail_reads_captured_snapshot_after_provider_gone`、`m06` 2 条、既有 14 条）；`spikes.rs` **3 passed**（S1/S2/S3）；doc-test 0。**合计 21/21** |
| 2 | `cargo +1.97.0 test --workspace` | ✅ 全绿（`cordis-async` 18+3；`cordis_core` lib 55 + 集成 28（access 4/check_in_place 5/failure_model 4/interception 5/preservation_recovery 3/progress_bound 1/property 2/update_binding 4）；loader 49；hmr 9；wasm 全套；无回归） |
| 3 | `cargo +1.97.0 clippy -p cordis-async --all-targets -- -D warnings` | ✅ `Finished dev profile`，**0 告警** |
| 4 | `cargo +1.97.0 fmt --check` | ✅ exit 0（workspace 格式干净） |
| 5 | `cargo +1.97.0 doc -p cordis-async --no-deps` | ✅ `Finished`，生成 index.html，**0 告警**（`deny(missing_docs)` 生效） |
| 6 | `grep -rn "unsafe" crates/cordis-async/src crates/cordis-async/tests` | ✅ 无 `unsafe` 用法（`unsafe_code=deny` 贯通） |

**文档对照核查**：EXIT §1 的 11 条协议单测清单与测试名逐一核对全符（I-1→`i1_composite_disposer_runs_lifo`；I-2→三者；I-3→`m03::i3_...`；I-4→`m04::i4_...`；drain→`m03` 二则；panic 隔离→`m07::async_panic_...`；快照→`m07::dependent_tail_...`；代次→`m05::update_...`；无环→`m05::retired_settled_...`；H 竞态→`m05` 二则；shutdown 一致性→`m04` 二则）。EXIT §2 三项 spike 与 `spikes.rs` 三个测试一一对应。EXIT §3 门禁数字与本次实际运行一致（cordis-async 21 = 18+3；core 55+28）。无夸大、无遗漏。

---

## 结论

**确认 Phase 0 出口成立**。11 条协议单测（草案 §9 #1–11）+ 3 项 spike（S1 事件总线自动退订 / S2 tokio 服务 sync 壳 / S3 agent loop flush）全部通过，工程门禁（fmt / clippy -D warnings / doc 0 告警 / workspace 无回归 / 无 unsafe / deny(missing_docs)）全绿，里程碑间独立审查门禁（M0.1–M0.6 = REVIEW-1005c8b/-91254a9/-83c254a/-596125d/-23383f3/-4f1e555）全部关闭，无未决 Major/Minor。审查结论 **PASS WITH NITS**（0 Major / 0 Minor / 3 Nit，均为文档措辞或注释精度层面的可选改进，不影响出口判定）。

**可进入 Phase 1 决策**（计划 §4.5 门禁）：满足「Phase 0 出口全过 + 出口走查无未解释偏差 + 与 dsh 工作区愿景对齐 + 固化 Phase 1 计划 + 用户确认」。建议用户参考 Nit-1/2/3 的可选改进，但与 `docs/cordis-async-PHASE0-EXIT.md` §4「进入 Phase 1 决策」结论一致，不阻塞推进。
