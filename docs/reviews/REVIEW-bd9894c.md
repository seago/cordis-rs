# 代码审查报告：commit `bd9894c`（PR #15 处置③——Thm 59/61 直接测试）

- **审查对象**：`bd9894c4378c7a707f56e6b25a351e42ffadd791`（`crates/cordis-core/tests/preservation_recovery.rs`，+467）及配套 docs 提交 `5ceeaaf`（`docs/THEORY-MAP.md` +50、`docs/PLAN.md` +1/−1，M1 走查记录 §6.2–6.4 全文）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show bd9894c` / `git show 5ceeaaf` 逐行核对 diff；读 `preservation_recovery.rs`（467 行全量）；对照 `runtime.rs`（register/O-Insert 前提、refresh/unload 路径、remove_fiber 前提）、`fiber.rs`（FiberState 形状、parent/inject/provide/state 访问器）、`context.rs`（use_component/set/set_dyn/resolve_realm）、`keyset.rs`（intersects）、`effect.rs`（execute/Disposer）；读 THEORY-MAP「M1 Wasm 后端走查记录」全文（§6.2–6.4 追踪表 + 处置清单①-⑨）；实跑 `cargo test -p cordis-core --test preservation_recovery`（**3 passed / 0 failed**）、`cargo fmt --all -- --check`（exit 0）、`RUSTFLAGS="-D warnings" cargo clippy -p cordis-core --test preservation_recovery --all-targets`（干净）。

---

## 结论：**通过**

处置③（Thm 59/61 直接测试）实质落地，四条款断言与实现语义一致；M1 走查记录证据与实现可对照、处置清单完整、门禁"通过（含处置清单）"判定成立。发现 1 项 major（文档诚实性：走查记录⑧的关键子句为**未经直证的外推**）与若干 nit，均不阻塞合入。

> **范围说明**：审查基准为 PR #15 的**已提交状态**（`bd9894c` + `5ceeaaf`），而非当前含 PR #16 与未提交改动的工作树。审查中发现工作树存在未提交的后续工作（`sandbox_isolation.rs` 新增 `guest_undeclared_set_panic_is_caught_and_host_survives` + 未跟踪 example `wasm-plugin-rust-misbehave/`），恰为「把记录⑧的外推转为直证测试」的落地——这一事实本身佐证了 m1 的成立。

---

## 🟠 建议修复（major）

### m1. 走查记录⑧的关键子句「（sandbox 测试证明该边界可捕获）」为**未经直证的外推**，而非当时已有测试事实
**位置**：`docs/THEORY-MAP.md:171`（§6.3 追踪表「补查风险点」行）

**事实**：该行断言恶意 guest 越界 `context::set` 写未声明键 → 核心 `set_dyn` 的 Def 43/48 纪律 `panic!`，并称「被宿主侧 `catch_unwind` 边界捕获（**sandbox 测试证明该边界可捕获**、宿主存活）」。但在 PR #15 的**已提交状态**下，`sandbox_isolation.rs`（`crates/cordis-wasm/tests/sandbox_isolation.rs`）仅含 1 个测试 `guest_trap_is_caught_and_host_survives`，测的是 guest **自身在 `task.step()` panic → wasmtime trap** 路径（trap 是 wasm1 语义的 guest 内错误，由 wasmtime 转 `Result` 错误），**并未**测「越界 set → 宿主 `set_dyn` panic」路径——后者是**宿主 Rust panic**，产生机制与 trap 不同（`forward_pending` → `set_dyn` → `panic!("组件 … 越界写入未声明的键 …")`，`context.rs:162/219`），需一个专门写未声明键的恶意 guest 才能触发，而该 guest 当时**不存在**（`wasm-plugin-rust-misbehave` 为审查时点尚未入库的未跟踪 example）。

**为何是 major 而非 blocker**：(1) 记录⑧整体被正确框定为「补查风险点 / 已知边界 / 随⑤处置」，**未声称已解决**；(2) 外推的**结论实际正确**——审查中把未跟踪的 `wasm-plugin-rust-misbehave` guest 构建后，实跑 `guest_undeclared_set_panic_is_caught_and_host_survives` **通过**，证明「越界 set → 宿主 panic 确可被 catch_unwind 捕获、宿主存活」。问题在于**文档把「推断」写成了「sandbox 测试证明」的事实**——括号里的背书指向了一个当时并不覆盖此路径的测试，与仓库一贯的「表述失准需更正」纪律（M0 走查曾主动更正 3 处表述失准）不符。

**建议**：既然直证已随后续未提交测试通过，将 L171 括号改为如实表述（如「与 trap 同路径由宿主 catch_unwind 边界兜底（PR #15 时为外推，后续 `guest_undeclared_set_panic_is_caught_and_host_survives` 直证通过）」），并把该测试 + example 正式合入，消除「外推当证据」的口径落差。

---

## ⚪ 细节（nit）

### nit1. Thm 59 条款 (1)（`π_n ∈ dom(Fγ) ∪ {root}`）中「非 root 父指针」子句**事实性空检**
**位置**：`crates/cordis-core/tests/preservation_recovery.rs:157-166`

**事实**：全部 fiber 均经 `use_at(&root, ...)` 在根上下文（`root` 的 `fiber: None`，`context.rs:86`）下实例化，故 `f.parent()` 恒为 `None`（= root），`if let Some(parent)` 分支在整条编排序列中**一次都不执行**。四条款中「π 落在 `dom(Fγ)`（非 root 父指针）」这一最低限子句未被任何断言实际行使。

**影响（非 block/major）**：该不变式在引擎侧由 O-Remove 的 `HasChildren` 前提（`runtime.rs:251-253`）结构性维系——父在子存续期间不可被移除，故"非 root 父必在 registry"被引擎强制维护，测试漏检不会掩盖真实回归；`unknown_parent_rejected`/`remove_preconditions` 片段 + property 已间接护住。但作为**专门**验证 Thm 59 四条款的直接测试，条款 (1) 只验了 trivially 成立的 root 半边，与测试自我声明的「四条款逐动作断言」存在口径落差。建议补一个「子 fiber 挂在非 root 父下」的场景，或在文档注释如实标注条款 (1) 仅覆盖 root 情形。

### nit2. 条款 (1) 父指针校验遍历**测试 shadow `registry` Vec**而非引擎侧 `dom(Fγ)`
**位置**：`crates/cordis-core/tests/preservation_recovery.rs:158-164`

**事实**：`registry.iter().any(|(id, _)| *id == parent)` 只认测试自己 `push` 的列表，而非 `runtime.fiber(parent)`。对「父已从运行时 registry 移除、但 id 恰仍留在 shadow 列表」的情形会**假阳性**通过（parent 不在 `dom(Fγ)` 却判「在注册表」）。本场景的编排顺序不会触发该假阳性，故当前无实际影响；但语义上「条款 (1) 应判 π ∈ dom(Fγ)」却判了「π ∈ 测试 shadow 列表」，断言源不严谨。建议改为 `runtime.fiber(parent).is_some()`（并与 root 判别合并）。

### nit3. Thm 61 文档「观测等价对照态」措辞 vs 逐键 store 断言的实现口径落差
**位置**：`crates/cordis-core/tests/preservation_recovery.rs:8-11` 对 `:381-401` / `:438-465` 断言链

**事实**：测试实际以 `store.contains(s("k0"))` 等**全局绑定表逐键存在/缺失**间接验证「某一 fiber 退役只撤回它自己的绑定」（每绑定 realm 键控于 fiber 派生上下文、退役沿 `fiber.dispose` LIFO + `ctx` 累加器仅撤该 fiber 注册的效应，逐键检查与「只撤自己贡献」等价）。这是**忠实且有效**的观测等价代理——但模块文档 L8-11 声称的「最终状态 == 其余 fiber 单独推进的状态」并未以字面形式构造对照态（无第二套 runtime 单独推进其余 fiber 后 diff）。属「实现比文字朴素但结论等价」，非缺陷；建议文档措辞对齐实现（「逐键断言贡献存续/撤销」）。

### nit4. 门禁结论行内联清单跳号 `⑧`（折叠进⑤的括号）
**位置**：`docs/THEORY-MAP.md:148`（里程碑走查记录「M1 Wasm 后端」结论行）

**事实**：处置清单表（L186-196）序号①-⑨**完整无缺**；但 L148 里程碑结论行的内联清单在 ⑤/⑥/⑦/⑨ 间**跳过了⑧**（⑧被折叠进⑤的「（失败模型实现时，含 ⑧ 的处置）」括号）。读者不点进处置清单表会误以为门禁结论行漏了⑧。建议 L148 内联清单也显式列出「⑧ 恶意 guest 越界 set → 宿主 panic 兜底（随⑤）」以逐条对齐。

### nit5. 稳定态确认「77 测试全绿」为**过期计数**（PR #15 实为 80）
**位置**：`docs/THEORY-MAP.md:153`

**事实**：「77 测试」是 PR #14 审查（`docs/reviews/REVIEW-b5131a9.md:5`）的计数，PR #15 新增 `preservation_recovery.rs` **3 个测试**，故走查稳定态确认真实值应为 **80**。测试确实全绿（实跑验证），仅计数沿用 PR #14 未更新。属文档精确性小事，建议随后续 PR 顺手更正（或改为「全 workspace 测试全绿」去除易腐化的绝对数）。

### nit6. PLAN M1 行「其余 8 项成为 M2 首批任务」口径——⑦ 实为 M3 而非 M2
**位置**：`docs/PLAN.md:312`（PR #15 提交态，见 `git show 5ceeaaf` 的该行）

**事实**：9 项处置 − ③已落地 = 8 项，其中 **⑦「§6.2 broker 示例」去向为 M3 案例素材**（THEORY-MAP 处置清单表 L194），并非 M2。故「其余 8 项成为 M2 首批任务」把 ⑦ 也计入了 M2，比实际（7 项 M2 + 1 项 M3）多算 1 项。属里程碑表口径的轻微不严，建议改为「其余 7 项成为 M2 首批任务 + ⑦ 为 M3 案例素材」。

---

## 正面确认（实现正确的点）

### Thm 59 四条款断言的忠实性（核心结论）

- **条款 (2)**（`m ≠ n ⇒ p_m ∩ p_n = ∅`）**忠实且含退役未移除者**：断言遍历 `members`（= `registry` 中仍满足 `runtime.fiber(id).is_some()` 者，恰等于 `dom(Fγ)`），两两 `provide().intersects()` 断言不相交（L169-178）。退休未移除的 fiber 仍留 `dom(Fγ)`、其 supply 名仍占用（O-Insert 的 `∀m ∈ dom(Fγ)` 覆盖全部 fiber，`runtime.rs:184-192` 的 `ProvisionClash` 检查），测试注释 L284-285 明确点到这一语义——与实现**一致**，无假阳性/假阴性。步骤 4（退役 b→c 级联停用）后 b 仍带 k1 supply 名、步骤 5 重连前先 `remove_fiber(b)` 释放该名后才允许 b2（亦提供 k1）注册，整条链路正确行使了「退役 supply 名占用直至移除」这一最关键边界。
- **条款 (3)/(4) 仅对 Active**：以 `let FiberState::Active { view } = ... else { continue }`（L181-184）严锁 installed（Active）fiber，注释 L145-147 明示 Unloading/Reloading 为转换中间态、同步引擎动作后已静止故不出现——与文档「clause 3/4 仅对 Active」**相符**。(3) 以 `dom(ω) == d`（`view.keys() ⋃ == inject`）验证全函数、取值 `view.values()` 逐一 `runtime.fiber(*provider).is_some()` 验证落 `dom(Fγ)`（L185-202）；(4) 对 provider 判 `FiberState::Active` = installed_m（L203-212）。三子断言忠实覆盖 Def 58(3)(4) 语义。
  - 注：`dom(ω)==d` 断言由 `resolve_view`（`runtime.rs:455-462`）按 `fiber.inject.iter()` 构造而**近同义反复**（引擎自身保证 dom(view)≡inject），故不脆弱但为较弱检查——不构成假阳性，仅覆盖强度有限（标注于此供 reviewer 知情）。
- **场景编排覆盖全面**：激活链 a→b→c（依赖链）、独立提供者 d、退役级联、移除+重连（b2/c2）、二次退役级联、清场（L239-328），6 段编排每段后均 `assert_well_formed`，末尾 `remove_fiber` 逐一执行后仍断言并以 `runtime.is_quiet()` 收束——四条款在「含退役未移除者的中间态」与「清场后空态」两端均被覆盖，显著强于 M0 走查记录的片段式覆盖（THEORY-MAP:47）。

### Thm 61 恢复精确性

- **「只撤自己的贡献」确有检验**：三纤维交错（a k0 / c 注入 k0 提供 k1 / b k2 / d 注入 k1）+ 中间退役 b 后，逐键断言 k0（a 的贡献）、k1（c 的贡献）保留、k2（b 的贡献）撤销（L381-393），再退役 c 断言 k1 撤、k0 仍在、d 级联停用（L395-401），最终退役 a 全清。第二测（`retiring_oldest_first`）反向顺序验证各 fiber 贡献独立存续（L438-465）。两测均实质行使了式 (56)「逆只撤回自己贡献、其余状态不变」的核心断言。
- **组件纪律（Def 43 声明 + 效应）正确**：`Consumer::apply` 在 provide 非空时经 `MultiIter` 逐键 `bind`（L105-116），多键提供者同构；`once_finished()` 处理零提供、零效应 consumer——杜绝「声明供给却不实际绑定」的假组件污染四条款/恢复断言。

### M1 走查记录（THEORY-MAP:150-196）与实现的一致性

- **§6.2**：排他绑定 = `ProvisionClash`（`runtime.rs:45/184-192`）+ loader 两阶段 apply（`same_supply_replacement_in_single_apply`，`cordis-loader/src/lib.rs`）准确；broker「可表达但无示例」如实记为处置⑦、非未解释偏差。
- **§6.3**：wasm 能力面=import 面、镜像仅 inject 键（`sync_injected` 结构性强制、`get` 未声明=None）、`set_dyn` Def 43/48 纪律、沙箱=wasmtime SFI+trap 捕获宿主存活（门禁 2/3 实测）——均与 PR #14 已合入实现对应；越界 panic 边界诚实标为新增记录⑧（但见 m1 的「直证 vs 外推」口径问题）。
- **§6.4**：逆句柄化 + Wasmtime 原生 embedder 丢弃即释放（与论文 "released when a native embedder drops it" 逐字一致）、Rust 过程宏路径 + 运行时符号级动态解析（新增记录⑨）——证据与代码（`WasmComponent::load`、`cordis-macro` `#[component(inject=[..], provide=[..])]`）对应。
- **处置清单完整**：①-⑨ 共 9 项全部列出且去向明确（①/② M2 首批、③ 已落地 PR #15、④ M2、⑤/⑧ M2 失败模型实现时、⑥ 记录 M2 评估、⑦ M3、⑨ M2 typed world 评估），承 M0 清单并在 L147/L148 间有清晰传递。
- **门禁「通过」判定成立**：拦截求值形态缺口（§6.3 L169）被正确分类为「承 M0 清单①、非 M1 新增、非未解释偏差」——该缺口是**已记录的既有偏差**（处置①），非 M1 走查暴露的未解释新偏差，故「逐节无未解释偏差」+「通过（含处置清单）」判定逻辑自洽（符合 PLAN §7 的「存在未解释偏差→不通过」规则）。

