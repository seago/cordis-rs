# 代码审查报告：commit `4c6e7fc`（PR #21 / M2-PR5——cordis-hmr：Algorithm 8/9/10 事务性热重载）

- **审查对象**：`4c6e7fc74e0042fa2011163adb80b3d710df502c`（`crates/cordis-hmr/src/lib.rs` +302/−6、`crates/cordis-hmr/tests/hmr.rs` +347、`crates/cordis-hmr/Cargo.toml` +2/−1、`Cargo.lock` +2）及配套 docs 提交 `c18a14f62de46577bb960dc4f0dcc9b13eacd3ad`（`docs/PLAN.md` +2/−1、`docs/THEORY-MAP.md` +3、`docs/reviews/REVIEW-24bfab5.md`、`docs/reviews/REVIEW-ef57804.md` 入库）
- **审查日期**：2026-08-17（仓库时区）
- **验证手段**：`git show 4c6e7fc` / `git show c18a14f` 逐行核对 diff；读 `crates/cordis-hmr/src/lib.rs`（`classify`/`get_dependencies`/`detect`/`Hmr::reload`/`ModuleGraph`/`ModuleLoader`）与 `tests/hmr.rs`；对照 `cordis-loader` 的 `Entry`/`Loader::{apply,register_component,component_of,fiber}`（`crates/cordis-loader/src/lib.rs` 1192 行起的现行版本）、`cordis-core/src/runtime.rs`（`register`/`reload`/`unload`/`provider_of`）与 `fiber.rs`（`FiberState`/L-Raise `Inactive(Some(ζ))`）逐处核实依赖的时序语义。**实跑**：`cargo test -p cordis-hmr`（**6 passed / 0 failed**）、`cargo test -p cordis-loader`（**20 passed / 0 failed**）、`cargo test -p cordis-core`（全绿）、`cargo clippy -p cordis-hmr -p cordis-loader -p cordis-core --all-targets -- -D warnings`（干净）、`cargo fmt --all -- --check`（exit 0）。**未跑 wasm 测试/全 workspace**（HMR 不触碰 wasm 后端，非回归）。

---

## 结论：**有条件通过**

PR #21 实质落地了论文 §5.2.2 的三阶段事务性热重载，核心语义正确且忠实：

- **Algorithm 8（`classify`）**：stashed/externals 种子不动点，pending 保留自身 + 扩展导入，自 stashed 向下传播（种子 = stashed 的导入），单调地扩张 `accepted ∪ declined` 保证有限终止，残留未决（含环）默认拒绝——与 Alg 8 原文逐条吻合。空导入集（叶子）经 `imports.iter().all(declined)` 空真判定为 declined，是"全部导入被拒则拒"的自然退化，测试已固格式化语义。
- **Algorithm 9（`detect`/`get_dependencies`）**：依赖树以 declined 为边界截断，`tree ∩ accepted ≠ ∅` 判定过期，过期树并入本地 `accepted` 副本驱动后续条目的间接过期判定——与 Alg 9 一致。
- **Algorithm 10（`Hmr::reload`）**：备份（`component_of` 取出旧组件）→ 逐个 `load`+注册新组件 → revision+1 仅对过期条目做 `apply`（其余条目零操作、状态保留）→ 失败检测（加载 `Err` 或 `Inactive(Some(ζ))`）→ 回滚（恢复旧注册 + revision+2 重新 `apply`）。加载失败路径（load 先于 apply，部分 load 失败即早退，从不 apply）与组件运行失败路径（L-Raise 同步落 `Inactive(Some(ζ))`，apply 返回后即可检）均经测试直证。

发现 **1 项 major**（事务 panic 安全缺口——`apply` 可 panic 且不进入回滚，违背"永不半重载"的完整保证）与若干 nit（多为覆盖/文档口径，不阻塞合入）。详见下文。

> **范围说明**：c18a14f 提交消息称"THEORY-MAP 三行"，diff 实际新增 3 行（PR #20 审查、PR #21、PR #19 审查），与描述一致；"PLAN M2 主目标开工"在 c18a14f 时点为 `进行中（主目标已开工）`，现行 HEAD 上的 PLAN M2 行已由后续 PR #22/#23 推进为 `通过（走查完成）`——属后续演进，非本 PR 缺陷。

