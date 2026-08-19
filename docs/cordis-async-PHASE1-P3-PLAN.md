# cordis-async Phase 1 · P1.3 开发计划（Remote 扩展 + 双运行时收口线）

**依据**：
1. 草案 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§2/§4（Remote 桥泛化目标：Send future 分池 / spawn_blocking 双形态、WasmRemote 接入）、O-6（桥政策）；
2. `docs/cordis-async-PHASE1-P2-PLAN.md` H3 冻结标注（P1.3 扩展点：以新增表述变体、不破坏既有签名）；
3. `docs/cordis-async-PHASE1-P2-EXIT.md` §5（P1.2 出口 → P1.3 决策）；
4. 既有实现：`crates/cordis-async`（M0.1–M0.6 + P1.2 H1–H3 全审查闭环）、S2 spike（tokio 服务 sync 壳 + spawn_remote 已验证）。
**状态**：**草案——待决策确认 + 开工指令**（P1.3 有 3 项设计决策待用户拍板，见 §1；按纪律开工由用户下达；未确认/下达不写实现）。
**保证**：里程碑间独立审查硬门禁（Gate B）同前；Gate A（fmt + clippy -D warnings + 单测全绿 + workspace 无回归）；commit 分 code/docs；tokio 只进 cordis-async run-deps。

---

## 0. 目标与交付物

把 Remote 桥从 v1（`spawn_blocking` 闭包单形态）扩展到草案 §4 的完整两形态（+Send future 分池），界定 WasmRemote 接入边界，并把双运行时（sync 树 + async 组合线程）共存模式文档化收口。

**交付**：
1. **Send-future 分池形态**：`RemoteRequest` 增设 Send async future 表述变体；`TokioRemote::submit` 双形态调度（闭包→`spawn_blocking`、future→multi_thread 池 `handle.spawn`）；单测直证提交 + join 回灌 + O-6 隔离；
2. **WasmRemote 边界**（按 D-2 决策范围）：接入点类型 + 协议接线文档；
3. **双运行时共存收口**：拓扑文档 + 一个集成示例（sync 组件 + async 组件 + events + Remote 组合）；
4. 新增单测 + 集成示例直证；workspace 无回归。

## 1. 前置：待决策项（默认建议，用户确认后生效）

| # | 决策 | 默认建议 | 影响 |
|---|---|---|---|
| D-1 | **Send-future 分池形态** | **做**（草案 §4 双形态）：`RemoteRequest` 增设 future 变体（`Pin<Box<dyn Future<Output=RemoteValue> + Send>>`），`TokioRemote::submit` 按变体调度（闭包→spawn_blocking / future→`handle.spawn` 上 multi_thread 池）；join 回灌组合线程（跨 runtime JoinHandle await，M0.6 已验安全） | 主实现 |
| D-2 | **WasmRemote 接入范围** | **接入点 + 协议接线文档**（`WasmRemote` 占位类型 + `submit` = 入队、宿主 step 边界驱动并回填的语义/doc，M1 宿主驱动协议对接说明）；**实际 wasm 桥**（host 驱动 step 边界、跨 wasm 值传递）**留 M1 桥专项**（涉及 cordis-wasm 宿主驱动改造，独立工作量）——若选「完整落地」则 P1.3 扩容至 wasm 专项 | 范围界定 |
| D-3 | **双运行时共存收口** | **文档化 + 集成示例**：进程拓扑文档（唯一组合线程 + LocalSet；sync-only 与 async 组件同 loader 树；`get_cloned/set/spawn_remote` 边界；O-6 桥政策）+ `examples/` 组合示例（loader 挂 sync 组件 + async 组件 + events 订阅 + Remote 调用） | 示例工作量可控（复用 S2/S3 spike 形态） |

## 2. 分步计划（P1.3 主交付线）

### Step R0：前置确认（R0）

- **目标**：确认 Send-future 双形态的技术前提无坑。
- **任务**：
  1. 核对 `tokio::runtime::Handle::spawn`（multi_thread 池）在组合线程可调 + `JoinHandle` 跨 runtime await（M0.6 已验证）——无需 core 改动；
  2. 复核 `RemoteRequest` 当前结构（私有 `Box<dyn FnOnce …>`）扩展为双变体 enum 的可行性（`submit` 签名不变，H3 冻结保持）。
- **产出**：无代码；确认记录。

### Step R1：Send-future 分池形态（R1）

