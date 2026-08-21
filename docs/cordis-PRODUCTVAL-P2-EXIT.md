# 产品验证线 P-2 出口判定 —— 双后端值类型下沉

**依据**：计划 `docs/cordis-PRODUCTVAL-P2-PLAN.md`（决策点执行结论）；THEORY-MAP PR#13（m2 边界）；审查 REVIEW-3a3591c（PASS，M-1/N-1 已处置）。
**判定日期**：2026-08-22。

## 1. 交付与验收

- **P2-1（含 P2-0 spike）**：独立 `crates/cordis-value`（`Value` 枚举，零第三方、零 core 依赖、`deny(missing_docs)`）；spike 结论——wit external 映射不可行（wasmtime bindgen 无 deps 键、单文件单 package、with 语法受限）→ **方案 C（转换层）**；桥接 `to_cv`/`from_cv`（trait 边界 get/set/submit/take）+ 内部（pending/bindings/remote_results）统一 `cordis_value::Value`；tests 9 文件迁移 `cordis_wasm::Value`。
- **P2-2 互通**：wasm 绑定落 store 为 `cordis_value::Value`（原生 `get_dyn` 可读）+ 原生同类型 set → wasm 镜像同步（`sync_injected` 边界消除）——**dual_backend 双向互通直证**（wasm→native / native→wasm）。
- **依赖方向达成**：原生组件经 `cordis-value` 互通（无需依赖 cordis-wasm）；`cordis-core` 零改动。
- **门禁**：wasm 全套绿（lib 8 + 集成 14 含 go）+ clippy/fmt/doc 0 + workspace 无回归。

## 2. 记录

- **THEORY-MAP PR#13 m2**：边界 → 已闭环（P-2 下沉，转换层为最终形态——REVIEW-3a3591c M-1 措辞对齐）。
- **N-1（观察）**：转换在 trait 边界 per-step 极轻开销（值类型小、仅边界）——可忽略。

## 3. 出口判定

**P-2 完成**：值类型下沉独立 crate + 双向互通（跨类型翻译边界消除）+ 依赖方向修复 + 全回归绿 + 审查闭环（0 Major 未决）。→ 下一线 **P-3（Await 生产化）**，计划按纪律起草待用户确认。
