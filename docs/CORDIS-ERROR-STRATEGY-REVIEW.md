# cordis-rs 错误策略草案评审记录（loader 三级错误分类）

- **评审对象**：`docs/cordis-rs-error-strategy-draft.md` v0.1（初稿，待评审）
- **评审日期**：2026-08-19
- **评审人**：review-agent（对照 `crates/cordis-loader`/`crates/cordis-core` 既有实现与 G1 语义）
- **范围**：§0–§10 + 附录（三级分类 / 现状迁移 / 新类型 / FailingComponent / first-wins / 报告面 / 边界 / 验收 / 开放问题）

---

## 总体结论

**方向正确、复用充分（判定公理清晰、边界明示、零新通道），但存在 1 项正确性冲突（Major-1：幂等/粘滞语义与现状 reconciler 的 disabled 分支实质不符）与 1 项迁移面遗漏（Major-2：组条目校验失败分支缺失）**——需修订到 v0.2 后方可动工。

- **Major**：2
- **Minor**：3
- **Nit**：3

通过项：`FiberError::new(msg).raise()` 为 core `pub`（fiber.rs:42/55）——**core 零改动成立**；判定公理「panic 保留 ⟺ 用户输入不可达」清晰；§8 边界清单明示不转理由；§9 验收直证性强；报告面 D siagnostic（Display 三要素）与 events 衔接列为 integration，均合理。

---

## 发现

### Major

### M-1（正确性）：「同 revision apply 不重跑 / 报告 Unchanged」（§4 注 2、验收 #5）与现状 reconciler 的 disabled 分支实质冲突，未核实

- **位置**：§4 流程 2「同 revision apply 不重跑（评审已确认的退役粘滞语义）」；§9 验收 #5「desired 未变时失败条目不重跑、不重复写回、报告 `Unchanged`」。
- **问题**（对照实现，lib.rs `reconcile_into`）：`if loaded.disabled != entry.disabled { … else { disabled 清除 → 先 unload_from（退役 fiber 仍在 registry）→ make_loaded 重实例化 } }`。失败组件经 retire hook **写回 `disabled = true`**（G1），而 desired 的 disabled 未变（仍 `false`）→ **下一次 apply 必然判定 `loaded.disabled(true) != entry.disabled(false)` → 进 disabled 清除分支 → 重跑失败组件**（再次 L-Raise / 再次失败 / 再次写回），并报告 `FailedFiber` 而非 `Unchanged`。「退役粘滞」只在**单次 apply 内**成立（写回发生在 apply 末尾 retire_pending 排空，早于下一次 reconcile）；跨 apply 则每次都重试。草案的「同 revision 不重跑」与现状**不符**。
- **草案/依据**：§4 注 2、§9 验收 #5 vs 实现 `reconcile_into`（lib.rs:577-598 的 disabled 分支）+ G1 写回（register_retire_hook）。
- **建议修法（二选一，草案须选明）**：
  1. **接受「每次 apply = 重试机会」语义**：失败条目在 desired 未变时每次 apply 重跑并再次报告 `FailedFiber`（= 天然自动重试；复活 = 修配置（要么 revision bump 要么 config 变化）后重跑成功）；验收 #5 改为「失败条目每次 apply 重试并报 `FailedFiber`、不 crash、不吞原因」。
  2. **实现「失败粘滞跳过」**：Make_loaded 前检查 fiber `Inactive(Some(ζ))` 且 desired 未变 → 跳过重跑、报 `Unchanged`/`FailedFiber` 保持（需在 loader 增加粘滞判定——新增机制，草案声明「零新增机制」将不成立，须重述）。
  - 推荐 **方案 1**（零新增机制、与「复活 = 修配置」一致），但草案 §4 注与验收 #5 措辞必须改（当前为「不重跑/Unchanged」，与实现冲突会误导实现）。

### M-2（迁移面遗漏）：组条目的校验失败分支缺失——`validate_config` 在 `instantiate_group`（lib.rs:802）同样调用

