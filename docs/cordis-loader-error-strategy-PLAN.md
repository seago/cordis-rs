# cordis-loader 错误策略线开发计划（OrchestrationError 迁移）

**依据**：
1. `docs/cordis-rs-error-strategy-draft.md` **v0.2**（已冻结判定：评审闭环 `docs/CORDIS-ERROR-STRATEGY-REVIEW.md` v0.1→v0.2，0 Major/0 Minor 未决，3 项 Nit 表述级）；
2. 既有 loader 实现（G1/G7 语义、两阶段协调、writeback 写回）。
**状态**：**待开工指令**（按执行纪律：开工由用户下达；未下达不写实现）。
**保证**：里程碑间独立审查硬门禁（Gate B）同前；Gate A（fmt + clippy -D warnings + 单测全绿 + workspace 无回归）；commit 分 code/docs；**cordis-core 零改动**（复用 `FiberError::new/raise` pub、L-Raise 全链路）。

---

## 0. 目标与交付物

把 loader 的「配置错误 panic = bug」二分升级为三级错误分类：Bug（panic 保持）/ ComponentFailure（L-Raise，仅运行时失败，不改行为）/ **OrchestrationError（Result + 逐条目报告，不中断 apply；用户输入可达错误全走此通道）**。

**交付**：
1. 新错误类型面：`EntryError`/`EntryErrorKind`（UnknownComponent / ConfigValidation / ProvisionClash{keys,owner} / UnknownParent）+ Display 契约（三要素）；
2. `EntryOutcome`（Unchanged/Activated/Failed/FailedFiber）+ `ApplyReport`（failed()/ok()）+ 汇聚机制（协调序 Vec）；
3. OrchestrationError 迁移：`validate_config` 改 `Result`、未知组件、`use_component` 失败（ProvisionClash/UnknownParent）**不 panic → 报告 + 跳过 + 每次 apply 重试**（校验失败未挂载路径，v0.2 决议）；
4. 报告面：`Loader::report()`（最近一次 ApplyReport 副本）+ `entry_state(id)`；events `loader/entry-failed` 衔接（integration）；
5. 既有 `#[should_panic]` 断言迁移（验收 #9）+ 验收 #1–#9 全过。

**非交付**：不改 L-Raise/ComponentFailure 行为；不转作者义务类 panic；不引错误码/国际化（O-5）。

## 1. 前置（动工前确认）

- **core 零改动复核**：`FiberError::new`（fiber.rs:42）+ `pub fn raise`（:55）均 pub——**无需 core 改动**；OrchestrationError 走 loader 侧 Result 通道，core `RegistryError::ProvisionClash` 保持 unit（key 由 loader 自交并推断）。
- **决策确认（用户）**：
  - E-1：`apply` 签名改为返回 `ApplyReport`（Rust 允许忽略返回值，既有调用兼容）——确认；
  - E-2：events `loader/entry-failed` 衔接在 S2 落地（events 已冻结，可直接发射）——默认做，确认；
  - E-3：UnknownParent 报告 + 跳过（与未知组件同路径）——确认。

## 2. 分步计划

### Step 0：类型面 + Display 契约（E0）

- **目标**：新错误类型面落地（纯类型，无行为迁移）。
- **任务**：
  1. `EntryError`/`EntryErrorKind`（四变体）+ `Display`（三要素：entry id + 键/组件名 + 原因；一行）；
  2. `EntryOutcome`（Unchanged/Activated/Failed/FailedFiber）+ `ApplyReport`（`failed()`/`ok()`）+ `EntryState`；
  3. crate doc 标注（草案 v0.2 依据、判定公理）。
- **验证**：`cargo build -p cordis-loader` + 类型链 smoke 测试。

### Step 1：OrchestrationError 迁移 + apply 返回报告（E1）

- **目标**：四个报告位点（validate / 未知组件 / ProvisionClash / UnknownParent）改不 panic + apply 返回 `ApplyReport`。
- **任务**：
  1. `config.rs:49` `validate_config` 改 `Result<(), String>`（`validate_config` 调用点 `?`/match）；
  2. `instantiate_leaf`：validate Err → 报告 `Failed(ConfigValidation)`、不挂载；未知组件 → `UnknownComponent`（不 panic）；`use_component` Err(ProvisionClash) → first-wins 报告 `Failed(ProvisionClash{keys,owner})`（keys = provide ∩ 注册键全列、owner 反查）；Err(UnknownParent) → 报告跳过；
  3. `instantiate_group` 同路径（组失败：整组 Failed、子条目不实例化不报告）；
  4. `apply`/`apply_into`：每层追加协调序 Vec → `apply` 返回 `ApplyReport`（写 `report()` 快照并存）；
  5. 两阶段不变；未挂载失败条目每次 apply 重试（不写回、无 fiber、无供给占用）。
- **验证**：验收 #1/#2/#4/#5/#7 单测（叶子失败不中断 / 组失败 / first-wins / 未知组件 / 同键替换不误报）。

### Step 2：报告面 + 事件衔接 + 幂等重试（E2）

- **目标**：`report()`/`entry_state(id)` + events `loader/entry-failed` 发射 + 重试语义直证。
- **任务**：
  1. `Loader::report()`（最近一次 ApplyReport 副本）+ `entry_state(id) -> Option<EntryState>`；
  2. events 衔接：`loader/entry-failed` 事件（载荷 = `EntryError`）经 cordis-events 发射（`EventsProvider` + subscribe 模式）；dev-dep events 引入 loader 测试；
  3. 验收 #3（重试与复活：失败不写回不挂载、desired 未变下次 apply 重试并重报 Failed、修配置+bump 后 Activated）、#6（Display 契约）。
- **验证**：验收 #3/#6 + events 衔接单测。

### Step 3：既有测试迁移 + 验收 #8/#9 + 出口（E3）

- **目标**：既有 `#[should_panic]` 断言迁移；全验收绿；出口走查。
- **任务**：
  1. 迁移清单（未注册组件 / ProvisionClash / G7 校验失败断言 → `report().failed()` 断言 + 其余条目 Activated 断言）；
  2. 验收 #8（core 供给纪律越界写仍 panic——分类边界护栏）；
  3. 全 workspace 门禁 + 出口走查 + `docs/cordis-loader-error-strategy-EXIT.md`。
- **验证**：验收 #1–#9 全过 + workspace 无回归 + 出口评审。

## 3. 里程碑与时间量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| E0 | Step 0 类型面 + Display | — | 0.5–1 天 |
| E1 | Step 1 OrchestrationError 迁移 + apply 返回 | E0 | 1–2 天 |
| E2 | Step 2 报告面 + events 衔接 | E1 | 1 天 |
| E3 | Step 3 迁移 + 出口 | E2 | 0.5–1 天 |

全程约 3–5 天（含审查门禁）。

## 4. 出口判定

**标准**：验收 #1–#9 全过（含既有 should_panic 迁移、panic 边界护栏）+ 报告面/事件衔接绿 + workspace 无回归 + 出口走查（无未解释偏差）→ 完成错误策略线，流转后续（wasm 桥 / Phase 2）。

## 5. 依赖与纪律约束

- **core 零改动**（统一护栏：`git diff` 不得触碰 `crates/cordis-core`）。
- 零第三方：loader run-deps 保持 cordis-core；events（dev）仅测试用。
- `unsafe_code=deny` + `#![deny(missing_docs)]`。
- commit 分 code/docs；审查报告入库 `docs/reviews/`。
- **开工纪律**：§1 决策（E-1/E-2/E-3 默认建议）确认 + 用户下达后才写实现。