---

## 总结

- **blocker**：无。
- **major**：m1（走查记录⑧把未经直证的「越界 set → 宿主 panic 可被捕获」外推写成「sandbox 测试证明」——外推结论虽经审查实测正确，但文档诚信口径需随直证测试合入后修正）。
- **nit**：nit1（条款 1 非 root 父分支事实空检）、nit2（条款 1 父指针遍历 shadow 列表而非 dom(Fγ)）、nit3（Thm 61 文档「观测等价对照态」措辞与逐键断言实现的口径落差）、nit4（门禁结论行内联清单跳号⑧）、nit5（「77 测试」过期计数，实为 80）、nit6（PLAN「其余 8 项 M2」把 ⑦ 误计入 M2）。

**结论：通过。** 置信度：高——逐行审读全 467 行测试与 `runtime.rs`/`fiber.rs`/`context.rs`/`interp.rs` 语义对照，实跑 3 测试全绿、fmt/clippy 干净；Thm 59 条款 (2)(3)(4) 与 Thm 61 恢复精确性均忠实且实质行使，条款 (1) 的「非 root 父」子句被结构性空检（有引擎 HasChildren 前提兜底、非回归风险）；M1 走查追踪表证据与实现一一对应、处置清单 9 项完整、门禁含处置清单判定经「拦截缺口=承 M0 清单①」正确归类而成立。m1 为文档诚信口径问题（外推未加直证背书），不构成功能缺陷，修正后即完全通过。