---

## 🟠 major

### major1. 事务 panic 安全缺口——`apply(&bumped)`/`apply(&rolled)` 可 panic 却不进入回滚，违背"永不半重载"的完整保证

**位置**：`crates/cordis-hmr/src/lib.rs:262`（`self.loader.apply(&bumped)`）、`:292`（回滚 `self.loader.apply(&rolled)`）；配合 `crates/cordis-loader/src/lib.rs:286-292`（`Loader::apply` 返回 `()`，错误以 panic 表达）、`:508-513`/`:526-539`（`entry_ctx`/`instantiate_leaf` 的 `unwrap_or_else(|| panic!(...))`）、`crates/cordis-core/src/runtime.rs:189-197`（`register` 对供给相交返回 `Err(ProvisionClash)`）

**事实**：
1. `reload_all` 闭包（`:245-272`）只捕获 `ModuleLoader::load` 的 `anyhow::Err`（`:247` 的 `?`）与失败检测循环的 `bail!`（`:268`），**不捕获 panic**。`self.loader.apply(&bumped)`（`:262`）返回 `()`，其内部错误全部以 panic 表达：未知组件（`entry_ctx` `:508-513` `panic!("未注册的组件")`）、实例化失败（`instantiate_leaf` `:539` `panic!("条目实例化失败")`）、供给冲突（`register` `:196` 返回 `Err(ProvisionClash)` → `:539` panic）。
2. 因此若新版本模块的 `provide`（导出）集发生变化，与**其它存活条目**的供给相交（例：旧模块提供 `{val}`、新版本提供 `{val, other}`，而 `other` 恰由第三个条目提供），`apply(&bumped)` 会在阶段二实例化处 panic。此时：新组件已 `register_component` 进注册表（`:248` 循环已执行完），部分 fiber 可能已重建、部分未重建——**注册表与 fiber 均进入半重载状态**，而 `match reload_all()` 的 `Err` 分支（`:276-294`）不触发（panic 不落 `Err`），事务无回滚。
3. 同样的缺口存在于回滚路径本身：`apply(&rolled)`（`:292`）若因某种原因 panic（理论上回滚用旧组件、旧供给集，不应有 clash，但无守卫），会导致"已恢复注册表、但 apply 未完成"的剥离态。

**为何是 major 而非 blocker**：(1) 本 PR 的**招牌保证**正是"永不进入半重载状态"（模块 doc `:19`、`Hmr::reload` doc `:219` 均明示），而 panic 路径绕开了该保证；(2) 加载错误（`load` 的 `Err`）与组件运行失败（L-Raise）两条失败通道已正确事务化，唯独**配置错误通道（panic）**未纳入事务——这与 `Loader` 自身"配置错误 = panic = bug"的模型一致（`apply` `:281-282` doc 也如此声明），但 `Hmr::reload` 既已承诺 `anyhow::Result` + 回滚，却未捕捉 `apply` 的 panic，属**不对称地半成品事务**；(3) 模块导出集随 HMR 变化是生产化（native 动态链接 / wasm wit 接口变更）的真实场景，供给冲突并非纯理论。

**影响（非 block）**：加载失败 + 组件运行失败两条主线均正确回滚、测试直证；供给冲突触发的 panic 缺回滚是边界缺口。**处置建议**：(a) 在 `reload` 中用 `std::panic::catch_unwind(AssertUnwindSafe(|| self.loader.apply(...)))` 包住正向与回滚 apply，将 panic 归一为 `anyhow::Err` 走同一回滚路径（回滚本身 panic 则 `resume_unwind`，避免吞噬宿主 bug）；或 (b) 在文档/已知边界明确声明"供给集变更导致的 ProvisionClash 属配置错误、按 panic=bug 处理、不走事务回滚"，与 `Loader::apply` 的既有约定对齐。**倾向 (a)**（与 M2-PR1 在 `reload` 内部已用 `catch_unwind` 区分 L-Raise 的先例一致，事务语义更完整），并补一个"新版本供给与第三条目冲突"的负向用例验证回滚完整。

---

## ⚪ 细节（nit）

