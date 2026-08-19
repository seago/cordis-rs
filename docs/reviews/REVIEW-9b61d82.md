# 代码审查报告：commit `9b61d82`（E1 OrchestrationError 迁移，错误策略线）

- **审查对象**：`9b61d822ffda450f779db4e15f5fda0b4376abf9` — `feat(loader,hmr): E1 OrchestrationError 迁移——validate/未知组件/Clash/UnknownParent 不 panic→逐条目报告 + apply 返回 ApplyReport + report()/entry_state + HMR 下游适配 + 既有 panic 测试迁移 + 验收 #1/#2/#4/#5/#7`
- **审查日期**：2026-08-19
- **审查人**：independent-review-agent
- **审查范围**：`crates/cordis-loader/src/{lib.rs,config.rs}` + `crates/cordis-hmr/src/{lib.rs,tests/hmr.rs}`，对照草案 `docs/cordis-rs-error-strategy-draft.md` v0.2（§4/§5/§2.1）与计划 `docs/cordis-loader-error-strategy-PLAN.md` E1。（注：`src/report.rs` 类型面属 E0 `1f9d5e8`/`c555fdd`，本 commit 使用，非审查对象。）
- **验证手段**：静态阅读 + `cargo +1.97.0 test -p cordis-loader -p cordis-hmr`（loader 55/55、hmr 9/9 全绿）；clippy/fmt/doc 由委托方本地验绿。

---

## 总体结论

✅ **通过（PASS WITH NITS）**

- **major**：0
- **minor**：3（新增组报告顺序与草案 §7「组先子后」相反；`UnknownParent` 防御分支 `parent` 空串；`entry_state` 的 `FailedFiber` 在 writeback 下被 `Disabled` 遮蔽）
- **nit**：3（未变组条目无 outcome；`ApplyReport::Display` 的 Unchanged/Activated 无条目 id（E0 nit-2 延续）；HMR 保守回滚范围注记）

核心迁移（validate→Result、未知组件、Clash first-wins、UnknownParent 不 panic → 逐条目报告；apply 返回 `ApplyReport` + report()/entry_state；HMR 经 report 检测回滚）与草案 v0.2/计划 E1 逐条对齐，语义正确（未挂载 = 无写回/无 fiber/无供给占用；每次 apply 重试）、组失败 `?` 传播正确（整组未挂载、子不实例化）、**core 零改动**成立（4 文件均非 `crates/cordis-core`）、既有 `should_panic` 测试迁移到位、测试全绿。3 项 Minor 均为报告面/诊断精化，不阻塞放行 E2。

---

## 发现

### Major：无

### Minor

### M-1（建议）：新增组的报告顺序与草案 §7「组条目先于其子条目」相反

- **位置**：`apply_into` 组 `None` 分支——`Ok(Some(fresh)) => outcomes.push(EntryOutcome::Activated)` 在 `make_loaded` **返回后**推组 outcome，而 `make_loaded` 组分支内部已先递归 `apply_into(&entry.children, …, outcomes)` 推入子条目 outcomes。故新增组的 `outcomes` 顺序 = **[子…Activated…, 组 Activated]**。
- **草案依据**：v0.2 §7「协调序：组条目先于其子条目，子条目按 keyed diff 序」——实现与草案声明相反。
- **影响**：仅报告**展示顺序**与草案不符（协调执行本身正确）；不破坏语义。
- **建议**：组条目 outcome 改为在 `make_loaded` 递归子条目**之前**入队（协调序=组先子后），或将草案 §7 措辞对齐实现（子先组后 = 执行序）。二选一，建议前者（贴近「协调序」语义）。

### M-2（建议）：`UnknownParent` 防御分支的 `parent` 为空字符串，诊断三要素缺父 id

- **位置**：`registration_error` 的 `_ => EntryErrorKind::UnknownParent { parent: String::new() }`。
- **问题**：core `RegistryError` 确有 `UnknownParent` 变体（runtime.rs）；本分支若被命中，报告为「父条目 "" 不存在」，**父 id 缺失**，违反草案 §6 诊断契约（三要素含父标识）。虽 loader 协调下子条目总挂到 holder ctx（有 fiber）故该分支**约定不可达**，但作为防御分支仍应携带父信息或注明不可达，避免触发时误导。
- **建议**：`registration_error` 让调用方传入父条目 id（组内子条目 = 组 id；叶子根 = 无父/A root），或在本分支注明「loader 协调保证不可达，防御性」并给 `parent` 一个可读占位（如 `format!("<ctx fiber {:?}>", ctx.fiber())`）。

### M-3（建议）：`entry_state` 在 writeback 开启时把组件运行时失败（`FailedFiber`）遮蔽为 `Disabled`

- **位置**：`entry_state`——`if loaded.disabled { EntryState::Disabled } else if …FailedFiber…`。
- **问题**：经 `register_retire_hook`（writeback 开）的 loader，组件运行时失败 → L-Raise → `Inactive(ζ)` + 写回 `disabled=true` → `entry_state` 恒返 `Disabled`，**`FailedFiber` 变体不可见**；仅未注册 retire_hook（默认不注册）时 `FailedFiber` 可达。草案 §3 期望失败主因（`FailedFiber`）呈现，当前优先级把协调字段（Disabled）放前——语义可辩护但 `FailedFiber` 的可见性/优先级未明示。
- **建议**：`entry_state` 注释或草案明示「writeback 开启时失败条目主态 = Disabled（协调事实优先）；`FailedFiber` 仅在未写回 / 需呈现 ζ 时使用」；或调整优先级（失败优先、Disabled 次之）。建议前者（disabled 是协调事实，优先合理），只需文档化。

