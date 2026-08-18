# cordis-async Phase 1 · P1.2 开发计划（AsyncRuntime 完善线）

**依据**：
1. 草案 `docs/cordis-async-protocol-draft.md` v1.4（冻结）§5 门面 API、O-2/O-3/O-4 开放项；
2. `docs/cordis-events-PHASE1-PLAN.md` §3 预备路线（P1.2 定义）；
3. `docs/cordis-events-PHASE1-EXIT.md` §4（P1.1 出口 → P1.2 决策）；
4. 既有实现：`crates/cordis-async`（M0.1–M0.6 全审查闭环）。
**状态**：**草案状态——待决策确认 + 开工指令**（P1.2 有 4 项设计决策待用户拍板，见 §1；按纪律开工由用户下达；未确认/下达不写实现）。
**保证**：里程碑间独立审查硬门禁（Gate B）同前；Gate A（fmt + clippy -D warnings + 单测全绿 + workspace 无回归）；commit 分 code/docs；tokio 只进 cordis-async run-deps（不污染 sync 侧）。

---

## 0. 目标与交付物

完善 `cordis-async` 的 AsyncRuntime 门面到草案 §5 的正式形态：**AsyncFiberHandle 门面收口**（M0.5 临时 `Rc<Fiber>` 定型）、O-2/O-3/O-4 三项开放项按决策落地、门面纪律（C-4）文档化，并将 P1.3（Remote 扩展）前置的 API 面稳定。

**交付**：
1. `AsyncFiberHandle` 类型（weak fiber + 代次）+ `use_component`/`retire`/`update` 签名收口到草案 §5 形态；
2. O-2/O-3/O-4 决策落地（默认值见 §1，用户确认后生效）；
3. 门面纪律文档（C-4 分界：绕过门面的直接 core 调用不记账）；
4. 新增单测直证 Handle 收口 + 决策项语义；workspace 无回归。

## 1. 前置：待决策项（默认建议，用户确认后生效）

| # | 决策 | 默认建议 | 影响 |
|---|---|---|---|
| D-1 | **AsyncFiberHandle 形态** | `{ fiber: Weak<Fiber>, generation: u64 }`（弱化防环 + 代次校验，防串代）；`use_component -> Result<AsyncFiberHandle, RegistryError>`；`retire(&AsyncFiberHandle)` / `update(&AsyncFiberHandle, config)` | 门面 API 正式化；既有 `Rc<Fiber>` 用法迁移（M0.5 测试适配） |
| D-2 | **O-2 auto-settle 模式** | 框架层**保持显式 settle**（现状）；`AutoSettle`/每批次自动 settle 的 app 层封装**不做**，仅文档说明（草案 O-2 倾向） | 若用户选「提供 app 层封装」则加一个小工具函数 + 测试 |
| D-3 | **O-3 lifecycle observer hook** | **不启用** core `update_hook`/`retire_hook` 作门面 hook（草案默认）；记录「若 C-4 频繁违反再启用」 | 仅文档记录 |
| D-4 | **O-4 Failed 载荷富化** | **保持 `String`**（草案 O-4：等首个真实失败场景再定）；`AsyncFiberError` 预留扩展点（`message()` 不变） | 仅文档记录 |

## 2. 分步计划（P1.2 主交付线）

### Step 0：AsyncFiberHandle 类型与门面签名收口（H1）

- **目标**：引入 `AsyncFiberHandle`；`use_component`/`retire`/`update` 迁至 Handle 形态；内部仍以 `Weak<Fiber>` + `generation` 解引用。
- **任务**：
  1. `AsyncFiberHandle { fiber: Weak<Fiber>, generation: u64 }`（+ `new`/`upgrade`/`generation`；pub 仅必要项）；
  2. `AsyncRuntime::use_component(...) -> Result<AsyncFiberHandle, RegistryError>`（原 `Rc<Fiber>` 改为 Handle；entry 自登记不变）；
  3. `retire(&AsyncFiberHandle)` / `update(&AsyncFiberHandle, config)` 迁移（内部 `upgrade` → 透传 core）；
  4. 测试：Handle 的 retire/update 路径回归（m05 测试 8/9 适配 + 一个新 Handle 语义测试：generation 校验防串代）。
- **产出**：Handle 收口 + 回归测试。
- **验证**：`cargo +1.97.0 test -p cordis-async`（M0.1–M0.6 全回归）。
- **风险**：`Weak<Fiber>` upgrade 与既有 entry/fiber 生命周期交互（Handle 持弱引，fiber 释放后 upgrade None——门面操作须优雅处理（panic 诊断或 no-op，按草案 panic=bug）。

