# 出口走查报告：cordis-async P1.2（H1–H3）

- **审查对象**：`docs/cordis-async-PHASE1-P2-EXIT.md`（ae5fb0b）对照 `crates/cordis-async`（H1 `61f78b8`/REVIEW-fa44fd6、H2 `23b75fa`/REVIEW-23b75fa、H3 `aa346d2`/REVIEW-aa346d2，全部已 PASS）
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent（P1.2 出口走查）
- **验证手段**：静态阅读 + 实测 `GOCACHE=.../gocache cargo +1.97.0 test -p cordis-async`

---

## 总体结论

✅ **通过（PASS）** — P1.2 出口成立，EXIT 文档与代码真实一致，无夸大 / 无遗漏 / 无未解释偏差。

- **major**：0
- **minor**：0
- **nit**：2（可读性，不阻塞）

## 核实记录

### H1 Handle 迁移完整性（EXIT §1）— 核实通过

| 项 | 核实 |
|---|---|
| `AsyncFiberHandle { fiber: Weak<Fiber>, generation: u64 }` | ✅ lib.rs:688-690，`new` 用 `Rc::downgrade`（弱引防环，评审点 B） |
| `use_component -> Result<AsyncFiberHandle, RegistryError>` | ✅ lib.rs:775；内部同步激活后捕获 `entry.generation.get()` 为审计代次 |
| `retire(&AsyncFiberHandle)` / `update(&AsyncFiberHandle, config)` | ✅ lib.rs:819/839；`upgrade().expect("句柄失效=宿主 bug")` |
| **无残留 `Rc<Fiber>` 门面签名** | ✅ grep `pub fn…Rc<Fiber>` 仅 `Handle::fiber() -> Option<Rc<Fiber>>`（705）——是**带警示 doc 的瞬时查询访问器**，非门面方法签名；门面全部走 Handle |
| generation 审计语义（换代不失效） | ✅ doc 注记 + REVIEW-fa44fd6 Minor-1 回写（防串代由条目内部代次承担）；`m05::async_fiber_handle_generation_and_id`（protocol.rs:1208）直证 |
| `fiber()`/`id()`/`generation()` | ✅ 均有 doc（fiber() 含「不长期持有强引」警示，REVIEW-fa44fd6 nit-2 落地）；`id() -> Option<FiberId>` 配合 `AsyncRuntime::entry` 查询 |

语义保证（仅签名迁移、对应状态查询方法有警示）成立；settle/shutdown/is_quiet/entry/自登记未触碰（本轮 grep 核对承诺范围）。

### H2 门面纪律 + 开放项决策（EXIT §2）— 核实通过

- crate doc `## 门面纪律（契约 C-4，P1.2 H2 文档化）`（lib.rs:12-18）
- **O-2**：`保持显式 settle()`（lib.rs:23-25，框架层不提供自动封装，草案决议采纳）✅
- **O-3**：`不启用` core hook + 「若 C-4 频繁违反再启用」睛述（lib.rs:26-28，REVIEW-23b75fa 措辞对齐）✅
- **O-4**：`AsyncFiberError` 保持 `String`（lib.rs:29-30）✅
- 与 EXIT §2、计划 §1 D-1/D-2/D-3/D-4 一致 ✅

### H3 Remote API 冻结（EXIT §3）— 核实通过

- `Remote`/`RemoteJoin`/`RemoteValue`/`RemoteRequest`/`TokioRemote` 文档均含「API 冻结（P1.2 H3）」标注（lib.rs:170/176/183/211/213）；`TokioRemote` 含「冻结声明」（227，REVIEW-aa346d2 nit-1）——生命周期 / O-6 语义复核完整，P1.3 以新增表述变体扩展、不破坏既有签名 ✅

### 测试适配与既有语义（EXIT §1 末）— 核实通过

- 抽查 `protocol.rs`：`rt.retire(&provider)`（406）、`rt.retire(&a/c/handle)`、`rt.update(&fiber, config)`（987）——全部走门面 Handle 形态；`m04` loader 路径（`loader.fiber(...).id()`）不动 ✅
- 实测 `cargo +1.97.0 test -p cordis-async` = **19 + 3 = 22/22**（protocol 19 + spikes 3），与 EXIT「22 条」精确一致 ✅
- （门禁 fmt/clippy/doc 由 H1–H3 审查各自实测，本轮以 test 回归为主复核）

## 发现

### Major：无
### Minor：无

### Nit（不阻塞，可读性）

- **Nit-1**：部分测试变量仍命名 `fiber` 而实为 `AsyncFiberHandle`（如 protocol.rs:1062/1135 `rt.retire(&fiber)`）——行为正确，命名与句柄语义略不符；可改 `handle` 提升可读性（非必须，避免过度 churn）。
- **Nit-2**：`AsyncFiberHandle::generation()` 当前无门面消费点（仅 m05 测试断言直证语义）；系审计元数据 / 未来防串代诊断用——doc 已注明「审计元数据」，可接受；留待 P1.3 若产生真实消费点再定。

## 结论

**P1.2 出口确认成立**：Handle 门面收口（语义等价迁移、无 `Rc<Fiber>` 门面签名残留）、C-4/O-2/O-3/O-4 决策落地与记录、Remote API 冻结标注、门禁全绿（cordis-async 22/22）、H1–H3 里程碑审查闭环——EXIT 文档与代码逐条一致，无夸大 / 无遗漏 / 无未解释偏差。

**照准进入 P1.3 决策**（Remote 扩展 + 双运行时收口：Send-future 分池形态 / WasmRemote 接入范围 / 共存文档化；详细计划开工前起草，按纪律由用户下达）。