### Nit

### N-1（低）：未变**组条目**无 outcome（不在报告缺席列表中）

- `reconcile_into` 组分支早 `return`（仅递归子条目）不推组自身的 `Unchanged`——未变组在报告缺席（草案 §3 逐条目含组）。叶子未变有 `Unchanged`。低影响（未变组 = 零操作），报告完整性可 E2 补。

### N-2（低）：`ApplyReport::Display` 的 Unchanged/Activated 无条目 id（E0 nit-2 观察项延续）

- report.rs 已知：`Unchanged/Activated/FailedFiber` 未携带条目 id → ApplyReport Display 只在 `Failed` 行显示条目标识；草案 §6.2「每行一条：条目 x：状态」对非失败行未落全。E1 未处理（延续观察）。建议 E2 在 `EntryOutcome` 装配条目 id 或文档标注 Display 契约范围。

### N-3（低）：HMR 保守回滚范围——非 stale 条目 Clash 也触发回滚

- `hmr.rs` 注释已言明「失败条目 id 不必 ∈ stale（可能由 stale 重载引发跨条目供给变化，如 db 重载致 c Clash）」→ 任何 `Failed` 即回滚（含非 stale 条目）。保守正确（变化不确定时回滚），建议 E2/E3 验证该场景有测试覆盖（现 hmr 测试覆盖 Clash 回滚，「非 stale 条目参与回滚」未单测）。

---

## 通过项（逐条确认）

- **validate_config → Result**（config.rs）：`Err(message)` 返回、调用方报告 `ConfigValidation`；未注册 cast 跳过校验语义保持 ✓。
- **instantiate_leaf/group → Result<Rc<Fiber>, EntryError>**：ConfigValidation / UnknownComponent / registration_error（ProvisionClash→clash_info / 其他→UnknownParent）——**四报告位点不 panic** ✓。
- **make_loaded → Result<Option<LoadedEntry>, EntryError>**：disabled→`Ok(None)` 不推 outcome ✓；组失败 `?` 传播 → 整组未挂载、子条目不实例化不推（§4.5 对称 O-3）✓。
- **clash_info**：`keys` = 组件 provide ∩ `provider_of_realm` 存在键（全列，多条）；`owner` 查**当前层 map**（协调器持 `entries` 可变借用 → `entry_of` 会 RefCell 冲突——同层条目优先 + 跨层 `fiber:{id}` 兜底，**简化与草案 §5 的偏差已注释且合理**）✓。
- **v0.2 语义**：校验失败=未挂载（无写回、无 fiber、无供给占用 §4）；未挂载 → 下轮当新增 → **每次 apply 重试**（§4.3/验收 #3 语义）✓；`Unchanged` 仅「已挂载且未变」推（reconcile 末尾）——与「失败未挂载=重试」两状态不混淆（§4.6）✓。
- **apply → ApplyReport + report() 快照 + entry_state(id)**：apply 返回当前次并存最近次快照（双轨，§7）✓；entry_state 的 Inactive(Some)/Inactive(None)/disabled 分支语义正确（Inactive(None)=Loaded 合理）。
- **HMR 适配**：apply 返回 report → `Failed`（OrchestrationError）bail 回滚；`FailedFiber` 由下方 L-Raise（带 stale 过滤，REVIEW-4c6e7fc）处理——**两类失败互补、无漏检** ✓；测试从 panic 断言迁移为 Err+回滚（`hmr_reload_rolls_back_on_provision_clash`：回滚后 sum(1)、条目 c 保留）✓。
- **既有 panic 测试迁移**：`unknown_component_panics`→`unknown_component_reports_not_panic`、`config_validate_failure_panics`→`config_validate_failure_reports_not_panic`——断言改为 report 检查 + 未挂载 + 其余 Activated ✓；新验收 #1/#2/#4/#5/#7 测试存在（config_validate 报告+其余继续、group_config_validation 整组未挂载、provision_clash first-wins、unknown_component、same_key_replace 不误报）且直证 ✓。
- **core 零改动**：4 文件均非 `crates/cordis-core` ✓（统一护栏成立）。
- **门禁实测**：`test -p cordis-loader -p cordis-hmr` loader 55/55、hmr 9/9 全绿 ✓。

---

## 结论

E1（OrchestrationError 迁移 + 报告面雏形 + HMR 适配）与草案 v0.2/计划 E1 对齐，核心语义（未挂载路径、first-wins、重试、组失败传播、HMR 回滚互补）正确，无逻辑缺陷，**建议放行进入 E2**（报告面 report()/entry_state 精化 + events `loader/entry-failed` 衔接 + 重试复活验收 #3/#6）。3 项 Minor（报告顺序、UnknownParent 信息、entry_state 遮蔽文档化）可在 E2/E3 落地，不阻塞。
