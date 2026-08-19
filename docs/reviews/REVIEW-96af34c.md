# 代码审查报告：commit `96af34c`（M1 wasm 桥 W1a，WasmRemote 专项）

- **审查对象**：`96af34cb01fe346ce5f3e990d5727c36a9e0f33f` — `feat(wasm): M1 wasm 桥 W1a——wit remote 接口（submit/take/handle）+ bindgen + Host stub（W1b 填驱动）`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent（被委派独立审查）
- **审查范围**：`crates/cordis-wasm/wit/cordis.wit`（+12：`remote` interface + world `import remote`）+ `crates/cordis-wasm/src/lib.rs`（+30：`remote::Host`/`HostHandle` stub），对照 `docs/cordis-wasm-WASMREMOTE-PROTOCOL.md` §2（协议细化稿）与 `docs/cordis-wasm-WASMREMOTE-PLAN.md` §1 W-D1..D4。
- **验证手段**：静态阅读 + 实际运行 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0 build -p cordis-wasm`（bindgen 编译通过）+ `cargo +1.97.0 test -p cordis-wasm`（既有 wasm 套件全绿）。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：0
- **nit**：3（`todo!()` 无上下文 panic 消息；协议稿可显式补「submit 立即返回句柄、错误延迟到 take」的异步契约一句；W2 需端到端验证 `Value` 标量参数往返——非本 commit 范围）

W1a（协议接口面 + bindgen + Host stub）与 W-D1..D4、协议细化稿 §2 完全一致，**bindgen 生成签名与 stub 精确匹配（编译通过）**，world `import remote` 正确，**既有 guest（rust/go）不受 import 新增影响（13 条 wasm 测试全绿）**，无新攻击面引入（W1a 仅接口/stub，`todo!` 不触达）。可放行 W1b。

---

## 发现

### Major：无

### Minor：无

### Nit

### Nit-1（低）：`submit` stub 的 `todo!()` panic 无上下文

- **位置**：`lib.rs` `remote::Host::submit` — `todo!("W1b：注册表 + 注入 Remote 提交 + 句柄登记")`。
- **问题**：若某调用方在 W1a 阶段意外触达（当前无——既有无 guest import remote，测试未触达），panic 消息无操作名/参数上下文，调试可辨性差。纯占位（W1b 填充前不应有调用方），低影响。
- **建议**：可在 W1b 前保留；若想更稳，用 `unimplemented!("W1b：submit({name:?}, {params:?}) 尚未实现")` 携带上下文。可选。

### Nit-2（低）：协议稿可显式补一句「submit 立即返回句柄、错误延迟到 take」的异步契约

- **位置**：`docs/cordis-wasm-WASMREMOTE-PROTOCOL.md` §2。
- **问题**：`submit(name, params) -> handle` **无错误返回**——未知名操作/错参无法在 submit 报错，只能由句柄 `take` 的 `err(string)` 通道承载（审查任务第 2 点确认）。协议稿§2「未知名/错参 → 句柄 err(...)」已隐含，但「submit 恒成功返回句柄、错误一律延迟到 take」的**异步契约**未显式条文化。
- **建议**：协议稿 §2 补一句「`submit` 恒返回句柄（异步契约：不等待、不在 submit 报错）；未知名操作/参数错误经 `take` 的 `err` 通道承载」——W1b 实现与 W2 错误测试的落地依据。

### Nit-3（低，范围外观察）：`Value` 标量参数/结果的往返需 W2 端到端验证

- wit `Value` 为标量集（flag/count/offset/text/blob）；`params: list<value>` 与结果 `value` 经 `Value`↔`RemoteValue` 适配器（W-D4）——**W2** 端到端测试应覆盖标量往返（仿 m06 的 42 断言）。非本 commit 范围，仅记录衔接。

---

## 未发现问题的核查点（逐条确认）

- **wit 与协议稿 §2 逐字一致**：`interface remote { use context.{value}; resource handle { take: func() -> option<result<value,string>>; } submit: func(name:string, params: list<value>) -> handle; }`、world `import remote;`——与 `WASMREMOTE-PROTOCOL.md` §2 逐 token 相符 ✓。
- **bindgen 签名一致**（核心）：build 通过——`remote::Host::submit -> Resource<Handle>`（**无 `wasmtime::Result` 包裹**，与 take 错通道承载设计一致）、`HostHandle::take -> Option<Result<Value, String>>`、`HostHandle::drop(self, Res) -> wasmtime::Result<()>`——stub 与生成 trait 精确匹配 ✓。
- **handle 资源 drop = 清理钩子**：Component 模型资源销毁为隐式（host 析构层调 `HostHandle::drop`）——stub 返回 `Ok(())`（W1b 填句柄表清理：guest 弃句柄/实例卸载）✓；wit 无需显式 `drop: func()`（正确）。
- **submit 无错误返回的协议含义**：未知名操作无法在 submit 报告 → 句柄 `take` 的 `err` 承载（审查任务点 2 确认与 W-D2/细化稿一致）——W1b 需为未知名造「立即 err 的句柄」（take=Some(Err(msg))）✓ 已记录为 W1b 交接。
- **沙箱（W1a 面）**：`import remote` 仅能力扩展；guest 任意 name/params → W1b 注册表未知名 → `err` 而非 panic 宿主；参数 `Value` 强类型（wit 编译期）——W1a 无实现 → **无新攻击面** ✓。
- **既有 guest 兼容**：rust/go guest 均不 import remote → 无需重编；实测 wasm 套件 **13/13 全绿**（bridge_core 2 / dependency_consumption 1 / dual_backend 2 / go_guest 2 / isolated_wasm 2 / load_guest 1 / sandbox_isolation 3）✓。
- **范围克制**：W1a 仅协议接口面 + stub（`todo!` 占位，注明 W1b）；未触碰 cordis-core（core 零改动纪律）✓。

---

## 验证记录（实际执行）

统一前缀 `GOCACHE=/usr/local/work/cordis-rs/target/gocache cargo +1.97.0`：

1. `git show 96af34c` — 2 文件 +42 行（wit +12 / lib.rs +30）。
2. `cargo build -p cordis-wasm` — **PASS**（bindgen 生成 + stub 签名匹配）。
3. `cargo test -p cordis-wasm` — **PASS**，13/13（7 套件全绿；go_guest 12.62s 含 go 工具链）。
4. `git diff 96af34c^ 96af34c -- crates/cordis-core` — 空（core 零改动）。

---

## 结论

W1a 全部核查通过：wit `remote` 与协议细化稿 §2 逐字一致、bindgen 签名精确匹配、world import 正确、既有 guest 不受影响、沙箱面无新攻击面、core 零改动、范围克制。3 项 Nit 均为文档/占位可辨性级，不阻塞。

**建议放行 W1b**（宿主驱动）：注册表（`register_remote(name, Fn(Vec<Value>)->RemoteValue+)`）+ 注入 `cordis_async::Remote`（v1 TokioRemote）提交 + 句柄结果登记（take 按句柄取/未知名立即 err）+ `Value`↔`RemoteValue` 适配器 + 宿主单测（fake Remote 执行、回填时序、未知名 err）。W1b 应带 Nit-2 的协议句条文化落地。