### nit1. 回滚 `revision + 2` 为隐式启发式，无注释解释其必要性——依赖"严格大于失败 apply 已提交的 +1"

**位置**：`crates/cordis-hmr/src/lib.rs:287`（`e.revision += 2`）

**事实**：回滚路径对 stale 条目 `revision + 2`（而非 `+1`），是因为组件运行失败场景下正向 `apply(&bumped)`（`+1`）**已把 `LoadedEntry.revision` 提交为 `desired+1`**（`reconcile_into` 的 `l.revision = entry.revision`，loader `:482`/`:417`），若回滚再 `+1` 则 revision 相等、`reconcile_into` 判定"未变"而**不重建**——旧组件注册表虽已恢复，但 fiber 仍停在 `Inactive`（broken），回滚失效。`+2` 恰好使回滚 revision 严格大于已提交值，强制重建。逻辑正确，但 `+2` 的动机、与 `+1` 的差异、以及对"`apply` 已提交 revision"这一前提的依赖完全未注释，后人极易误改为 `+1` 而引入回滚失效回归。建议在 `:287` 加注释说明，或改用显式"回滚无条件下 unload+重建"以消除对 revision 递增值的隐式契约。

### nit2. `stale` 可含重复项——多条目共享同一 component/url 时 `detect` 返回重复、备份与加载循环冗余

**位置**：`crates/cordis-hmr/src/lib.rs:164-172`（`detect` 的 `stale.push(entry.clone())`）、`:238-243`（备份循环）、`:246-249`（重载循环）

**事实**：`detect` 以 `entry_urls`（=`desired` 各条目的 `component`，`:230`）逐项判过期，**多条目共享同一 component 时**（如 `p1`/`p2` 均 `component = "db"`），`stale` 会含重复的 `"db"`（两次 `push`）。备份循环对重复 url 会 `component_of` 两次（第二次覆盖第一次，`HashMap` 去重、值相同）；重载循环 `load`/`register_component` 两次（第二次注册覆盖第一次，最终一致）；`bumped` 的 `stale.contains(&e.component)` 对两个条目都命中、都 revision+1、都重建——**功能上收敛到正确结果（同组件两条目都该重载），但冗余且 `stale` 返回值语义含糊（重复项）。** 建议 `detect` 去重（或 `reload` 入口 `stale` 排序去重），并补一个"两条目共享同一组件"的用例固化该语义。

### nit3. 失败检测循环遍历**全部** `bumped`（含非 stale 条目），可能把预存在的无关 `Inactive(Some(ζ))` 误判为本次重载失败

**位置**：`crates/cordis-hmr/src/lib.rs:264-270`

**事实**：失败检测 `for entry in &bumped` 对**每个**条目（不只 stale 条目——`bumped` 是全体 `desired`，仅 stale 的 revision 被 +1）查 `fiber.state() == Inactive(Some(ζ))`。`Inactive(Some(_))` 精确区分了"自身失败"（Some）与"依赖级联停用"（None），这点正确；但若某**非 stale** 条目在本次 reload 之外已因先前失败处于 `Inactive(Some(ζ))`，本次 reload 成功后仍会被误报为失败、触发不必要的回滚。属边界误判。建议失败检测只遍历 `stale` 对应的条目（或只遍历 revision 被 +1 的条目），缩小检测面。

### nit4. 测试覆盖缺口：部分成功回滚、多条目共享 url、空 stashed 空操作均无用例

**位置**：`crates/cordis-hmr/tests/hmr.rs`（现有 6 测试无上述场景）

**事实**：审查要点 3 点名的三处缺口均未覆盖：(1) **部分成功回滚**——现有 `hmr_reload_rolls_back_on_load_failure` 只有单 stale 条目，未行使"第一个 stale 成功、第二个 load 失败"时"回滚恢复**全部**（含已成功的第一个）旧组件"的完整性（代码路径正确——load 循环的 `?` 早退、apply 未执行、备份 HashMap 全量恢复——但未被直证）；(2) **多条目共享同一 url**（nit2 场景）；(3) **空 stashed 空操作**——`stale.is_empty()` 早退（`:232-234`）返回 `Ok([])` 且不触任何注册/apply，无用例断言"空 stashed 改变不了任何状态（fiber id 不变、runtime 静止）"。建议各补一个轻量负向用例，尤其 (1) 的多条目部分成功回滚最能固化"事务性"承诺。

