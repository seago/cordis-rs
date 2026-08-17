# PR #32 审查报告（e1b97e5 + 4348054）

**审查对象**：G5 配置插值 + G6 include patches（TS 参照安全收窄）
- `e1b97e5`（code）：`crates/cordis-loader/src/config.rs`（新增，+75）、`patch.rs`（新增，+189）、`lib.rs`（+6）
- `4348054`（docs）：`docs/THEORY-MAP.md`（+1）、`docs/TS-REFERENCE-GAP.md`（+4/-2）

**对照参照**：
- `/tmp/cordis-ts/packages/loader/src/config/utils.ts`（`interpolate` / `__jsExpr` / `evaluate`）
- `/tmp/cordis-ts/packages/include/src/index.ts`（`PatchOptions` / `applyPatches`）

**验证环境**：本机工具链 rustc **1.95.0**（非提交声称的 1.97；环境差异，非代码缺陷）；`cargo test -p cordis-loader` 40 全绿；`cargo fmt --check` 干净；`cargo clippy -p cordis-loader --all-targets` 干净；`Cargo.toml` 无任何变更（零新增依赖，纪律满足）。

---

## 逐条发现

### 1. G5 `interpolate`（config.rs）

**1.1 `{{name}}` 解析正确性** —— 通过（无 bug）。

逐项走查：
- **多占位符**：`while let Some(start) = rest.find("{{")` + `rest = &after[end+2..]` 推进，正确消费连续占位符 `{{a}}{{b}}`。
- **未闭合**：`{{` 后无 `}}` → 提前 `out.push_str(&rest[start..])` 并 `return`，保留原样，正确。
- **trim**：`after[..end].trim()`，`{{ x }}` → name `x`，有测试直证（`trims_whitespace_and_handles_empty`）。
- **未解析保留**：`None` 分支 `out.push_str(&rest[start..start+2+end+2])` 原样回写 `{{name}}`（含未 trim 的原始空白），正确返回。
- **边界**：
  - `{{}}`（空 name）：`find` 定位 `}}` 于 `after[..0]`，name = ""，走 `resolve("")`（通常 None）→ 保留原样，无 panic。
  - 连续占位符：正确。
  - `{{` 内嵌 `}}`（如 `{{{a}}}`）：`after` = `{a}}}`, `find("}}")` 命中 index 2 → name = `{a}`，未支持嵌套，但属「受控占位符」既定边界，非 bug，也未在文档中承诺嵌套支持。

**1.2 "公开差异"声明诚实性** —— 诚实充分（通过）。

模块 doc 明确三点：TS `with(ctx) eval` 任意表达式求值不支持、未解析占位符**保留原样**（与 TS eval 对未定义变量 throw 不同，属宽容）、「每次（重）加载时求值」由编排方承担。与参照 `utils.ts`（`new Function('ctx','expr')` + `with(ctx) eval` 求值 `__jsExpr`）逐点对照属实。无夸大。

**1.3 resolve 回调形态** —— 合理（通过）。

`&dyn Fn(&str) -> Option<String>`：把 ctx/环境变量解析推迟给编排方，`Option` 表达「未解析」以触发宽容保留，形态与收窄语义自洽，无借用/生命周期问题（`interpolate` 仅借用 `template`，返回值独立 `String`）。

---

### 2. G6 `Patch` / `apply_patches`（patch.rs）

**2.1 【major】重复插入 bug：递归置于 `for patch` 循环内导致嵌套组 insert 被写 p 次。**

`patch_entry` 中 `out.children = out.children.iter().map(|c| patch_entry(c, patches)).collect()` 位于 `for patch in patches` 循环**体内**，每个 patch 迭代都对整棵子树**重新跑一遍全量 patch 序列**。

- 覆盖字段（name/config/revision/disabled）幂等，看不出错；
- 但 **insert 是追加、非幂等**：当目标组位于嵌套层级、且 patch 列表长度 > 1 时，子条目被重复插入 patch 数（p）次。

**实测复现**（临时集成测试，已删除）：
```
树：group "g" → group "h" → [c1]
patches = [ insert_into(Some("h"), [c2]), override_fields("ghost") ]
结果 h.children = [c1, c2, c2]   // c2 重复，期望 [c1, c2]
```

根因：外层 `for patch` 每次迭代都触发一次全量递归，`p=2` 时 "h" 被 `patch_entry` 调用了 2 次，每次各追加一次 insert。深度 D 的树在最坏情况下呈 `O(p^D)` 的重复重算，且语义错误。**该 bug 被三个既有测试完全漏掉**——它们均只用单 patch（见 §4）。

