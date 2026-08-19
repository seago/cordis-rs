# cordis-wasm M1 桥专项 · WasmRemote 宿主驱动开发计划

**依据**：
1. 草案 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§2/§4（WasmRemote = M1 宿主驱动协议接入点：guest 无自发线程，submit = 请求入队 + 宿主 step 边界驱动并回填）、O-6（桥政策）；
2. `docs/cordis-async-PHASE1-P3-EXIT.md` §4（P1.3 R2：WasmRemote 占位接入点已落地，实际宿主桥留本专项）；
3. 既有桥基础设施 `crates/cordis-wasm`（PR #10–#14+：宿主驱动效应迭代 `task.step()`、pending-set、wit `Value` 统一值、沙箱隔离、go/rust guest）。
**状态**：**草案——待架构决策确认 + 开工指令**（本专项与既有 cordis-wasm 同步 step 模型存在架构张力，§1 决策必须先定；按纪律用户确认后开工）。
**保证**：里程碑间独立审查硬门禁（Gate B）同前；Gate A（fmt + clippy -D warnings + 单测全绿 + workspace 无回归）；commit 分 code/docs；core 零改动；cordis-wasm 沙箱/双后端回归不破坏。

---

## 0. 目标与非目标

**目标**：让 **wasm 插件（guest）能发起远端请求**——`spawn_remote` 语义的 wasm 落地：guest 在 step 里提交请求（入队）→ 宿主在 step 边界取走 → **宿主侧 worker（复用 TokioRemote 的 `spawn_blocking`/`handle.spawn`）执行（O-6：不触碰组合线程资源）** → 结果回灌 guest（pending/句柄取回）。补完草案 §4 三形态 Remote 的 wasm 端。

**非目标**：
- 不在 wasm guest 内运行 `cordis-async`（tokio 不 target wasm32-wasip2；guest 保持同步 step 模型）；
- 不改 `cordis-core`（值/绑定语义零改动）；
- 不做跨进程/跨机器通信（本专项仍是进程内跨 wasm 隔离边界）。

## 1. 前置：架构对齐决策（W0，待用户确认）

本专项与草案 v1.4 的一处张力必须先对齐：草案假设「WasmRemote 作为 host 驱动协议的接入点、guest 上跑 async 层」——但既有 cordis-wasm 是**宿主驱动同步 step 迭代**（`task.step()`），guest 不跑 tokio/async。四项决策（默认建议）：

| # | 决策 | 默认建议 | 说明 |
|---|---|---|---|
| W-D1 | **WasmRemote 职责重定位** | 「guest 经新增 wit `remote` import 发起；**宿主侧注入既有 `Remote`（TokioRemote）执行回灌**」；`cordis-async` 的 `WasmRemote` 占位改为桥接线注记（不实现 `Remote`——guest 侧不需要 trait 实现） | 消除草案「guest 跑 async」与同步 step 模型的张力；复用 TokioRemote，免造 worker |
| W-D2 | **guest 载荷形态** | 跨边界**不能传闭包** → guest 请求 = **宿主注册的远端操作**（名称 + `Value` 参数）；宿主映射到可执行闭包/服务 | 与 wit `Value`（统一值类型）一致；宿主维护 remote 操作注册表 |
| W-D3 | **回填时序** | v1 用「提交句柄 + 后续 step `remote::take_result(handle)`」：宿主 worker 在 **step 边界之间**执行（组合线程上宿主仅在 step 边界驱动队列、不阻塞），结果按句柄回填 | 保持 step 模型 + O-6（组合线程不阻塞）；`join.await` 的「等」 = 后续 step 轮询 take |
| W-D4 | **值传递** | 载荷/结果全部走 wit `Value`（数据值，无句柄跨边界 v1）；`Value` 与本机 `RemoteValue`（`Box<dyn Any+Send>`）的边界转换在宿主侧适配器 | 复用 PR #13 的统一值类型 |

## 2. 分步计划

### Step W0：架构对齐与协议细化（W0）

- **目标**：决策确认（W-D1..D4）+ wit/桥协议细化稿。
- **任务**：
  1. 依据确认的 W-D1..D4 形成协议细化记录（提交句柄语义、操作注册表、回填契约、错误通道）；
  2. 复核既有 `task.step()`/pending-set 机务（guest context::set → 宿主 step 后转发）可复用的回填通道；
  3. 评估沙箱边界（guest 崩溃/恶意请求不伤宿主，沿用 PR #14 沙箱）。