### nit5. 若干语义耦合与退化仅在代码/测试注释点明，模块文档未上升为公开契约

**位置**：`crates/cordis-hmr/src/lib.rs:92-98`（`classify` 种子 = stashed 的导入）、`lib.rs:104-108`（空导入集经 `all(declined)` 空真 → declined）、`lib.rs:230`（`entry.component` 即模块 url 的命名耦合）

**事实**：(1) "种子 = stashed 的**导入**（而非 stashed 自身）"这一 Algorithm 8 的关键语义只体现在实现（`:92-98`）与测试注释（`hmr.rs:27-28` `:38-40`），模块 doc（`:11-14`、`classify` doc `:75-83`）未明确"stashed 自身是 accepted 种子、其导入进入 pending"的精确分层，读者易误读为"自 stashed 向下"含 stashed 全部；（2）叶子模块（空导入集）默认 declined 的退化仅测试注释说明，模块 doc 未列；（3）`entry.component`（`Entry` 的组件注册名）被直接当作模块 `url` 喂给 `classify`/`detect`/`get_dependencies`（`reload` `:230`）——"组件名 ≡ 模块 url"是同名空间耦合，生产化（cargo metadata 的 module path ≠ loader 组件注册名）需在 `ModuleLoader`/`reload` 之间引入 url→component 映射，目前无 doc 点明这一 M2 边界。均为文档口径，建议在模块 doc 的"已知边界/假设"小节集中声明。

---

## 正面确认（实现正确的点）

### Algorithm 8/9/10 忠实性（核心结论）

- **classify 不动点正确终止**（`:100-126`）：每轮 `progress` 严格单调扩张 `accepted ∪ declined`（`accepted.insert`/`declined.insert` 后置 `progress = true`），pending 每轮重建（保留未决自身 + 扩展其导入 ∖ (accepted ∪ declined)）；因图有限、集合单调，必在有限轮内 `progress == false` 退出，残留 pending 经 `declined.extend(pending)` 默认拒绝（环即经此路径）。空导入集 `all(declined)` 空真 → declined，是"全导被拒则拒"的正确退化。
- **detect 的间接过期正确**（`:160-175`）：`accepted` 为本地克隆，`tree.extend` 并入后，后续条目若依赖树触碰先前已并入的过期树，同样被判过期——这是 Alg 9 "依赖树 ∩ accepted" 在**多条目顺序扫描**下的正确传播（`classification.accepted` 本体不被污染，`reload` 只消费返回的 `stale`）。
- **reload 备份/失效映射忠实**（`:238-243`）：论文 `invalidate_caches(accepted)` 的缓存 = loader 组件注册表，`component_of` 取出旧组件即"失效 + 备份"；恢复（`:278-281`）重注册旧组件。旧 fiber 的退役由 `apply` 的两阶段协调（revision 变更 → `unload_from` → 重建）承接，而非显式 `dispose`——机制不同、结果等价（旧 fiber `retire` + `remove_fiber`），与 THEORY-MAP/模块 doc 的"以 loader 注册表 + apply 为缓存"表述一致。

### 失败检测时序正确性（审查要点 2 核心回应）

- **组件运行失败（L-Raise）在 apply 返回时已同步定型**：`instantiate_leaf` → `use_component` → `register` → `refresh` → `reload`（`runtime.rs:375-422`），组件迭代器 raise 经 `catch_unwind` 识别 `FiberError` 载荷 → `unload` 恢复已完成步骤 → 终态 `Inactive(Some(err))`（`:408`），**全程同步**，故 `apply` 返回后 `reload` 的失败检测循环（`:264-270`）读到的是稳定的失败态，无竞态/时序错位。
- **`Inactive(Some(_))` vs `Inactive(None)` 的判别正确区分自身失败与级联停用**：broken `p` 落 `Some(ζ)`，被通知级联停用的依赖者 `c` 落 `None`（`unload` 的 `:449` 分支）。因此失败检测循环以 `Some(_)` 判定"本次重载引入的失败"，不会把受牵连的正常停用误判为失败——`hmr_reload_rolls_back_on_component_failure` 的回滚后 `c` 恢复 Active、`sum(1)` 恢复，直证依赖者经惯性链（`reload` 的 `ctx.notify` → 依赖者重激活）而非 loader apply 重建。