- **位置**：§4 只覆盖 `instantiate_leaf` 的 FailingComponent 路径；但 `validate_config` 亦在 `instantiate_group`（lib.rs:802，组持有者 GroupHolder 注册前）调用——组条目校验失败现状同样 `panic!`，迁移后**走哪条通道、组与子条目如何呈现**未定义。
- **问题**：组不是叶子（无真实组件，用 GroupHolder），FailingComponent（d/p 取自 entry）对组条目并非直接适用——组的失败语义（整组 Failed 且子条目不挂？子条目逐条报告？组 failed 时子代是否仍实例化）草案未言明。
- **建议修法**：§4 增补「组条目校验失败」——建议组条目失败走 **OrchestrationError 类**（报告 `Failed(EntryError)`，整组跳过实例化，子条目不挂；或按 O-3 逐子报告）或给组设计 Failing 变体；明确组失败与子代的关系，并列入验收（如验收 #1 补「组条目校验失败 → 整组 Failed、子不挂、其余条目继续」）。

### Minor

### m-1（建议）：报告汇聚机制未说明——`ApplyReport` 从递归 `apply_into` 的逐层汇聚如何落地

- §3 定义 `ApplyReport`/`EntryOutcome`，§7 说 `Loader::report()`（最近一次 apply 副本）；但 `apply_into` 是递归 per-layer（组 → 子组），outcomes 的收集/排序/「逐条目报告」的汇聚位置（叶子实例化点 + 组条目 + 子条目展开）未设计。建议 §7 增补：`apply_into` 每次调用汇入共享 Vec（或返回值），组条目展开子条目顺序 = 协调序；`report()` 存最近 apply 快照。

### m-2（建议）：迁移回归面未列——既有依赖「配置错误 panic」的 loader 测试须改写

- 现有一批测试以 `#[should_panic]` 断言未注册组件 / ProvisionClash / 校验失败（如 lib.rs:1749「G7 校验失败 = 配置错误（panic）」等）。迁移为报告后这些测试的断言形态改变，草案未列改写清单/回归护栏（除验收 #8 只覆盖 core 供给纪律 panic 边界）。建议 §9 增补「既有 should_panic 测试迁移表」（哪些改为报 `Failed`/`FailedFiber` 断言）。

### m-3（建议）：ProvisionClash 的冲突键获取未细化

- `RegistryError::ProvisionClash` 为 unit（runtime.rs:45，不带载荷）；`EntryErrorKind::ProvisionClash { key, owner }` 的 `key` 需 loader 自行推断（当前条目 provide 与既有 registry 提供键的交集）+ `owner` 反查。§5 只提 owner 反查，`key` 来源未言明（可能多条相交）。建议 §5 注明「key = 当前条目 provide 与既有注册提供键的交集（可多条；报告首条 or 列出全部）」。

### Nit

### n-1（可选）：FailingComponent 的注解应用未定义

- 失败组件用真实组件的 d/p，但 isolate/intercept 注解是否同样应用到 Failing 条目（annotated_ctx）未说；失败组件不激活，注解影响小，但建议注明「FailingComponent 也走 annotated_ctx（与真实路径一致）或明示跳过」。

### n-2（可选）：failed 条目的 `provision` 占用与维护的长期语义

- FailingComponent 保留供给名防二次冲突（§4 流程 1）——但失败条目长期占有供给名是否阻碍「另一组件在后续 apply 提供该键」？建议注明（fallback：失败条目不占、但同批内重名条目报 Clash——与两阶段冲突处理的一致性）。

### n-3（确认）：`EntryOutcome`/`EntryState`/`ApplyReport` 为 loader 新 pub API

- loader 是已审查 crate；新增 pub 类型属演化，在 loader 里程碑审查覆盖（非 core）；OK。

---

## 通过项（逐条确认）

- **core 零改动成立**：`FiberError::new`（fiber.rs:42）、`pub fn raise(self) -> !`（:55）均 pub；`FailingComponent` 纯用 core 既有 L-Raise（reload 的 catch_unwind 识别）✓。
- **判定公理**：panic 保留 ⟺ 用户输入不可达——边界清单（§8）逐条理由成立（核心供给纪律 = 作者义务、内部一致性 = 前置已检查、async/events 守卫 = 冻结协议纪律）✓。
- **复用 G1 全链路**：L-Raise → `Inactive(Some(ζ))` + retire hook 写回 disabled + `update_fiber` 复活——均为已落地件（M0.4/M2）✓；FailingComponent 保留供给名防同批二次冲突（§4 流程 1）✓。
- **first-wins 与两阶段**：卸载侧先释放供给名 → 同键替换不报 Clash（lib.rs 两阶段已如此）✓；后到者 Failed 报告确定性 ✓（唯一需补 key 来源，m-3）。
- **诊断契约**：Display 三要素 + Clone/PartialEq（§6）——报告面/测试断言合理 ✓。
- **报告面与 events 衔接**（§7）：loader 查询面 + events `entry-failed` 列为 integration、events crate 已落地（P1.1）可接；HMR 保持现状（O-4）合理 ✓。
- **验收 #1–#8**（除 #5 需按 M-1 修改）：单条目失败不中断、写回复活、first-wins、未知组件、Display、同键替换、panic 边界——直证性强 ✓。

