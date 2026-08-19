# 代码审查报告：commit `704a46c`（M1 wasm 桥专项 W2 · 端到端）

- **审查对象**：`704a46c` + `777f3e6` — `feat/fix(wasm): M1 wasm 桥 W2 端到端——guest 经 wit remote::submit 提交 + 宿主 WasmComponent::poll_remotes 驱动回填 + remote_e2e 测试（O-6 隔离断言）+ guest 单步探针`
- **审查日期**：2026-08-20
- **审查人**：independent-review-agent
- **审查范围**：`crates/cordis-wasm/src/lib.rs`（`poll_remotes`/`remote_results_debug` 公开驱动面）、`tests/remote_e2e.rs`（新）、`examples/wasm-plugin-rust/src/lib.rs`（guest 探针）；对照 W0 协议细化 + `docs/cordis-wasm-WASMREMOTE-PLAN.md` W2、`crates/cordis-wasm` 既有桥（W1/W1b：`configure_remote`/`register_remote`/`drive_poll_remote`/err 通道）。
- **验证手段**：静态阅读 + `cargo +1.97.0 test -p cordis-wasm`（全套绿，remote_e2e 0.38s 真实回填）；clippy/fmt 由委托方本地验绿。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：1（`poll_remotes` 在「激活内一步 done」场景是回填的唯一驱动面——doc 可补一句必要性）
- **nit**：3（时序措辞可再精确一档；轮询上限调度依赖注记；guest 未用 handle 的探针意图已注释）

W2（guest 端到端 + 宿主轮询驱动 + O-6 隔离断言）与 W0 协议/计划 W2 对齐：**「guest 提交 → 宿主注入 TokioRemote worker 执行 → 宿主 remote_result 轮询真实回填」链路直证**、O-6（worker 线程 ≠ 组合线程）实测断言成立、时序边界诚实明示（guest `take` 完整轮询回填留 M2 解锁）、既有回归（bridge_core/dependency/dual_backend/go_guest/isolated/load/sandbox）全套绿、core 零改动（本 commit 未触 `crates/cordis-core`）、API 均有 doc（`deny(missing_docs)` 满足）。

---

## 发现

### Major：无

### Minor-1（建议 doc 精化）：`poll_remotes` 在「激活内一步 done」场景是回填的**唯一**驱动面——必要性与生产语义可明示

- **位置**：`src/lib.rs` `pub fn poll_remotes`（doc「execute 之外也可驱动回填——组合线程在空闲/检查点调用，不阻塞」）。
- **问题**：本专项链路的事实是——core `execute` 同步一口气执行；guest 探针 `DbTask` **一步 `done`** → 激活后不再有后续 `WasmTaskIter::next` → W1b 内置的 `self.poll_remotes()`（next 头部）不再被驱动 → **提交的远端结果只能经显式 `poll_remotes` 回填**（测试正是靠它，否则 4000 次轮询后 expect 失败）。当前 doc 表述为「空闲/检查点调用」对**编排/测试**场景成立（也因此 W2 的暴露正确），但对生产形态（无后续 step 的 submit）建议明示「**一次激活内 submit 且无后续 step 时，回填须编排方显式 `poll_remotes`（或由宿主在组合线程定期驱动）**」——避免实现歧义（M2 步间驱动解锁后可减轻该义务）。
- **建议**：`poll_remotes` doc 补一句必要性说明（见上）；不阻塞。

### Nit-1：时序措辞可再精确一档（W2 记录「take 回填需两次驱动」已诚实，但可与 W1b 已落地机制对齐）

- `remote_e2e.rs` 头注：`handle.take()` 无法在单次激活内等到异步 worker、take 回填需两次驱动（M2 async 驱动解锁）——**正确**。可再精确：W1b 已让 `WasmTaskIter::next` 头部 `self.poll_remotes()`（回填在**下一次 step 驱动**时可被 guest take 读到）；M2 解锁的是「core `execute` 同步一口气」导致的**单次激活内**步间暂停（而非「回填全然不可达」）。建议措辞补一句「（W1b 已使连续/再次 step 驱动时可读回填；M2 解锁单次 execute 内步间时序）」。