### 门禁用例非假阳性

- `hmr_reload_applies_new_version_keeping_other_components`（`hmr.rs:229-270`）：断言 `c_now.id() == c_first`（consumer fiber id 不变 = 不重建）+ `sum(v2:1)`（新版本 provider 生效）+ `p_new` 新 fiber、`runtime.is_quiet()`——三重实质判定，区分了"重建"与"静默旧态"。
- 两个回滚用例（`hmr.rs:274-306`、`:310-346`）：加载失败用例断言 `sum(1)` 恢复 + `c` fiber 不变 + `p` fiber 变化（`assert_ne!`，证明回滚经重建而非原地保留）+ 静止；组件失败用例同样断言旧版本恢复 + consumer Active + 静止。非假阳性。

### 回归与卫生

- **测试**：`cargo test -p cordis-hmr` 6 绿、`cargo test -p cordis-loader` 20 绿、`cargo test -p cordis-core` 全绿（含 Thm 59/61/66/73 四组）。
- **fmt/clippy**：`cargo fmt --all -- --check` exit 0；`cargo clippy -p cordis-hmr -p cordis-loader -p cordis-core --all-targets -- -D warnings` 干净（无历史警告残留）。
- **workspace/锁文件**：`crates/cordis-hmr` 已在 `Cargo.toml` members（`:6`）；`cordis-hmr` 新增依赖 `anyhow`/`cordis-loader`（`Cargo.toml`）+ `Cargo.lock` 正确补 `anyhow`/`cordis-loader` 依赖行。
- **文档一致性**：THEORY-MAP PR #21 行（`:147`）准确概括三阶段、`ModuleGraph` 双实现 + 生产化适配器边界、双回滚直证；PLAN M2 行在 c18a14f 时点为"主目标已开工 + 门禁用例直证"（现行 HEAD 已由后续 PR 推进为"通过"）；crate 模块 doc（`lib.rs:1-57`）的 M2 边界（"cargo metadata / wit import 解析为生产化适配器"）与 THEORY-MAP 一致。REVIEW-24bfab5/REVIEW-ef57804 入库（c18a14f）与 THEORY-MAP 的"PR #19 审查/PR #20 审查"行对应。

---

## 总结

- **blocker**：无。
- **major**：major1（事务 panic 安全缺口——`apply` 的供给冲突/未知组件/实例化失败以 panic 表达、不进入 `reload` 的回滚路径，违背"永不半重载"的完整保证；宜以 `catch_unwind` 归一或明确记录为 panic=bug 边界）。
- **nit**：nit1（回滚 `revision+2` 隐式启发式无注释，依赖"严格大于已提交 +1"）、nit2（多条目共享 url 时 `stale` 重复、备份/加载冗余）、nit3（失败检测遍历全 `bumped` 可误判预存在失败）、nit4（缺部分成功回滚/多条目共享 url/空 stashed 空操作测试）、nit5（"种子=stashed 的导入"、"叶子默认 declined"、"组件名≡url"三处语义耦合未入模块 doc）。

**结论：有条件通过。** 置信度：高——逐行审读 `lib.rs` 302 行与 `tests/hmr.rs` 347 行，并回读 `cordis-loader`/`cordis-core` 的 `apply`/`register`/`reload`/`unload`/`FiberState` 逐处核实失败检测与回滚的时序依赖；实跑 hmr 6、loader 20、core 全绿、clippy/fmt 干净。Algorithm 8/9/10 三阶段忠实、加载失败与组件运行失败两条事务主线正确回滚且测试非假阳性、门禁用例（保存即生效 + 双回滚）直证。唯一 major 属"配置错误通道未纳入事务"的边界缺口——不阻塞功能合入，但建议随本 PR 或紧随 PR 以 `catch_unwind` 归一 + 负向用例补齐，使"永不半重载"的保证对所有失败通道成立（见 major1 处置 (a)）。
