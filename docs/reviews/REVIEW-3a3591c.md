# 代码审查报告：commit `3a3591c`（P-2 双后端值类型下沉 方案 C）

> 审查人说明：本报告由独立审查执行（含实测）。审查对象 = P-2 P2-1+P2-2（方案 C）。

- **审查对象**：`3a3591c`（P-2 双后端值类型下沉，产品验证线）
- **范围**：`crates/cordis-value`（新）+ `crates/cordis-wasm/{src/lib.rs, Cargo.toml}` + 9 个测试文件迁移 + `Cargo.toml`/`Cargo.lock`
- **验证**：`cargo +1.97.0 test -p cordis-wasm`（lib 8 + 集成 14 全绿，含 go 19.9s）+ `clippy --workspace --all-targets -D warnings`（0）+ `fmt --check`（OK）+ `doc --workspace --no-deps`（0 告警）

---

## 总体结论

✅ **PASS（主体达成）** — 双后端值类型下沉（方案 C）落地正确、互通直证、零 core 改动、零第三方。
- **major**：0
- **minor**：1（方案命名一致性问题，见 M-1）
- **nit**：1（转换层开销观察）

## 核查要点（逐条通过）

### 1. 方案 C 采纳（P2-0 spike 结论）
- wit-bindgen 0.60 / wasmtime bindgen **不支持 external type 映射**（无 deps 键、单文件单 package 限制）→ 采用**转换层方案 C**——wit 不动、guest（rust 4 + go）无感、值语义零变化。
- 与计划决策点②的偏离（计划预想 external 映射→方案 B；实施走转换层 C）：**合理且已记录**（`P-2` 计划/EXIT 应同步命名）。

### 2. 桥接转换完整性（核心）
- `pub use cordis_value::Value`——**公开面统一类型**；wit 生成类型保留内部（`wit` 模块）不泄漏。
- `to_cv`/`from_cv`：wit 生成 `value` ↔ `cordis_value::Value` 全变体（Flag/Count/Offset/Text/Blob）双向往返，无遗漏。
- **trait 边界转换正确**（抽查 lib.rs:243-287）：
  - `Host::submit`：params 经 `to_cv`（wit→内部）✓
  - `Host::take`：结果经 `from_cv`（内部→wit）✓
  - `ContextHost::get`：内部经 `from_cv` 返回 ✓
  - `ContextHost::set`：wit 经 `to_cv` 存入 ✓
- **内部统一**：`PendingSet`/`RemotePending`/`bindings`/`remote_results` 全用 `cordis_value::Value`。

### 3. 互通达成（关键）
- wasm 绑定落 store 为 **`cordis_value::Value`**（`set_dyn` 装箱统一类型）→**原生 `get_dyn` 可读**；
- 原生用 `cordis_value::Value` set → wasm 镜像同步（`sync_injected` downcast 通）；
- **`dual_backend` 双向互通测试直证**（2 passed）：原生 provider ↔ wasm consumer 经统一类型——**THEORY-MAP PR#13 "跨类型值翻译不支持"边界消除**。

### 4. cordis-value crate
- 零第三方、零 `cordis-core` 依赖、`#![deny(missing_docs)]`、workspace 成员 + Cargo.lock 登记；`Value` 枚举 `Clone/Debug/PartialEq/Eq + Send`。形态与 wit 同构——值语义零变化。

### 5. 依赖方向达成
- 原生组件经 `cordis-value` 互通（无需依赖 cordis-wasm）；`cordis-core` 零改动；workspace 回归无（父会话已验 + 本轮 clippy/fmt/doc 0）。

## 发现

### M-1（建议）
- **方案命名一致性**：计划 `docs/cordis-PRODUCTVAL-P2-PLAN.md` §4 决策点②预想「external 映射，不支持→方案 B（类型重映射）」；实施走了**转换层（方案 C）**——建议 P2-3 出口把计划的"方案 B"措辞与实现"转换层方案"对齐/记录（声明 external 与 B 均不可行、转换层为最终形态的 spike 结论）。非语义问题。

### N-1（观察）
- 转换层在 **trait 边界**（每次 guest get/set/submit/take）做 `to_cv`/`from_cv`——每次转换含枚举 match + 可能 String/Vec 移动/克隆；对高吞吐 guest 有极轻微 per-step 开销（值类型通常小，可忽略；且转换仅发生在边界，非热点）。

## 通过项（已实测）
- `dual_backend` 双向互通 2/2 + `a2_e2e`（guest 完整 take-await）+ 全套 wasm 14 集成绿（含 go_guest 19.9s）
- clippy `-D warnings` 0、fmt 0、doc 0；cordis-core 零改动；零第三方。

## 结论

**P-2 主体（方案 C）达成**：统一值类型下沉独立 crate、转换层桥接正确、双后端互通直证、依赖方向反转、零 core 改动 + 门禁全绿。建议放行 **P2-3**（THEORY-MAP PR#13 边界更新 + EXIT + 出口走查），M-1 随 P2-3 文档一并处理。
