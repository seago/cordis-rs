# 产品验证线 P-2 详细计划 —— 双后端值类型下沉（cordis-value）

**依据**：Phase 2 提案（P-2，决策全做）；cordis-wasm lib.rs 已记录边界（THEORY-MAP PR#13：`Value` 类型定义在 cordis-wasm 的 wit 绑定中——原生组件要与 wasm 互通须依赖 cordis-wasm（仅为值类型），依赖方向"原生→wasm"与"wasm 依赖 core、core 无关后端"的分层不一致）；REVIEW-PHASE2-PROPOSAL m-1（量级 4–6 天：wit 重生成 + 双后端重编）。
**状态**：**草案——待开工指令**。
**保证**：Gate A/B 同前；commit 分 code/docs；**cordis-core 零改动**（值类型放独立 crate，不触 core 纪律）；双后端回归全绿（wasm + loader + native）。

---

## 0. 问题与目标

- **现状**：统一值类型 `Value`（wit `variant value { flag(bool), count(u64), offset(s64), text(string), blob(list<u8>) }`）由 cordis-wasm 的 bindgen 生成——原生组件要与 wasm 组件互通（`set_dyn`/`get_dyn` 值语义）须**依赖 cordis-wasm**（仅为该类型）；wasm 组件经 `set_dyn` 装箱 wit `Value`，原生组件装自己类型 → 跨类型值翻译不支持（M1 边界：sync_injected 只同步 wasm 装箱值）。
- **目标**：把统一值类型下沉到**独立 `crates/cordis-value`**（零第三方、零 core 依赖）——原生与 wasm 组件共用同一 `Value` 类型（依赖方向"原生 → cordis-value"，cordis-wasm 也依赖 cordis-value；core 无关后端的分层恢复）。

## 1. 设计

### 1.1 cordis-value crate
- `crates/cordis-value`（workspace 新成员）：`pub enum Value { Flag(bool), Count(u64), Offset(i64), Text(String), Blob(Vec<u8>) }`（derive Clone/Debug/PartialEq/Eq + Send+Sync；无第三方）。
- **与 wit 的关联**：wit 世界中的 `value` 变体声明为 **external type**（从独立 wit package `cordis:value` 引入）——wit-bindgen 生成代码对 external 类型生成**引用**（`cordis_value::Value`）而非新类型；value crate 提供 bindgen 要求的生成接口（`wit-bindgen` 生成侧：external 类型由使用方实现 `#[derive]`/接口——具体机制见 P2-0 spike）。

### 1.2 wit 重构
- wit：新 package/interface `cordis:value`（`variant value { … }`）；`context` interface `use value.{value}`（get/set/remote 载荷均用该 external 类型）。
- 生成侧：cordis-wasm 的 bindgen 配置把 external 映射到 `cordis_value::Value`。

### 1.3 依赖方向（达成后）
```
cordis-core（零第三方）
   ↑                    ↑
cordis-value            cordis-loader
   ↑  ↑                    ↑
cordis-wasm ── natives/原生组件（经 cordis-value 互通）
```
- 原生组件（examples/hello-plugin 等）若需与 wasm 互通：依赖 `cordis-value`（轻量），不再依赖 cordis-wasm。

## 2. 分步

### Step P2-0：external type spike（P2-0）
- **目标**：验证 wit-bindgen 0.60 的 external type 映射可行（最小双 package wit + 生成引用 cordis_value::Value）。
- **任务**：最小 spike（临时 wit 双 package + 生成样例）→ 确认映射机制（`type_*` 配置/use 外部 package 的生成形态）；若 0.60 不支持 → 方案 B（见 §5 风险）。
- **验证**：spike 编译 + 类型同一性（生成代码引用 cordis_value::Value 而非副本）。

### Step P2-1：cordis-value crate + wit 重构（P2-1）
- **任务**：建 crate（Value 枚举 + 全套 derive）；wit 拆 package；cordis-wasm bindgen external 映射 + 迁移既有 `wit::cordis::core::context::Value` 引用到 `cordis_value::Value`；`set_dyn` 装箱/`get_dyn` downcast 侧统一。
- **验证**：cordis-wasm lib/tests 编译 + 既有 wasm 测试绿（值语义不变）。

### Step P2-2：双后端互通打通（P2-2）
- **目标**：原生组件与 wasm 组件经 `cordis-value` 互通（消除"跨类型值翻译不支持"边界）。
- **任务**：
  1. 原生侧组件改用 `cordis_value::Value`（`set_dyn` 装箱 cordis_value::Value——与 wasm 侧同类型）——hello-plugin/im-bot 或新增互通测试组件；
  2. `sync_injected`/镜像同步的"仅 wasm 装箱值"边界移除（现下同类型即通）；
  3. 测试：原生 provider + wasm consumer 经 cordis-value 互通（及反向）——直证依赖方向反转。
- **验证**：新增互通测试 + 全套回归。

### Step P2-3：清理与出口（P2-3）
- **任务**：cordis-wasm 的 `pub use … Value` 兼容重导出（过渡期）或移除（按决策）；THEORY-MAP PR#13 边界行更新（已下沉）；EXIT 文档 + 出口走查。
- **验证**：门禁全绿（workspace 无回归）+ 走查 PASS。

## 3. 里程碑与量级

| 里程碑 | 内容 | 依赖 | 量级 |
|---|---|---|---|
| P2-0 | external type spike（机制验证） | 开工指令 | 0.5–1 天 |
| P2-1 | cordis-value + wit 重构 + 迁移 | P2-0 | 1–2 天 |
| P2-2 | 双后端互通打通 + 测试 | P2-1 | 1–2 天 |
| P2-3 | 清理 + EXIT + 走查 | P2-2 | 0.5 天 |

全程约 4–6 天（含审查门禁）。

## 4. 决策点（开工前确认）

1. **下沉位置**：独立 `crates/cordis-value`（推荐——零 core 改动、零第三方）vs core 内模块（破坏 core 零改动纪律）——确认独立 crate；
2. **wit external 映射**：P2-0 spike 验证（若 wit-bindgen 0.60 不支持 external → 方案 B：wit 保持单 package，但 Rust 侧以 `type_value = "cordis_value::Value"` 类配置重映射生成——同样可达同一类型；spike 定稿）；
3. **兼容重导出**：P2-3 是否保留 `cordis_wasm::Value` 重导出（过渡）——默认保留（doc 标注 deprecated 指向 cordis-value）。

## 5. 风险

- **wit-bindgen 0.60 external type 支持**：spike 前置验证；不支持则方案 B（类型重映射）仍可达目标（工作量相近）。
- **双后端全量重编**：wit 结构变（package 拆分）→ 全部 guest（rust 4 + go）+ host + 原生组件引用迁移；CI 构建步骤确认。
- **值语义零变化**：Value 枚举形态与 wit 相同——以既有 wasm 测试 + 新互通测试为护栏。

## 6. 纪律

Gate A（fmt/clippy -D warnings/workspace 无回归）+ Gate B（REVIEW-<hash>）；commit 分 code/docs；**cordis-core 零改动**；零第三方（cordis-value 无依赖）。
