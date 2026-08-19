# 代码审查报告：commit `dadc512`（P1.4 DX-1/DX-2，cordis-rs）

- **审查对象**：`dadc512e4517e2bdd79720cacefda211100bc7ad` — `docs+demo(async): P1.4 DX-1/DX-2 插件作者指南 + 错误/安静语义 + plugin_template 示例（事件订阅+agent-loop+flush）（P1.4）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show dadc512`（`docs/cordis-PLUGIN-GUIDE.md` +88 / `docs/cordis-ERRORS-QUIET.md` +52 / `crates/cordis-async/examples/plugin_template.rs` +185），对照冻结草案 v1.4（async §3.3/§4、事件 §2.2）与已审查实现（cordis-{core,async,events,loader}）。
- **验证手段**：静态阅读 + 实际运行 `cargo run -p cordis-async --example plugin_template`（PASS，完整日志）+ `cargo run -p cordis-async --example async_combo`（回归 PASS）；实现关键断言逐条 grep 核对。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（PLUGIN-GUIDE §9 引用不存在的 `docs/cordis-PLUGIN-TEMPLATE-*.md` 文件——broken doc reference）
- **nit**：1（plugin_template 的 `worker` runtime 创建但未接入 Remote——冗余演示）

DX 文档与已审查实现语义**一致、无夸大、无引入新语义**：值纪律（C-1/C-1'）、绑定 vs 资源（C-2）、门面纪律（C-4 + AsyncFiberHandle 弱引）、事件订阅即效应（ctx.effect 落账 + 卸载自动退订 + 双路径 armed + ctx 语义）、监听器 Send+Sync（§0 核心义务）、Remote 两形态 + O-6、错误/安静语义（Failed→静止→自退役 disabled→复活、panic 隔离、is_quiet/shutdown 双真/settle 64 轮守卫）均与实现逐点相符；事件层错误语义（panic 传播/符号冲突/派发 R）正确。plugin_template 可运行且形态完整（事件订阅 + agent-loop cancel 检查点退出 + flush 收账，日志断言齐全）。修复 1 项 broken doc 引用即可闭合。

---

## 发现

### Major：无

### Minor

### Minor-1（建议修复）：PLUGIN-GUIDE §9 引用不存在的 `docs/cordis-PLUGIN-TEMPLATE-*.md`

- **位置**：`docs/cordis-PLUGIN-GUIDE.md` §9「组合示例」第 2 条——「事件订阅 + async 监听器 + agent loop 模板见 `docs/cordis-PLUGIN-TEMPLATE-*.md` 与 P1.4 示例插件」。
- **问题**：`docs/` 下**无** `cordis-PLUGIN-TEMPLATE-*.md` 文件（实测 `ls docs` 仅见 `cordis-PLUGIN-GUIDE.md`）；P1.4 实际交付的模板是 `crates/cordis-async/examples/plugin_template.rs`。指南指向不存在的文档文件，读者按图索骥会落空——**文档与交付不一致**（broken reference）。
- **建议**：把该行改为直接指向 `crates/cordis-async/examples/plugin_template.rs`（代码即模板），或补一份 `docs/cordis-PLUGIN-TEMPLATE.md`（若意图是文字说明版）。改动一行即可闭合。

### Nit

### Nit-1（可选）：plugin_template 的 `worker` multi_thread runtime 创建但未接入 Remote

- **位置**：`examples/plugin_template.rs` `main()` 开头 `let worker = tokio::runtime::Builder::new_multi_thread()...build()` 与尾部 `let _ = worker;`。
- **问题**：全示例**未调用 `rt.set_remote(...)` / `spawn_remote`**——`worker` 仅是创建后立即弃用（`let _ =`）。示例聚焦「事件订阅 + agent-loop + flush」，Remote 回路已由 `async_combo` 展示——`worker` 属冗余演示（无功能影响，`let _ = worker;` 吸收 unused 警告）。
- **建议**：删除 `worker` 创建与 `let _ = worker;`（示例不需要），或在文件头注释「Remote 回路见 async_combo；本模板聚焦事件 + agent-loop + flush」。可选。

### 未发现问题的核查点（逐条确认）

