# 代码审查报告：commit `501c0a1`（M1 wasm 桥专项 W3 · 清理语义 + 占位重定位）

- **审查对象**：`501c0a1` + `2753b1e` + `562c3a3`
- **审查日期**：2026-08-20
- **审查人**：independent-review-agent
- **审查范围**：`crates/cordis-async/src/lib.rs`（WasmRemote 占位 doc 重定位）、`crates/cordis-wasm/src/lib.rs`（`poll_remotes` 必要性 doc + `host_handle_drop_clears_result_slot` 单测）、`crates/cordis-core/src/runtime.rs`（历史 broken rustdoc 私链清零——纯文档文案）；对照 W0 协议（W-D1）+ 计划 W3 + 反查实现。
- **验证手段**：静态阅读 + `git show`/`git diff` 逐条核对 + `cargo +1.97.0 test -p cordis-wasm`（全套实测）。

---

## 总体结论

✅ **通过（PASS）**—— 0 Major / 0 Minor / 2 Nit

W3 三项交付（清理语义测试、占位 doc 重定位、doc 归零）+ 回归全部达成；`cordis-core` 改动经 `git diff` 核对为**逐字纯文档文案**（`[Self::…]` 私链 → 纯文本），零语义、零代码。

---

## 核查要点

| 项 | 核对 | 判定 |
|---|---|---|
| **WasmRemote 占位 doc 重定位（W-D1）** | 新 doc：guest 不跑 cordis-async / 不实现 `Remote`；经 wit `remote` import（submit=入队+宿主 step 边界驱动+`take` 回填）由 cordis-wasm 承接；宿主侧注入既有 `Remote`（TokioRemote）worker 执行（O-6）。**与实现逐点吻合**：`wit/cordis.wit` `import remote;`（:52）、Host `submit`(:207)/`take`(:221)/`drop`、`configure_remote`(:342)/`register_remote`(:347)/`drive_poll_remote`(:615)、guest `cordis::core::remote::submit`（guest lib.rs:64）——无夸大、无新承诺 | ✅ |
| **清理语义** | 新增 `host_handle_drop_clears_result_slot`：drop 句柄 → 结果槽清除（guest 弃句柄/实例卸载不残留）；驱动完成即弃（W1b `drive_poll_remote` removes done）、`remote_e2e` 退役后 `is_quiet`——三处合围无残留路径 | ✅ |
| **poll_remotes 必要性 doc** | REVIEW-704a46c Minor-1 落地：注明「core `execute` 同步一口气 + 单步组件一步 done → poll_remotes 是单步激活回填唯一驱动面；多步/长驻由迭代器步界轮询」——消除了语义歧义 | ✅ |
| **core doc 归零** | `2753b1e`/`562c3a3` 逐字核对：仅删除 `[Self::unload]`/`[Self::reload]`/`[Self::refresh]`/`[Self::reload]` 的链接语法（→ 纯文本），**无任何代码/语义改动**（`git diff` 仅注释行）；仓库「broken rustdoc links 归零」纪律达成 | ✅ |
| **回归** | `cargo +1.97.0 test -p cordis-wasm` 全套实测：**lib 7 + 集成 13 全绿**（含 go_guest 13.26s、dual_backend、isolated_wasm、sandbox_isolation、remote_e2e 0.40s 真实回填）——沙箱/双后端/go guest 不破坏 | ✅ |
| **语义边界** | 占位 doc 保留 `_private: ()` 无构造入口（REVIEW-42c1edc nit-1 承接）；W1-W2 已审（REVIEW-96af34c / REVIEW-f883492 / REVIEW-704a46c）与本 W3 的时序边界说明（take 契约面 vs M2 两次驱动解锁）前后一致 | ✅ |

---

## 发现

### Major：无

### Minor：无

### Nit

### N-1（低，可选）：`register_remote` 的 `Arc<RemoteOp>` 闭包允许捕获 `Rc`（panic 诊断 `panic_payload_to_string` 已在 worker 内捕获，无泄漏面）——文档可明示「op 应避免捕获组合线程 `Rc`（跨线程经 `Arc`）」，与 O-6 措辞强化（不影响正确性——`RemoteRequest::boxed` 的 `Send` 上界编译期已拦截非 Send 捕获）。

### N-2（低，可选）：`host_handle_drop_clears_result_slot` 直接经 `HostHandle::drop` 显式调用；真实 guest 弃句柄路径（组件模型资源隐式 drop）由 wasmtime 资源表驱动——语义一致但未直接覆盖隐式路径（W2 端到端已有退役清理断言，可接受）。

---

## 结论

**W3 达成**：清理语义（drop 清槽 + 完成即弃 + 退役静止）落地并测试直证；WasmRemote 占位按 W-D1 重定位（doc 与实现一致、无夸大）；历史 doc 私链归零（core 纯文案）；沙箱/双后端/go guest 全回归绿。

→ **建议放行 W4**（专项出口走查 + `docs/cordis-wasm-WASMREMOTE-EXIT.md`）。2 项 Nit 记录在案，不阻塞。
