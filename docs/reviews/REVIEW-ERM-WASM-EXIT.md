# 出口走查报告：M1 wasm 桥专项（WasmRemote 宿主驱动）

- **评审对象**：`docs/cordis-wasm-WASMREMOTE-EXIT.md`（2026-08-20，W4 出口判定）
- **评审日期**：2026-08-20
- **评审人**：independent-review-agent（对照 `crates/cordis-wasm`、`crates/cordis-async`、`examples/wasm-plugin-rust` 既有实现与 W1a–W3 审查记录）
- **评审范围**：决策落地（W-D1..D4）/ 里程碑对照 / 门禁回归数字 / 时序边界诚实性 / 出口判定
- **验证手段**：静态阅读 + grep 代码事实 + `cargo +1.97.0 test -p cordis-wasm`（全套，含 go_guest/sandbox/dual_backend）+ `doc --workspace --no-deps`（0 告警）

---

## 总体结论

**✅ PASS — M1 wasm 桥专项（WasmRemote 宿主驱动）出口成立**

- **Major**：0
- **Minor**：0
- **Nit**：1（EXIT §3 集成测试计数 13 → 实测 14，数字口径微调）

## 发现

### Nit-1（数字口径）：EXIT §3「cordis-wasm lib 7 + 集成 13」—— 实测集成为 **14**（lib 7 + 集成 14 = 21 全绿）

- **位置**：EXIT §3「`cargo +1.97.0 test --workspace` ✅ 无回归（cordis-wasm lib 7 + 集成 13 …）」。
- **实测**（`cargo +1.97.0 test -p cordis-wasm` 逐文件）：
  - lib（unittests）**7**（W1b 5 + op_panic + Host drop 清槽 = 7）
  - 集成 8 文件 = **14**：bridge_core 2、dependency_consumption 1、dual_backend 2、go_guest 2、isolated_wasm 2、load_guest 1、remote_e2e 1、sandbox_isolation 3
- **判定**：纯数字口径（14 vs 13 差 1，可能为 remote_e2e 前某轮记录）；所有测试全绿、无一处 FAILED——不影响出口成立。建议 EXIT 数字改为「lib 7 + 集成 14」。

## 通过项（逐条确认）

### 决策落地（EXIT §1 ↔ 代码）
- **W-D1**：guest `examples/wasm-plugin-rust` 依赖仅 `wit-bindgen`（无 cordis-async，no_std 同步 step）——`remote::submit` 真实调用（lib.rs:64）；`WasmRemote` 占位 doc 已重定位为协议接线注记（cordis-async lib.rs:939-957 "重定位（决策 W-D1）…guest 不实现 Remote…宿主注入 TokioRemote"）✓
- **W-D2**：`pub type RemoteOp`（lib.rs:116，Fn(Vec<Value>)->Value + Send+Sync）+ `WasmComponent::register_remote`（:347，Arc 存储跨 worker）✓
- **W-D3**：`drive_pump_remote`（:574）/`drive_poll_remote`（:615，noop-waker 非阻塞）——组合线程不阻塞（O-6）；时序边界在 §4 明示 ✓
- **W-D4**：m-1 err 通道——`panic_payload_to_string`（:707）+ `RemoteValue` 载荷为 `Box<Result<Value,String>>`（op 显式 err + panic 兜底经 err 回填，组合线程零 panic）✓

### 里程碑与审查（EXIT §2）
- REVIEW-96af34c（W1a 协议面）/ REVIEW-f883492（W1b 宿主驱动）/ REVIEW-704a46c（W2 端到端）/ REVIEW-501c0a1（W3 清理+回归）全部存在于 `docs/reviews/`，均 PASS ✓

### 门禁与回归（EXIT §3）
- `doc --workspace --no-deps` **0 告警**（本文复跑，含 cordis-core/cordis-wasm 私链清零——纯文档文案，core 零语义改动）✓
- `cargo +1.97.0 test -p cordis-wasm` **lib 7 + 集成 14 全绿**（remote_e2e 真实回填 0.39s + go_guest 14.25s + sandbox 1.26s；无一失败）✓；clippy/fmt 由父会话验绿；workspace 整体回归父会话已确认（WS=0）✓
- 专项测试面：guest 提交→worker→回填（worker tid ≠ 组合线程 O-6 实测）；未知操作/未配置→句柄 err；op panic→err 兜底；Host drop 清槽；sandbox 回归 ✓

### 时序边界（EXIT §4）——诚实、无夸大
- core `execute` 同步一口气（无步间暂停）→ guest `handle.take()` 单次激活内取不到异步结果——陈述与实现相符（`execute` 为 `loop { guard; next }`，无外部再驱动面）；
- **真实回填走宿主 `poll_remotes`**（组合线程检查点 noop-waker 驱动）+ `remote_result`——提交→worker→回填链路真实（remote_e2e 直证）；
- guest `take` 契约为接口面（wit 编译——guest 含 `Handle`/`take` 调用编译通过即证）；M2 异步驱动后解锁完整 join 等待语义——表述准确、无夸大 ✓

## 结论

**M1 wasm 桥专项（WasmRemote 宿主驱动）出口确认成立**：wit `remote` 协议面 + 宿主驱动（注入 Remote 执行、注册表、回填、错误通道、panic 兜底）+ guest 真实提交端到端 + 清理语义 + 沙箱/双后端回归 + 门禁全绿（lib 7 + 集成 14）+ 审查闭环（0 Major 未决）。草案 v1.4 三形态 Remote 的 wasm 端落地成立，时序边界作为 M2 解锁项诚实记录。

1 项 Nit（EXIT 计数 13→14）不阻塞——建议随并入条修正。评审人：**照准出口**；后续（Phase 2 / wasm 桥异步化 M2 等）按纪律由用户下达。