**2.2 【major】`insert_into(None, …)`（文档承诺的「根层插入」）是静默 no-op。**

`Patch::insert_into` 文档：「`id` = 目标组条目 id；**None = 根层插入**」。但实现中：

- `patch_entry` 里 `id_match = patch.id.as_deref().is_some_and(|id| id == entry.id)` —— `None` 恒为 false，`insert` 永不命中；
- `apply_patches` 只对顶层条目 `map(patch_entry)`，**没有**对标 TS `else { data.push(...insert) }` 的顶层插入分支。

实测 `insert_into(None, [b])` 之于树 `[a]`，结果仍为 `[a]`（长度 1，无 `b`）。**API 文档与行为直接矛盾**，且与 TS 参照（`id` 缺省 → `data.push(...insert)`）语义背离。

**2.3 `name` 语义与 TS 参照不一致（未声明的重解释）** —— nit（文档诚实性）。

TS `applyPatches` 中 `name` 经解构 `{ id, insert, name, ...overrides }` 抽出，仅作**匹配护栏**：`name !== target.name` 时 `skip`（warn），**从不改写** target.name。Rust 把 `name` 当作**组件改名覆盖**：`out.component = name.clone()`（`patch_matches_nested_ids_and_unknown_ids_ignored` 直接断言 component 变成 "other"）。

这是一个实在的语义分歧：TS 的 `name` 是防护/校验，Rust 的是重命名。模块 doc 罗列的「公开差异」只提了文件读写/watch/持久化 + `inject`/`intercept`/`isolate`，**未将 `name` 的重解释列为公开差异**。若改名是有意扩展，应在「公开差异」中明示；若本意对齐 TS，则是语义错误。

**2.4 字段覆盖正确性（name→component、config、revision、disabled）** —— 除 2.3 外通过。

`id_match` 判定、`config` 用 `Rc::clone` 复用、`revision`/`disabled` 直接赋值，字段映射与 `Entry` 结构（`component/config/revision/disabled`）一致。config 覆盖不自动递增 revision——文档注释「`revision` 应随变更递增，同 config 纪律」已如实说明（与既有 config 变更纪律一致，由调用方负责）。

**2.5 insert 向组插入 + 非组目标忽略 + 未知 id 忽略 + 嵌套 id 递归** —— 通过（单 patch 场景）。

- insert 命中且 `is_group()` → `out.children.extend(...)`，正确；非组目标 → 忽略（文档「同 TS warn」，Rust 无声 warn，无 logger 通道，属合理收窄）。
- 未知 id：`is_some_and` 不命中 → 跳过，递归继续，正确。
- 嵌套 id 命中：递归 `patch_entry` 覆盖组 children，正确（`patch_matches_nested_ids_and_unknown_ids_ignored` 直证）。

**2.6 补丁顺序语义（多个 patch 命中同一条目：后应用覆盖）** —— 覆盖顺序正确，但 insert 顺序/重复受 2.1 破坏（见上）。

对**同一条目**覆盖字段，同一 `patch_entry` 调用的顺序循环里后 patch 覆盖先 patch，最终值为最后一个命中 patch 的字段，顺序确定。但 2.1 的重复递归使「insert 后组顺序」在嵌套 + 多 patch 下不可信。`insert` 的 revision 纪律：「子条目 revision 由调用方给定」——`insert` 直接搬运调用方给定的 `Entry`（含其 revision），实现如实，无额外改写，文档未就此立约，OK。

---

### 3. API 形态（lib.rs re-export）

- `pub use config::interpolate` / `pub use patch::{Patch, apply_patches}` 命名与模块 doc、THEORY-MAP、TS-REFERENCE-GAP 一致，无冲突。
- re-export 位置在 `lib.rs` 顶部 mod 区，`use` 顺序无格式问题（fmt clean）。
- **builder 充分性**：`insert_into(id, children)` / `override_fields(id)` 可用，但 `override_fields` 只填 `id`（其余全 `None`），测试均靠**手工 `p.config = ...; p.revision = ...`** 逐字段赋 public 字段来完成构造。够用（字段全 pub），但无链式 builder（如 `with_config`/`with_revision`）。属可扩展性/工效 nit，非缺陷。**注意**：`override_fields` 无法表达 `id: None`（签名 `impl Into<String>` 非 Option），而 `insert_into` 支持 `None` 却因 2.2 bug 失效——两者不一致加剧了 2.2 的误导性。

---

### 4. 测试质量