---

## 结论

v0.1 的**分类框架与复用策略正确**（核心公理、G1 复用、first-wins、报告面），但 **M-1（同 revision 幂等/粘滞与现状 reconciler 冲突——验收 #5 与实现矛盾）与 M-2（组条目校验失败分支缺失）** 须修订到 **v0.2**：M-1 二选一定案（推荐「每次 apply = 重试机会」）、M-2 补组失败语义 + 验收；m-1..m-3（报告汇聚、回归迁移表、Clash key 来源）在 v0.2 一并细化。修订由用户改草案（外部工作文件），v0.2 复评后冻结再按纪律开工。

---

## Addendum v0.2（复核 2026-08-19 · 冻结判定）

对 v0.2（采纳 v0.1 评审的修订稿）逐条复核：**关键决议（配置校验失败 → OrchestrationError 未挂载路径）正确消解了 M-1 与 M-2**，全部 v0.1 意见落地，无残留 Major/Minor。结论：**v0.2 具备冻结条件**。

### 复核确认（全部采纳正确）

- **M-1 ✅**：校验失败改「未挂载 + 报告 + 每次 apply 重试」（§0 决议 / §4）——从未挂载即**无写回、无 disabled 分支冲突**；§4.6 明确「已挂载未变 = Unchanged」与「失败未挂载 = 每次重试」两状态不混淆；验收 #3 直证重试与复活。v0.1 的「同 revision 不重跑」表述作废（§4.6）——诚实。
- **M-2 ✅**：组条目校验失败并入同一路径（整组 `Failed`、子条目不实例化不单独报告，§4.5，与 O-3 对称）；验收 #2 增补。
- **m-1 ✅**：报告汇聚机制（`apply_into` 每层追加协调序 Vec；组先、子后）§7；**m-2 ✅**：既有 `#[should_panic]` 迁移清单列验收 #9；**m-3 ✅**：`ProvisionClash { keys: Vec<Symbol> }` = provide ∩ 既有注册键**全列** + owner 反查首键（§5）。
- **n-1/n-2 ✅**：随 `FailingComponent` 废弃自然消解（无注解应用、无供给占用，§4 "无写回、无 fiber、无供给占用"）；**n-3 ✅**：新 pub API 归 loader 里程碑审查（确认）。
- **TS 对齐如实**：附表明示「形态不同（未挂载+重试 vs fiber 失败态）、语义等价（不崩/其余继续/可复活）」——无夸大。

### 新发现（均 Nit，不阻塞）

- **n-1'**：§7 并存「apply 返回 `ApplyReport`」与「`Loader::report()` 最近一次副本」——建议注一句「apply 返回当前次的同时写 `report()` 快照」，消除双轨读法。
- **n-2'**：运行时失败（`FailedFiber`）跨 apply 的重跑是**既有 G1 语义**（写回 disabled=true 后 reconciler disabled 分支会重跑失败 fiber——v0.1 M-1 的冲突形式对其仍在）——草案 §3 注「本草案不改其行为」已划界；建议再补一句明示「运行时失败条目的跨 apply 重跑为既有语义（延续 G1），非本草案（校验失败）路径」以防未来混淆。
- **n-3'**：未挂载失败条目的 validate 每次 apply 重跑 → 高频 apply（HMR）下报告可能的噪声/洪水——已记 O-2（告警节流待场景）✓。

---

## Addendum v0.2 结论

**v0.2 具备冻结条件**：M-1（未挂载 + 重试）根因修复、M-2（组统一）到位、v0.1 全部意见落地且无残留阻塞；3 项 Nit（§7 双轨注记 / 运行时失败既有语义明示 / 报告节流 O-2 已有）为文档表述级。

按执行纪律：冻结后开工（loader 里程碑：OrchestrationError 迁移 + EntryError/ApplyReport 报告面 + 验收 #1–#9）由用户下达。
