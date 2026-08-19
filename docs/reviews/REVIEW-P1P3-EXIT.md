# 代码走查报告：cordis-async P1.3 出口（Remote 双形态 + WasmRemote 边界 + 双运行时收口）

- **走查对象**：`docs/cordis-async-PHASE1-P3-EXIT.md` ↔ 代码（R1 `281c6ac` / R2 `42c1edc` / R3 `9af13e6` 已逐一审查 PASS，REVIEW-281c6ac / -42c1edc / -9af13e6）
- **走查日期**：2026-08-19
- **走查人**：independent-review-agent
- **范围**：EXIT §1 交付表逐条对证（R1 双形态 / R2 WasmRemote 边界 / R3 收口）+ 单测 24/24 + 组合示例 + 零第三方 + 决策 D-1/D-2/D-3 记录 + 门禁。

---

## 总体结论

✅ **通过（PASS）** — P1.3 出口成立。

- **major**：0　**minor**：0　**nit**：2（可选）

EXIT 文档与代码逐条对证**无夸大、无遗漏、无未解释偏差**；实测 `test -p cordis-async` 24/24（21+3）、`cargo run --example async_combo` 打印通过（hits=7）、`cargo tree` run 分支仅 `cordis-core`+`tokio`——与 EXIT 记录精确一致。

---

## 证据核验

| EXIT 交付 | 代码核验 |
|---|---|
| RemoteRequest 双变体（Closure \| Future） | ✅ `RemoteRequestInner` enum（`Closure(Box<dyn FnOnce … + Send>)` \| `Future(Pin<Box<dyn Future<Output=RemoteValue> + Send>>)`）；`boxed`/`From`（闭包态）+ `from_future`（future 态，含阻塞/CPU vs 非阻塞异步分工 doc） |
| TokioRemote 双形态调度 | ✅ `submit` match：`I::Closure → spawn_blocking` / `I::Future → handle.spawn`（multi_thread 池）；两路经 `await_remote_join` 回灌（远端 panic = 宿主 bug） |
| **submit 冻结签名保持** | ✅ `submit(&self, req: RemoteRequest) -> RemoteJoin<RemoteValue>` 不变（P1.2 H3 冻结；扩展在内部变体） |
| WasmRemote 边界（D-2） | ✅ 占位类型 `WasmRemote { _private: () }` + M1 协议接线 doc（guest 无自发线程、submit=入队+宿主 step 边界驱动+回填、M1 专项 `impl Remote`）；**无构造入口是刻意**（REVIEW-42c1edc nit-1：接入 host 协议前构造无意义，doc 已明示） |
| 双运行时收口（D-3） | ✅ `docs/cordis-async-THREADING.md`（进程拓扑图：组合线程 LoadLocalSet↔卫星运行时；C-3/O-6/C-1 边界）+ `examples/async_combo.rs`（143 行，loader 挂 EventsProvider + use_component + `spawn_remote(RemoteRequest::from_future)` 回路 + retire/settle） |
| 零第三方 run-deps | ✅ `cargo tree -p cordis-async --depth 1` run 分支 = `cordis-core` + `tokio`（events/loader 在 dev-dependencies） |

**单测（实测）**：`test -p cordis-async` = protocol **21** + spikes **3** = **24/24** 全绿。
- R1 新测试直证（抽查）：
  - `send_future_submits_to_worker_pool_and_joins_back`：`from_future(async { 6*7 })` → submit → join.await → downcast `42`（提交 + join 回灌直证）。
  - `send_future_executes_on_worker_pool_not_combo_thread`：future 内取 `thread::current().id()` → `assert_ne!(worker_tid, combo_tid)`（O-6 隔离直证）。
- 闭包形态（m06 既有）回归不破坏（21 条含 m06 全过）。

**组合示例（实测）**：`cargo run -p cordis-async --example async_combo` → 打印 `R3 组合示例通过：sync 树(loader)+async 层(use_component)+Remote 回路共存，hits=7`（最终断言式打印，运行期为真通过）。

**决策记录**：D-1（双形态，future=非阻塞 / 闭包=阻塞·CPU，分工 doc）/ D-2（接入点 + 协议 doc，宿主桥留 M1 wasm 专项）/ D-3（拓扑文档 + 组合示例）——与 EXIT §4 及计划 §1 默认一致。

**门禁**：EXIT §3 记录（fmt / clippy `-D warnings` / doc 0 告警 / workspace 无回归 / 审查闭环 0 Major 0 Minor 未决）——本走查实测 test/tree/example 相符；clippy/fmt/doc 由 R1–R3 各里程碑审查已验，未重跑。

---

## 发现

### Major：无

### Minor：无

### Nit（可选）

### Nit-1：EXIT §2「protocol 21 + spikes 3 = 24/24」的构成未列明 R1 两条新增在 21 之内（推断即达，无歧义，纯表述）。

### Nit-2：`async_combo` 示例以最终断言式打印（hits=7）而非 panic-style 断言——运行输出即验收，形式合理；若日后示例改输入，打印的魔力数需同步（低风险）。

---

## 结论

`docs/cordis-async-PHASE1-P3-EXIT.md` 与代码逐条对证一致，无夸大/遗漏/未解释偏差；实测 24/24 测试、组合示例运行、零第三方 run-deps 与 EXIT 记录精确匹配。**P1.3 出口确认成立**。

**照准进入 P1.4 决策**（DX 文档：插件作者指南、错误/安静语义、示例插件模板；纯文档线，按纪律由用户下达开工）。
