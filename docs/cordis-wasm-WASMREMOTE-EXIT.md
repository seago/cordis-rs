# M1 wasm 桥专项出口判定（WasmRemote 宿主驱动）

**依据**：计划 `docs/cordis-wasm-WASMREMOTE-PLAN.md`（W0–W4 + 决策 W-D1..D4）；协议细化 `docs/cordis-wasm-WASMREMOTE-PROTOCOL.md`；草案 v1.4 §2/§4（WasmRemote 接入点）。
**判定日期**：2026-08-20（W1a–W3 审查闭环 + W4 出口走查）。
**出口标准**（计划 §4）：guest 提交 → 宿主 worker 执行 → 回灌断言直证（O-6 隔离）+ 错误通道 + 清理语义 + 沙箱/双后端回归 + workspace 无回归 + 出口走查。

---

## 1. 决策落地（W-D1..D4）

| 决策 | 落地 |
|---|---|
| W-D1 WasmRemote 职责重定位 | guest **不实现 Remote**（不跑 cordis-async）：远端请求经 wit `remote` import（submit=入队、宿主驱动、take 回填）；宿主侧注入既有 `Remote`（v1 `TokioRemote`）worker 执行（O-6）；`WasmRemote` 占位 doc 重定位为接线注记 |
| W-D2 guest 载荷 | submit(name, params: list<value>)——操作注册表 `register_remote`（Arc<RemoteOp>，Send+Sync 跨 worker） |
| W-D3 回填时序 | 提交句柄 + take 轮询；宿主 `drive_poll_remote`（noop-waker 非阻塞，组合线程不阻塞）；**时序边界**见 §4 |
| W-D4 值传递 | 载荷/结果全走 wit `value`；`RemoteValue`（Box<Result<Value,String>>）传输容器（m-1：op 显式 err + panic 兜底经 err 回填） |

## 2. 里程碑与验收对照

| 里程碑 | 内容 | 审查 |
|---|---|---|
| W1a | wit `remote` 接口（submit/take/handle）+ bindgen + stub | REVIEW-96af34c PASS |
| W1b | 宿主驱动：Host submit/take/drop、注册表、drive pump/poll 纯函数、适配器、单测 6 + op panic 兜底（err 通道） | REVIEW-f883492 PASS |
| W2 | guest 端到端：真实 submit → worker 执行 → poll_remotes 回填 → O-6 隔离断言 | REVIEW-704a46c PASS |
| W3 | 清理语义（drop 清槽、驱动完成即弃、退役 quiet）+ doc 重定位 + 全回归 + doc 归零 | REVIEW-501c0a1 PASS |

## 3. 门禁与回归

- `cargo +1.97.0 fmt --check` ✅ / `clippy --workspace --all-targets -- -D warnings` ✅ 0 告警 / `doc --workspace --no-deps` ✅ 0 告警（含 cordis-core/cordis-wasm 历史私链清零——纯文档文案，core 零语义改动）
- `cargo +1.97.0 test --workspace` ✅ 无回归（cordis-wasm lib 7 + 集成 14 含 remote_e2e 真回填——REVIEW-ERM-WASM-EXIT nit-1 计数修正 + go_guest/sandbox/dual_backend 等，共 21 条；loader/events/hmr/async 既绿）
- 专项测试：guest 提交→worker→回填（worker tid ≠ 组合线程 O-6 实测）；未知操作/未配置→句柄 err；op panic→err 兜底（组合线程零 panic）；Host drop 清槽；sandbox 回归（guest 恶意输入不 panic 宿主）

## 4. 时序边界（诚实记录）

- **core `execute` 为同步一口气循环**（无步间暂停）——guest 的 `handle.take()` 无法在**单次激活**内等到异步 worker 完成；`take` 回填的完整语义需**两次驱动**（M2 async 驱动 / 核心异步化解锁）。
- W2 的真实回填断言走**宿主 `poll_remotes`**（组合线程检查点非阻塞驱动）+ `remote_result`（提交→worker→回填链路真实）；guest `take` 契约为接口面（wit/编译——W1a/W2 guest 编译即证）。
- M2 解锁后，guest 可多步 take（轮询/等待）获得与 TokioRemote 同等的 join 等待语义。

## 5. 出口判定

**M1 wasm 桥专项（WasmRemote 宿主驱动）完成**：wit `remote` 协议面 + 宿主驱动（注入 Remote 执行、注册表、回填、错误通道、panic 兜底）+ guest 真实提交端到端 + 清理语义 + 沙箱/双后端回归 + 门禁全绿 + 审查闭环（0 Major 未决）。草案 v1.4 三形态 Remote 的 wasm 端落地（时序边界 M2 解锁）。

→ 后续取向：Phase 2 决策 / 更多产品假设 spike / wasm 桥时序异步化解锁（M2 任务）等，按纪律由用户下达。