### Step 1：门面纪律文档 + O-2/O-3/O-4 决策落地（H2）

- **目标**：C-4 门面纪律文档 + D-2/D-3/D-4 按确认值落地。
- **任务**：
  1. `docs/` 或 crate doc：C-4 分界（retire/update/apply 走门面；直接 core sync API 允许但 async 尾巴不 settle 记账）；
  2. O-2：若默认（显式）→ 仅文档；若选封装 → `AutoSettle` 小工具 + 测试；
  3. O-3/O-4：默认仅文档记录（`update_hook`/`retire_hook` 既有、Failed 保持 String）；
  4. 测试（若无新增决策实现则无新测试；D-2 封装若启用则 +1 测试）。
- **产出**：门面纪律文档 + 决策落地。
- **验证**：doc 0 告警（`deny(missing_docs)`）。

### Step 2：P1.3 前置 API 稳定（H3）

- **目标**：P1.3（Remote 扩展）所需的 API 面稳定——`Remote` trait、`AsyncCx::spawn_remote`、`TokioRemote` 签名冻结（M0.6 已实现，本步做不变量固化：`#[non_exhaustive]`/doc 版本标注、行为 doc 补齐）。
- **任务**：
  1. `Remote`/`RemoteJoin`/`RemoteValue`/`RemoteRequest` 的 doc 补全（含「Send-future 分池形态为 P1.3 扩展点」标注，O-即草案 §2/§4）；
  2. `TokioRemote` 行为 doc（worker 生命周期、O-6 边界）复核补全；
  3. 无行为改动（仅 doc/冻结标注）；若有 doc 触发 missing_docs → 补。
- **产出**：P1.3 前置 API 面稳定。
- **验证**：doc 0 告警 + 回归。

### Step 3：P1.2 出口（H4）

- **目标**：Handle 收口 + 决策落地 + 文档齐备 + 出口走查。
- **任务**：
  1. 全 workspace 门禁（fmt/clippy/test 无回归）；
  2. 出口走查：Handle 迁移完整性（无残留 `Rc<Fiber>` 门面签名）、O-2/O-3/O-4 决策记录、C-4 文档；
  3. 固化 `docs/cordis-async-PHASE1-P2-EXIT.md`（迁移对照 + 决策 + 门禁 + 出口判定）；流转 P1.3。
- **验证**：全部测试绿 + 出口文档评审。

## 3. 里程碑与时间量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| H1 | Step 0 Handle 收口 | — | 1–2 天 |
| H2 | Step 1 门面纪律 + O-2/3/4 | H1 | 0.5–1 天 |
| H3 | Step 2 Remote API 冻结标注 | H2 | 0.5 天 |
| 出口 | Step 3 走查 + EXIT | H3 | 0.5 天 |

全程约 3–4 天（含审查门禁）。

## 4. 后续线概览（P1.3 / P1.4，开工另达）

### P1.3 Remote 扩展 + 双运行时收口
- **前置**：P1.2 H3（API 面稳定）+ 待决策：Send-future 分池形态（`submit` 扩展 `Send` async future）、WasmRemote 接入 M1 协议（wasm 侧工作量，可单独排期）、双运行时共存收口（S2 已验，文档化）。
- **草案依据**：async v1.4 §2/§4（桥泛化目标）、O-6。
- **出口**：Remote 双形态 + 共存文档 + 出口走查。
- 详细计划在 P1.2 出口时起草（含上述决策点）。

### P1.4 DX 文档
- **内容**：插件作者指南（Arc 值惯例 C-1 / 绑定 vs 资源 C-2 / 门面纪律 C-4）、错误与安静语义文档、示例插件模板（事件订阅 + async 监听器 + agent loop 组合示例，基于 spikes S1/S3）。
- **前置**：P1.2/P1.3 API 定稿。
- 开工由用户下达；纯文档线，里程碑可精简。

## 5. 依赖与纪律约束

- **零第三方**：cordis-async run-deps 仅 tokio（既有）+ cordis-core；不改 sync 侧零依赖纪律。
- `unsafe_code=deny` + `#![deny(missing_docs)]`。
- commit 分 code/docs；审查报告入库 `docs/reviews/`。
- 接口迁移影响：`use_component` 返回形态变化会触碰 cordis-async 既有测试（m05/m06 等）——回归适配计入 Step 0，审查重点核对「仅签名形态迁移、无语义变化」（AsyncFiberHandle 解引用语义与 `Rc<Fiber>` 等价，除升级失败路径）。
- **开工纪律**：本计划为草案；§1 决策确认 + 用户下达后才写实现。