- interpolate 3 个测试直证：多占位符替换、未解析/未闭合保留、trim + 空模板 + 无占位符。**覆盖良好**，直证核心语义。
- patch 3 个测试：字段覆盖（单 patch）、组插入 + 非组忽略（单 patch）、嵌套 id + 未知 id（单 patch，仅 override）。
- **缺失场景（对应的正是两个 major 的盲区）**：
  1. **多个 patch 命中同一条目**（顺序/覆盖）——无测试；即便加了，若只测覆盖字段（幂等）也探不出 2.1。
  2. **多个 patch 中嵌套组 insert**（触发 2.1 重复插入）——无测试。
  3. **`insert_into(None, …)` 根层插入**（触发 2.2 no-op）——`insert_into` 的 `None` 分支文档承诺了、却从未被测试触碰。
  4. **insert 后组内顺序**（多 patch 追加顺序）——无测试。

结论：既有测试「直证」了各自单一路径，但 patch 侧覆盖不足以致两个 major 均漏网。

---

### 5. docs 一致性

- **TS-REFERENCE-GAP**：G4 行已补「✅ 已落地（PR #30）」标记（顺带把 G4 收口），G5/G6 行补「✅ G5/G6 已落地（PR #32，安全收窄）」，并列出剩余（yaml/json 读取、watch、写回持久化= G6 后半、依赖清单=⑫）。完成标记与 PR 实际内容一致。
- **THEORY-MAP**：+1 行记录 PR #32（G5/G6 收窄落地），表格列（时间/PR/内容/理论锚点 §5.2.1/状态/备注）格式对齐既有行，无矛盾。
- **config.rs / patch.rs 模块 doc 与代码一致**：除 §2.3（`name` 重解释未列入公开差异）与 §2.2（`insert_into` 文档承诺 Root 插入但实现 nop）外，其余描述（纯变换、递归 id、非组忽略、原树不动）与代码一致。
- **公开差异声明诚实性**：yaml/json 文件读取、文件 watch、写回持久化归为「编排工具层 / 零第三方依赖纪律」——如实；TS 侧 `Include` 确实承担 `readFile`/`watch`(via `internal/update`)/`writeFile`/js-yaml，Rust 侧这些一概未实现，收窄声明诚实。但 `name` 语义分歧（2.3）未声明，属文档遗漏。

---

### 6. 纪律

- fmt：clean（`cargo fmt --check` 退出 0）。
- clippy：clean（无 warning）。
- 零第三方依赖：新模块无新依赖，`crates/cordis-loader/Cargo.toml` 未改动，`dependencies` 仍仅 `cordis-core`。
- 备注：提交声称「1.97 fmt/clippy」，本机为 1.95；属环境差异，不构成代码 defect，但「1.97 验证」的说法在本仓库 rust-version=1.95 的约束下无法复核（非本 PR 问题）。

---

## 总体结论

**不通过（需修复后合入）**：G5 interpolate 实现正确、文档诚实；G6 `apply_patches` 存在**两个 major 语义 bug**——嵌套组 insert 在多 patch 下重复插入（递归置于 patch 循环内）、`insert_into(None)` 根层插入静默失效（文档承诺 vs 实现矛盾）。二者均被现有测试（各单 patch）漏掉，且 `name` 的「重命名 vs TS 护栏」语义分歧未在公开差异中声明。

### major（2）

1. **patch.rs 重复插入**：`patch_entry` 的 child 递归置于 `for patch` 循环内 → 多 patch 时嵌套组 insert 被重复执行 p 次（实测 `[c1,c2,c2]`），并伴 `O(p^D)` 放大。修复方向：把递归移出 patch 循环（先按 id 应用全部 patch，再仅递归一次子树），或调整为先 clone、对每个 patch 只做字段/insert 判定、递归单独一趟。
2. **patch.rs 根层插入失效**：`insert_into(None, …)` 文档承诺「根层插入」但实现无顶层 push 分支，静默 no-op。修复方向：`apply_patches` 对 `id: None` 且 `insert: Some` 的 patch 执行 `entries.extend(...)`（对标 TS `data.push(...insert)`）。

### nit（1）

1. **`name` 语义分歧未声明**：TS `PatchOptions.name` 是匹配护栏（不匹配即 skip），Rust 将之实现为组件改名覆盖（`out.component = name`），且未在「公开差异」中列明；若有意扩展应补充声明，若本意对齐 TS 则为语义错误。

> 附注（不计入 major/nit）：`override_fields` 无 `id: None` 表达 + 无链式 builder，工效可改进；测试缺失「多 patch 同条目 / 嵌套组 insert / 根层 insert / insert 后组顺序」四类场景，应随修复补充回归测试。
