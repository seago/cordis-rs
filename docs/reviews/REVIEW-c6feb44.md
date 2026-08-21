# 代码审查报告：commit `c6feb44`（A2b G1+G2——wit 显式 variant 化 + rust 系适配）

- **审查对象**：`c6feb4426643121c780037f6811b115ff19309c5` — feat(wasm): A2b G1+G2（wit `effect-step` 改显式 `variant { step(inverse), done(inverse), wait }` + 宿主三分支 + rust 适配 + 断言）
- **审查日期**：2026-08-22
- **审查人**：independent-review-agent
- **范围**：`crates/cordis-wasm/{wit/cordis.wit, src/lib.rs, tests/load_guest.rs}` + `examples/wasm-plugin-rust&&-consumer/src/lib.rs`；对照 A2b 计划 `docs/cordis-wasm-A2B-PLAN.md` G1/G2。

---

## 总体结论

✅ **PASS WITH NITS（Major 0 / Minor 1 / Nit 3）→ 放行 G3**

- **核心目标达成**：`option<resource>`（`inverse` 可选）这一导致 `invalid option discriminant` 的编码坑已被 **wit 结构消除**（variant 显式化），为 G3（go guest 修复）铺路——本 commit 后宿主 wit 不再含 `option<inverse>`。
- 宿主三分支语义正确、guest 语义闭环、回归全绿、**不改 core**。

---

## 核查（逐项）

### 1. variant 语义与宿主三分支（通过）
- wit：`variant effect-step { step(inverse), done(inverse), wait }`——`step`=有逆步继续 / `done`=终止（可带最后逆）/ `wait`=等待。**设计决策 D-1 采用推荐 B 形态的变体**（`done(inverse)` 带载荷，保留最后逆表达力）。
- `WasmTaskIter::next`：`Step(_)→Yielded(step_inverse)`、`Done(_)→Finished(step_inverse)`、`Wait+在途join→Step::Await`、`Wait 无join→Yielded`、`None→Finished`——与 forward_pending 机制一致。

### 2. 载荷逆 vs 真实逆通道（通过，重要）
- 真实逆（执行核心 unbind/notify）来自 **`forward_pending`**（guest `set` → `Host.set` 记录 pending rep → 迭代器 step 后转发核心）；**variant 载荷逆（`Step(_)/Done(_)`）宿主不消费其 rep**（`_` 忽略）——**无双重执行**。
- 与 A2a 既定机制一致（A2a 的 record `inverse: Some(...)` 同样由 host 忽略、走 pending）——非本 commit 引入。句柄驻留属既有已知边界（REVIEW-2a7a686 m3 相关，`core_inverses`/资源表单调）；见 N-2。

### 3. Await 判定闭环（通过）
- `Wait` 只在 guest 的 take 轮询分支产出；take 前提是 `submit` 过 → 必有在途 join；join 未回填时才 take→None→`Wait`；回填后 take 就绪→`Done`。故 `Wait+在途join→Await` 语义闭环，无悬挂 `Wait`。防御分支 `Wait 无join→Yielded`（空逆步继续）正确。

### 4. rust guest 适配（通过，附澄清）
- 主 guest：`submit` 步→`Step(inverse)`、take 就绪→`Done(inverse)`、take 未就绪→`Wait`——三态完整。
- consumer：单步 `Done(inverse)`；misbehave/panic：`step()` 返回 `None`/`panic`（**不构造 `EffectStep`**）→ 天然兼容、无需改。
- **澄清（非缺陷）**：任务描述"rust 4 guest 适配"实际仅 **2 个构造方**（主 guest + consumer）需改；misbehave/panic 不构造该类型。stat 只含这两者，正确。

### 5. 断言适配（通过）
- `load_guest`：step0 `Step(_)`、等待步 `Some(Wait)`（variant 断言）——与 guest 行为一致；`a2_e2e` 断言 store 侧结果（probe = worker tid）——与 guest `EffectStep` 构造解耦，无需改（guest take-await 语义未变）。

### 6. 门禁实测
- `cargo +1.97.0 test -p cordis-wasm` 全套绿（**lib 7 + 集成 12**，go_guest 2 项 `#[ignore]`）；父会话验 clippy/fmt/doc 0、workspace 无回归、**core 0 diff**（本 commit 不触 crates/cordis-core）。

---

## 发现

### Minor

### M-1（建议，观察项）：`step`/`done` 的折叠逆宿主未显式消费其 wit 句柄
- `EffectStep::Step(_)/Done(_)` 载荷逆句柄 host 忽略（只走 pending rep）。host 侧资源表对该句柄（own handle 跨边界传入）未显式 drop → **驻留至实例卸载**（每 step 一个）。与 A2a 既有机制一致（非回归），且对核心逆语义无影响（真实逆走 pending）。
- **建议**：不在本 commit 修复（避免引入无关改动）；列为观察——若 guest 步数多/长驻，可在 G4 出口记录或后续让 host 在 `forward_pending` 后显式 `drop` 载荷句柄（wit 资源表回收），与 REVIEW-2a7a686 m3 的"单调边界"一并处置。

### Nit

- **N-1**：`Wait => Step::Yielded(step_inverse)` 防御分支（wait 无在途 join）无注释——建议补一句（罕见路径；`take` 前提是 submit 故正常必有 join）。
- **N-2**：`EffectStep` 的 `pub use` doc 略简（A2b variant 形态说明）——可再精确到"真实逆经 forward_pending 流转；载荷逆仅 wit 形态携带"。
- **N-3**：任务文档/提交信息说"rust 4 guest 适配"，实际仅主+consumer（misbehave/panic 不构造）——措辞可精确为"rust 构造方（主/consumer）适配"，避免后续审计误解。

---

## 通过项（确认清单）
- wit variant 消除 `option<resource>` → **G3 前置完成** ✓
- 宿主三分支 + Await 判定闭环（wait 前提 submit）✓
- 载荷逆 host 忽略、真实逆走 forward_pending（无双重、与 A2a 一致）✓
- guest 三态适配（主/consumer 构造方）+ 断言匹配 ✓
- 不改 core；回归全绿 ✓

## 结论

**G1+G2 达成，放行 G3**（go guest 适配：重生成 go 绑定 + plugin.go 映射 variant 三态 + `go_guest` 2 测试去 `#[ignore]` 转绿 + 双语言回归）。M-1 与 N-1/N-2 为观察/文档级，G4 出口一并处置即可。建议 G3 前置确认 go 绑定重生成路径（build.sh 管线是否自动重跑 wit-bindgen go）。