- **目标**：`RemoteRequest` 双变体 + `TokioRemote::submit` 双形态调度 + 单测。
- **任务**：
  1. `RemoteRequest` 改内部双变体：`Closure(Box<dyn FnOnce() -> RemoteValue + Send>)` | `Future(Pin<Box<dyn Future<Output = RemoteValue> + Send>>)`；`boxed`/`From<FnOnce>` 保留（闭包形态）；新增 `from_future`（或 `From<Pin<Box<...>>>`）构造；
  2. `TokioRemote::submit`：match 闭包→`spawn_blocking`、future→`handle.spawn(fut)`（multi_thread 池）；两路 join 回灌；
  3. 单测 R1-a（future 提交）：组合线程 `submit(from_future(async { compute }))` → join 回灌值断言；R1-b（O-6 隔离）：future 在 worker 池执行（线程 id 断言 != 组合线程，仿 m06 模式）；回归：既有闭包形态（m06 测试）不回归。
- **产出**：双形态 + 2–3 条单测。
- **验证**：`cargo +1.97.0 test -p cordis-async`（累计 >22）。
- **风险**：`Pin<Box<dyn Future + Send>>` 的构造/await；TokenRemote Handle 双形态调度；组合线程不触碰 worker 结果之外资源（O-6）。

### Step R2：WasmRemote 接入边界（R2，按 D-2）

- **目标**：`WasmRemote` 接入点类型 + 协议接线文档（范围见 D-2 默认）。
- **任务**：
  1. `crates/cordis-async`（或按 D-2 定的位置）定义 `WasmRemote`（占位：`submit` = 请求入队 + 宿主 step 边界驱动并回填语义 doc；实际桥在 M1 专项）；
  2. 协议 doc：对接 M1 宿主驱动协议（PR #11–13）接线说明（guest 无自发线程、submit 入队、宿主 step 边界驱动、回填语义不变）；
  3. 若 D-2 选「完整落地」→ 改为 wasm 专项（local port 试点），范围另行展开。
- **产出**：接入点 + 协议 doc。
- **验证**：doc 0 告警 + 回归。

### Step R3：双运行时共存收口（R3，按 D-3）

- **目标**：拓扑文档 + 组合示例。
- **任务**：
  1. 文档：进程拓扑（§4 单组合线程 + LocalSet；sync-only 与 async 组件同 loader 树；`get_cloned`/`set`/`spawn_remote` 边界；O-6 sync→async 桥政策）；
  2. `examples/` 组合示例（复用 S2/S3 形态）：loader 挂 sync 组件 + async 组件 + EventsProvider + spawn_remote 远端调用回路的迷你 app（可运行 `cargo run`）；
  3. 示例验收：回路跑通、卸载收账。
- **产出**：拓扑文档 + 可运行示例。
- **验证**：示例编译运行 + 回归。

### Step R4：P1.3 出口（R4）

- **目标**：双形态 + 边界 + 收口齐备 + 出口走查。
- **任务**：
  1. 全 workspace 门禁（fmt/clippy/test 无回归）；
  2. 出口走查：双形态语义完整（闭包表态回归 + future 新形态直证）、WasmRemote 边界符合 D-2、共存文档/示例与实现一致；
  3. 固化 `docs/cordis-async-PHASE1-P3-EXIT.md`；流转 P1.4（DX 文档）+ Phase 2 决策取向。
- **验证**：全部测试绿 + 出口文档评审。

## 3. 里程碑与时间量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| R0 | 前置确认 | — | 0.5 天 |
| R1 | Send-future 分池形态 | R0 | 1–2 天 |
| R2 | WasmRemote 边界 | R1 | 0.5–1 天 |
| R3 | 双运行时收口 | R1 | 1 天 |
| 出口 | R4 走查 + EXIT | R3 | 0.5 天 |

全程约 4–5 天（含审查门禁；D-2 若选完整 wasm 落地另加 wasm 专项工期）。

## 4. 后续线概览（P1.4 / 后续）

- **P1.4 DX 文档**：插件作者指南（C-1/C-1'/C-2/C-4：Arc 惯例、绑定 vs 资源、门面纪律）、错误/安静语义文档、示例插件模板（事件订阅 + async 监听器 + agent loop 组合，基于 spikes S1/S3 + P1.3 组合示例）。开工由用户下达；纯文档线。
- **Phase 2 取向**：wasm 桥专项（WasmRemote 宿主驱动）、更多产品假设 spike——P1.3 出口时评估。

## 5. 依赖与纪律约束

- **零第三方**：cordis-async run-deps 仅 tokio + cordis-core（Send-future 形态不引新依赖——`Pin<Box<dyn Future + Send>>` 标准库/tokio 即可）。
- `unsafe_code=deny` + `#![deny(missing_docs)]`。
- **API 冻结保持**：P1.2 H3 冻结的 `submit(RemoteRequest) -> RemoteJoin<RemoteValue>` 签名**不破坏**（扩展在 `RemoteRequest` 内部变体，`submit` 签名不变）。
- commit 分 code/docs；审查报告入库 `docs/reviews/`。
- **开工纪律**：§1 决策（D-1/D-2/D-3）确认 + 用户下达后才写实现。
