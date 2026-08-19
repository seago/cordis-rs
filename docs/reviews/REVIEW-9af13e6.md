# 代码审查报告：commit `9af13e6`（P1.3 R3 双运行时共存收口）

- **审查对象**：`9af13e66cba46f51638cdd51b477b648bbd5e397` — `feat(async): P1.3 R3 双运行时共存收口——线程拓扑文档 + async_combo 组合示例（sync 树+async 层+Remote 回路）（P1.3）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`git show 9af13e6`（`docs/cordis-async-THREADING.md` 拓扑文档 +69、`crates/cordis-async/examples/async_combo.rs` 组合示例 +142、`Cargo.toml` dev-deps +1 `cordis-events`、`Cargo.lock` +1），对照 P1.3 计划 §2 Step R3（按 D-3 默认）与草案 v1.4 §4 / 契约 C-3 / O-6。
- **验证手段**：静态阅读 + 实际运行 `cargo run -p cordis-async --example async_combo`、`cargo +1.97.0 test -p cordis-async`、`cargo tree -p cordis-async`。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：2（示例未实操事件订阅/派发，仅挂 EventsProvider 作 sync 树代表；示例 hits 断言用 `>=7` 而非 `==7`）

R3（按 D-3：拓扑文档化 + 组合示例）达成：文档与草案 §4/C-3/O-6 逐点一致，示例真实跑通「sync 树（loader/EventsProvider）+ async 层（use_component 组件 + spawn_remote）+ Remote（worker 池）」三端共存回路并收账，零第三方 run 保持。可放行 P1.3 出口走查（R4）。

---

## 证据核验

| 检查点 | 核验 |
|---|---|
| 进程拓扑（单组合线程 + LocalSet + 卫星运行时） | ✅ 与草案 §4 拓扑一致；C-3（唯一组合线程、违反=panic）如实 |
| sync 树 + async 组件同 loader 树共享 store | ✅ 与实现一致（realm 键控、依赖解析/级联/退役走 core、I-3 免费获得）；EventsProvider 根条目（P1.1） |
| AsyncCx 视图边界表（get_cloned/set/spawn_remote/cancellation） | ✅ 与 AsyncCx API 逐一相符（teardown 窗口 C-1' 快照纪律正确） |
| Remote 两形态（闭包 spawn_blocking / Send-future handle.spawn） | ✅ 与 P1.3 R1 实现一致；WasmRemote M1 接入点（R2 边界） |
| O-6 桥政策 + store 值纪律（C-1 Arc/Send+Sync） | ✅ 与草案一致（spawn_bridge 逃生口、监听器/Remote 载荷 Send+Sync 上界） |
| 组合示例回路真实 | ✅ **`cargo run` 实测打印「R3 组合示例通过…hits=7」**：EventsProvider 挂载 → use_component + `RemoteRequest::from_future`（Send-future）→ join 回灌 7 → retire（门面 C-4）+ settle 收账；断言 + 收账完整，非玩具 |
| **零第三方** | ✅ `cargo tree -p cordis-async` run 分支仅 `cordis-core` + `tokio`；`cordis-events`/`cordis-loader` 全在 dev-dependencies（Cargo.toml dev-deps 新增 `cordis-events`） |
| 测试 | ✅ `cargo +1.97.0 test -p cordis-async` = 21（protocol）+ 3（spikes）= **24/24** 全绿 |

---

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1（可选）：示例未实操事件订阅/派发——仅挂 `EventsProvider` 作 sync 树代表

- **位置**：`examples/async_combo.rs`——`loader.apply(EventsProvider)` 后无订阅/emit 实操；`docs/cordis-async-THREADING.md` §2 提「事件订阅经 `ctx.effect` 随 fiber 卸载自动退订」。
- **问题**：示例聚焦「sync 树 + async 层 + Remote」三端，事件总线作为 sync 组件代表**挂载**进树，但未演示订阅/派发/退订回路——拓扑文档 §2 的对应承诺未在示例内直证（P1.1 自有 `m13` 测试已直证订阅/退订语义，此处仅属「组合示例未带事件实操」）。
- **修法**：可选。在示例加一段事件订阅（EventsProvider 挂载后 `submit_serial`/`emit` 回路 + retire 后不再到达）即可补全演示价值；或 doc 注明「事件实操直证见 P1.1 tests」。不阻塞。

### Nit-2（可选）：hits 断言 `>=7` 而非 `==7`

- **位置**：`examples/async_combo.rs` 轮询 guarded `if *hits.read().unwrap() >= 7` + 后置 `assert_eq!(..., 7)`。
- **问题**：轮询条件用 `>=7`（防多自旋读）配合最终 `==7` 断言——语义正确（ComboIter 单步 `done` 标志，drive 仅一次，hits 恰为 7），但 `>=` 的「为什么可能多」未注释（实为双保险写法）。
- **修法**：可选。加一行注释（单步 iter + 恰一次回灌 → 恒 7；`>=` 仅为轮询防抖）。不阻塞。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache`。

1. `cargo run -p cordis-async --example async_combo` — **PASS**，stdout：`R3 组合示例通过：sync 树(loader)+async 层(use_component)+Remote 回路共存，hits=7`。
2. `cargo +1.97.0 test -p cordis-async` — **PASS**，21（protocol）+ 3（spikes）= 24/24。
3. `cargo tree -p cordis-async` — run 分支仅 `cordis-core` + `tokio`（+pin-project-lite/tokio-macros 传递），无 `cordis-events`/`cordis-loader`（dev）。
4. `git show 9af13e6` — diff 仅 4 文件（文档 + 示例 + dev-dep + lock），无对 `src/lib.rs` 的改动（R3 是纯收口文档/示例，无行为变更）。

---

## 结论

P1.3 R3（双运行时共存收口，按 D-3 默认：文档化 + 集成示例）**达成**：拓扑文档与草案 §4/C-3/O-6 逐点一致、无 doc 漏洞；`async_combo` 示例真实跑通三端共存回路（sync 树挂载 + async 组件 + Remote Send-future 回灌 + 退役收账），`cargo run` 实测打印通过；零第三方 run 纪律保持（events/loader 仅 dev-deps）；24/24 测试全绿。2 项 Nit 记录在案、不阻塞。

**建议放行 P1.3 出口走查（R4）**（双形态语义完整 + WasmRemote 边界 D-2 + 收口文档/示例与实现一致 + EXIT 文档）。