### Nit-2：轮询上限 4000×1ms（4s）的调度依赖未注释

- 与 m06 同款经验：worker 毫秒级回填（实测 0.38s 即 break），4s 余量充足、非 flaky 主诉；建议注释「若极端调度慢导致 expect 失败，属调度依赖而非 flaky」即可。

### Nit-3：guest `_h`（submit 的 handle）未使用——探针意图已注释

- guest `let _h = … remote::submit(...)`——显式保留句柄、未 take（契约面存在；完整取回留 M2）。注释已言明，属刻意探针，无问题。

---

## 通过项（逐条确认）

- **guest 真实提交**：guest `DbTask::step0` 经 `cordis::core::remote::submit("echo", &[Value::Count(7)])` 真实调用 wit `remote` import（wit 含 `remote`：W1 落地，编译经 bindgen 产物确证）✓。
- **宿主注入与操作注册**：`comp.configure_remote(Some(TokioRemote::new(worker.handle())))` + `register_remote("echo", Arc::new(op → 返回 worker 线程 id))`——复用既有 `Remote`（TokioRemote）与 `Arc<RemoteOp>` 面（W-D1/W-D2 落地）✓。
- **真回填链路直证**：激活 execute（`WasmTaskIter` pump 提交 → `remote_joins`）→ 测试循环 `comp.poll_remotes()`（`drive_poll_remote` noop-waker 单次探测、Ready 回填 `remote_results`）+ `remote_result(0)` 至 `Some(Some(Ok(Text(tid))))` → **`assert_ne!(tid, combo_tid)` 直证 O-6 隔离**（worker 池线程 ≠ 组合线程）——真实 worker 提交→回填，非 stub ✓。
- **时序边界诚实**：测试头注明示「execute 同步一口气、guest take 单次激活内取不到、本测试以宿主 remote_result 断言真实链路、take 为接口面（编译/语义）」——**无夸大**（Nit-1 为措辞精化）✓。
- **既有回归**：guest 探针保持 `provide ["db"]`、step0 = db 绑定 + submit + done——`bridge_core`/`dependency_consumption`/`dual_backend`/`go_guest`/`isolated_wasm`/`load_guest`/`sandbox_isolation` 全套绿（实测）✓。
- **API 面**：`poll_remotes`/`remote_results_debug`（+既有 `remote_result`）均有 doc、非阻塞（noop-waker）；`deny(missing_docs)` 满足（`cargo test -p cordis-wasm` lib 6/6）✓。
- **777f3e6（clippy）**：去 unused `Remote` import（测试未直接调 `Remote` trait 方法，仅用 `TokioRemote` 具名类型）——1 行修正合理 ✓。
- **core 零改动**：`git show 704a46c 777f3e6` 未触 `crates/cordis-core` ✓。

---

## 验证记录（实际运行）

```
GOCACHE=.../gocache cargo +1.97.0 test -p cordis-wasm
  lib: 6/6 ok
  bridge_core: 2, dependency_consumption: 1, dual_backend: 2,
  go_guest: 2 (13.2s), isolated_wasm: 2, load_guest: 1,
  remote_e2e: 1 (0.38s, 真实回填), sandbox_isolation: 3
→ 全套绿，RC=0
```

## 结论

W2（guest 端到端 + 宿主轮询驱动 + O-6 隔离直证）与 W0 协议/计划对齐，真实「提交→worker→回填」链路成立、时序边界诚实、既有回归全绿、core 零改动、API 文档齐。**建议放行进入 W3**（清理/退役语义——未决请求句柄回收 + 沙箱/双后端回归 + `cordis-async` `WasmRemote` 占位 doc 按 W-D1 重定位）。4 项发现（0 Major/1 Minor/3 Nit）为 doc/措辞级，不阻塞；Minor-1（`poll_remotes` 必要性 doc）建议 W3 一并补。