- **产出**：协议细化稿（docs 内部审议）。
- **验证**：无代码；W-D 决策落地记录。

### Step W1：wit `remote` 接口 + 宿主驱动（W1）

- **目标**：wit 世界新增 `remote` import（submit/submit操作注册/take_result）；宿主侧实现驱动（worker 执行 + step 边界回填）。
- **任务**：
  1. `wit/cordis.wit` 增 `remote` 接口：`submit(name, params: list<Value>?) -> remote-handle`、`take_result(handle) -> option<result<Value, remote-error>>`、`register`（宿主侧操作注册 API——宿主侧非 wit）；
  2. 宿主侧：`Host`/`InstanceState` 增 remote 队列（组合线程侧入队）+ worker 执行 + 结果句柄表；step 边界驱动（复用 pending 通道回填时机）；
  3. 复用宿主注入 `Rc<dyn Remote>`（TokioRemote）执行；`Value`↔`RemoteValue` 适配器；
  4. 沙箱：guest 非法操作名/错参 → `remote-error`（不 panic 宿主）。
- **产出**：wit + 宿主驱动 + 适配器。
- **验证**：宿主单测（fake Remote 执行 + 回填时序）。

### Step W2：guest 侧接线 + 端到端（W2）

- **目标**：guest 组件在 step 中经 `remote` import 提交并取回；端到端遥控。
- **任务**：
  1. rust guest 示例扩展：`remote::submit` → 后续 step `take_result` 轮询 → 注入效应/记录；
  2. 测试：guest 提交 + 宿主 worker 执行（O-6 线程隔离断言，仿 m06）+ 回填值断言；
  3. 错误通道测试：未知操作/参数错 → `remote-error` → guest 侧记录（不级联、宿主不崩）。
- **产出**：guest 接线 + 2–3 条端到端测试。
- **验证**：`cargo test -p cordis-wasm`（端点测试）+ go/rust guest 沙箱回归。

### Step W3：中断/卸载语义 + 回归（W3）

- **目标**：guest 卸载/fiber 退役时 remote 未决请求的处理（句柄撤销、结果丢弃不泄漏；契约 C-5 无野任务）。
- **任务**：
  1. guest 迭代结束/退役 → 未取结果句柄清理（宿主 worker 任务完成即弃、句柄表回收——pending-set 泛化语义）；
  2. 沙箱/双后端/go guest 全回归；
  3. `cordis-async` `WasmRemote` 占位 doc 更新（W-D1 重定位说明）。
- **产出**：清理语义 + 回归绿。
- **验证**：全 workspace 门禁。

### Step W4：出口（W4）

- **目标**：专项出口走查 + EXIT 文档。
- **任务**：验收清单（W1 宿主驱动 / W2 端到端 / W3 清理与回归）逐条对证；`docs/cordis-wasm-WASMREMOTE-EXIT.md`；流转。
- **验证**：全部测试绿 + 出口走查 + 审查。

## 3. 里程碑与时间量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| W0 | 架构对齐 + 协议细化 | 决策确认 | 0.5–1 天 |
| W1 | wit remote + 宿主驱动 | W0 | 1–2 天 |
| W2 | guest 接线 + 端到端 | W1 | 1–2 天 |
| W3 | 清理语义 + 回归 | W2 | 1 天 |
| 出口 | 走查 + EXIT | W3 | 0.5 天 |

全程约 4–6 天（含审查门禁；适配器/沙箱工作量为主风险）。

## 4. 出口判定

**标准**：guest 提交 → 宿主 worker 执行 → 回灌值断言直证（O-6 隔离）+ 错误通道 + 清理语义 + 沙箱/双后端回归 + workspace 无回归 + 出口走查（无未解释偏差）→ 补完草案 §4 Remote 三形态（TokioRemote / WasmRemote 桥 / 接入点）。

## 5. 依赖与纪律约束

- **core 零改动**（值/绑定语义不动；`Value` 下沉 core 的 PRE-XXX 边界不并入本专项）；`cordis-wasm` run-deps 增加 tick? 依赖（wasmtime/wasi 既有 + 复用 `cordis-async` 的 `Remote`（dev 或 run——接入点经 trait 引用，避免重依赖）。
- `unsafe_code=deny` + `#![deny(missing_docs)]`；沙箱纪律（guest 恶意输入 → `remote-error` 不 panic 宿主）。
- commit 分 code/docs；审查报告入库 `docs/reviews/`。
- **开工纪律**：§1 决策（W-D1..D4）确认 + 用户下达后才写实现。