- **值纪律 C-1/C-1'**：Arc 惯例（`Key::Value: Send+Sync` 强制）、快照纪律（步创建时捕获 Arc 克隆、尾巴不读活 store、`get_cloned` 立即克隆释放借用）——与 async 约定 C-1/C-1'、实现 `AsyncCx::get_cloned`（lib.rs `self.ctx.get...map(clone)`）一致 ✅。
- **C-2（绑定 vs 资源）**：`set` 只放服务绑定（逆 sync 卸载）；async 资源走 async 步 + `AsyncDisposer`——与实现（`AsyncCx::set` 透传 core、settle 收 AsyncDisposer）一致 ✅。
- **C-4（门面纪律）**：`use_component`/`retire`/`update` 走 AsyncRuntime 门面（AsyncFiberHandle 弱引）、直接 core sync API 允许但不 settle 记账、显式 settle（O-2 决策）——与 P1.2 H1/H2 实现与 crate doc 一致 ✅。
- **订阅即效应**：`ctx.effect` 落账（fiber 卸载自动退订）、双路径 armed（disposer + 累加器逆至多一次）、`ctx` 应为订阅者 fiber 上下文（传根 ctx 需手动 dispose）——与 events `subscribe*`（lib.rs `ctx.effect(once(...))` 实现）与 REVIEW-f8541f1 nit-3 doc 一致 ✅。
- **事件监听器约束**：`Send + Sync + 'static` 上界、不得捕获 Rc（§0 核心义务）、O-6' 线程私有总线——与 events 实现（20 处 `Send + Sync + 'static`）一致 ✅。
- **Remote 两形态 + O-6**：闭包（`boxed`/`From` → `spawn_blocking`）+ Send-future（`from_future` → `handle.spawn`）、O-6 边界、panic=bug 诊断——与 P1.3 R1 实现（RemoteRequest 双变体 + 双形态调度 + `await_remote_join`）一致 ✅。
- **错误/安静语义**（ERRORS-QUIET）：Failed → 静止终态 + 自退役（`on_failed` → `fiber.retire()`，异步 lib.rs:578-585）+ loader `retire_hook` 写回 `disabled=true`（loader lib.rs:479/592）+ 复活（重启用重建换代）；失败路径 slot 留空（`on_failed` 不写槽）；panic 隔离（测试 6 直证）；`is_quiet` = 无尾巴 ∧ 无 Active（lib.rs:898）∧ Failed 静止 + 合取限定；shutdown 双真（lib.rs:933-934）；settle FIFO + 64 轮守卫（lib.rs:407）——与草案 §3.3/§5 及实现全部一致 ✅。
- **事件层错误**：listener panic 传播（panic=bug）、符号冲突（同名异载荷/同模式异 R/跨模式异载荷）订阅 panic + 类型名诊断、派发 R 不符派发 panic——与 events 冲突检测实现一致 ✅。
- **plugin_template 可运行性与形态**：实测 `cargo run` PASS，日志序 `["tick:7","token:user:你好","token:tool:get_weather","loop:exit@cancel","flush:session"]`——事件订阅收到 → agent 注册器循环处理 mock 流 → 退役 cancel → 检查点退出 → flush 收账完整直证；无限循环靠 cancel 退出（S3 已验证形态），无死锁（settle await handle 收 drive 的 Finished）。`async_combo` 回归 PASS（R3 未破坏）✅。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo`。

1. `cargo run -p cordis-async --example plugin_template` — **PASS**（打印「P1.4 插件模板通过：事件订阅 + agent-loop + 卸载 flush」+ 完整日志序）。
2. `cargo run -p cordis-async --example async_combo` — **PASS**（回归：R3 组合示例 hits=7）。
3. 实现断言 grep 核对（drive LIFO:110、on_failed→Failed:578/582 + retire、is_quiet:898、shutdown 双真:933、MAX_DRAIN_ROUNDS=64:407、events Send+Sync ×20、subscribe ctx.effect、loader disabled 写回:479/592）— 全部命中。
4. `ls docs` — 确认无 `cordis-PLUGIN-TEMPLATE-*.md`（Minor-1 依据）。

---

## 结论

P1.4 DX-1/DX-2（插件作者指南 + 错误/安静语义 + plugin_template 示例）**达成且语义准确**——文档与已审查实现（cordis-core/async/events/loader）逐点相符，无夸大、无引入新语义；plugin_template 可运行、形态完整（事件订阅 + agent-loop cancel 检查点退出 + flush 收账）。

**建议放行进入 P1.4 出口走查**；通过前建议处理 1 项 Minor（修 PLUGIN-GUIDE §9 的 broken doc 引用：指向 `examples/plugin_template.rs` 即可）——1 项 Nit（worker 冗余）记录在案，可选处理。
