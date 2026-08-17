# M3-PR3 处置⑩⑪⑫ 评估收尾审查（REVIEW-d457b60）

**审查对象**：`d457b60`（fix(loader,hmr)：新增测试 `retired_component_persists_across_unchanged_apply` + loader 已知边界①/③文档更新 + cordis-hmr 模块图文档⑫注记）与 `2c24c9b`（docs：THEORY-MAP 处置⑩⑪⑫ 三行评估完成 + PR #26 行 + PLAN M3 行）。

**审查范围**：对照论文 §5.2.1（Def 74 entry 语义、"the binding runs in both directions"、组条目）与实现（`crates/cordis-loader/src/lib.rs` 的 `reconcile_into`/`entry_ctx`/`annotated_ctx`/`make_loaded`/`apply_into`/`patch_isolation`、`crates/cordis-core/src/fiber.rs` 的 `Fiber::retire`/`retired`）、`crates/cordis-hmr/src/lib.rs` 的 `HashMapGraph`/`classify`/`detect`/`reload`），审查⑩测试语义正确性、⑪⑫评估论点准确性、docs 一致性与纪律。

**验证记录**：`cargo test -p cordis-loader retired_component_persists_across_unchanged_apply` 实际运行通过（1 passed）；`cargo fmt --check` 干净（exit 0）；clippy 做静态判断（新增代码仅为文档注释与一个测试函数，风格与既有测试一致，无 clippy 风险）。论文原文经 `paper/paper.pdf` 提取核实（Def 74 与 §5.2.1 段落）。

---

## 逐条发现

### ⑩ 测试正确性：通过（无发现）

对照 `reconcile_into`（`crates/cordis-loader/src/lib.rs` 第 377–456 行）与 `Fiber::retire`（`crates/cordis-core/src/fiber.rs` 第 183–186 行），测试的三段断言与实现一致：

1. **退役粘滞**：`provider.retire()` 置 `retired` 并 `refresh`（卸载其 `val` 绑定），`provider` 与 `consumer` 均为根条目（无父 fiber 级联路径），`consumer` 依赖 `val` 丢失 → 停用（`Inactive`），与断言"退役级联：consumer 停用"吻合。✓
2. **未变 apply 零操作**：第二次 `apply` 传入完全相同的条目，`apply_into` 阶段一（第 345–362 行）对 provider/consumer 的 `component`/`revision`/`isolate` 均未变 → `rebuilding`/`disabling` 均为 false → 不解除；阶段二走 `reconcile_into` 第 448–455 行"仅拦截注解变化"分支（只 `apply_intercept` + 字段等价赋值），**不检查 fiber 的 `retired` 状态、不重建** → `provider.retired()` 仍真、`consumer` 仍 `Inactive`。✓（幂等短路确不探查 fiber 活性，测试断言与实际行为精确对应。）
3. **revision 递增恢复**：`provider` revision 1→2，阶段一 `rebuilding = true` → `unload_from` → 阶段二 `make_loaded` 新建 fiber → `Active`，经 `loader.fiber("provider")` 重取新 fiber 断言 `Active`；`consumer` 依赖重新满足 → `Active`。✓（测试持有的是旧的 `Rc<Fiber>` 引用 `provider`，但修订后的断言改用 `loader.fiber(...)` 重取，避免了悬空旧 fiber 的误判。）

测试注释中"组件→条目写回缺席"的边界①语义与论文 §5.2.1 原文（"a component that revises its own configuration or disables itself has the change written back to its entry"）一致：`retire` 对应"disables itself"（Def 74 `disabled` 给 τ）的**运行时杆杠**，但不写回条目——正是边界①记录的公开差异方向，测试如实钉死该可观察语义。

### MAJOR：无

### NIT-1：⑪ "论文 §5.2.1 未声明组级 realm 语义"措辞不精确（轻微失实）

- **位置**：`crates/cordis-loader/src/lib.rs` 第 35–36 行；`docs/THEORY-MAP.md` 处置⑪ 行与 PR #26 行中"论文 §5.2.1 未声明组级 realm 语义"的同义表述。
- **现象**：论文 Def 74 明文 "**isolate** — an isolation annotation **applied to the entry's context**"，且同段明示组条目（`@cordisjs/group`）"are ordinary components resting on the registration primitive of Definition 47"，即组也是 entry、也拥有被 isolate 注解的 context。故"论文未声明组级 realm 语义"作为公开差异的**理由**不完全成立——论文声明了 isolate 应用于 entry（含组条目）的 context，实现中组 isolate 因 `GroupHolder` 空键而自然 no-op（`instantiate_group` 第 568 行的 `annotated_ctx(..., &KeySet::new())`），这实际是**实现与 Def 74 的一个真实偏差**，而非"论文未声明"。
- **理由**：⑪/边界③ 的核心结论——组级 isolate 候选语义"继承至子树（最近优先）"需 `effective-isolate` 穿透 instantiation（`entry_ctx` 第 503–529 行对组只走 `derive + intercept`，不应用 isolate）与 Algorithm 7 `patch_isolation`（第 631–714 行只处理**叶子** isolate 变更，组 isolate 变更在 `reconcile_into` 第 405–412 行走"整棵重建"分支、不进入 patch）**两条路径**，且因组无组件声明键集、需从子树收集键而产生 realm 脱同步风险——这一技术论证**成立且与实现一致**。仅"论文未声明组级 realm 语义"这一句归因失准，论文其实是声明了 isolate 应用于 entry context 的，只是未展开组级继承/最近优先的传播细则。
- **建议修法**：将理由改为更精确的表述，例如"论文 Def 74 将 isolate 声明为应用于 entry 的 context（组亦为 entry），但未展开组级 isolate 继承至子树的传播语义；实现中组 isolate 因 GroupHolder 空键自然 no-op，构成与 Def 74 的字面偏差——候选语义'继承至子树（最近优先）'需 effective-isolate 穿透 instantiation 与 Algorithm 7 patch 两条路径（realm 脱同步风险），记录为公开差异，随 typed world/编排工具层实现"。此改动同样同步到 THEORY-MAP 处置⑪ 行与 PR #26 行。

### NIT-2：⑫ "仓库零第三方依赖纪律"表述范围失准

- **位置**：`crates/cordis-hmr/src/lib.rs` 第 30 行（"仓库零第三方依赖纪律下无 TOML/JSON 解析器可用"）、`docs/THEORY-MAP.md` 处置⑫ 行与 PR #26 行（"零依赖纪律下无 TOML/JSON 解析器"）。
- **现象**：仓库并非严格"零第三方依赖"——`crates/cordis-hmr/Cargo.toml` 明确依赖 `anyhow = "1"`，`crates/cordis-macro` 依赖 `proc-macro2`/`quote`/`syn`，`crates/cordis-wasm` 依赖 `wasmtime`/`wasmtime-wasi`/`wit-bindgen`/`anyhow`。准确的事实是：**核心算法 crate**（`cordis-core`、`cordis-loader`）零第三方依赖，`cordis-hmr` 仅 `anyhow`（错误处理，非解析器），且 `serde` 只作为 wasmtime（cranelift）的**传递依赖**存在、非 hmr 直接可用。
- **理由**：⑫ 的核心结论仍成立——hmr 算法 crate 确实无 TOML/JSON 解析器依赖可用（`anyhow` 不是解析器），手写清单解析脆弱，"`HashMapGraph` 已证算法数据驱动（`classify`/`detect`/`reload` 经 `&dyn ModuleGraph` trait 只消费 `get_imports`，适配器仅换数据来源、不触碰算法）——适配器随构建工具 crate（可引 serde_json/toml）落地"的论证与 `crates/cordis-hmr/src/lib.rs` 第 49–62/92–180/234–300 行代码完全一致。但"零第三方依赖纪律"作为前提语，若读者照字面理解"全仓库零第三方依赖"，会与 hmr 已用 anyhow 的事实矛盾；更精确的表述是"算法 crate 无解析器依赖（hmr 仅 anyhow 错误处理）"。
- **建议修法**：将"仓库零第三方依赖纪律"改为"算法 crate 无 TOML/JSON 解析器依赖（hmr 仅 `anyhow` 错误处理；`serde` 为 wasmtime 传递依赖不可用）"，同步 THEORY-MAP 两处"零依赖纪律"表述。

### ⑫ HashMapGraph 数据驱动论断：通过（无发现）

对照 `crates/cordis-hmr/src/lib.rs`：`HashMapGraph`（第 56–62 行）是 `pub struct HashMapGraph(pub HashMap<String, Vec<String>>)` 纯数据 + 6 行 `get_imports` 实现；`classify`（第 92–138 行）/`detect`（第 168 行起）/`reload`（第 234 行起）均通过 `graph: &dyn ModuleGraph` 只调用 `get_imports`，不依赖任何具体图表示。故"适配器只是数据来源替换，不触碰算法"论断与代码精确一致。✓

### docs 一致性：通过（一处表述随 NIT-1/NIT-2 联动）

THEORY-MAP 处置⑩⑪⑫ 三行、"PR #26" 行、PLAN M3 行与代码事实一致（⑩条目权威 + 退役粘滞测试、⑪候选语义与 Algorithm 7 交互、⑫零依赖→构建工具 crate、PLAN M3 标注剩余仅走查 §5.3）。仅⑪"论文未声明组级 realm 语义"与⑫"零（第三方）依赖纪律"两处措辞失准，已分列 NIT-1/NIT-2，需在三处（loader 边界③、hmr 模块图注记、THEORY-MAP 处置行与 PR #26 行）联动更正。

### 纪律：通过

`cargo fmt --check` 干净（exit 0）。新增内容为文档注释与一个测试函数，测试代码（`matches!`/`assert!`、复用既有 `entry`/`loader` helper）风格与既有测试一致，无 clippy 风险面（未引入 newtype/needless 表达式等）。

---

## 总体结论

**通过**（含 2 项 nit，均为评估结案文书的措辞精确性问题，不影响技术结论的正确性）。

⑩ 语义钉死测试与实现精确一致且实测通过；⑫ HashMapGraph 数据驱动论断与代码一致；docs 与代码事实一致；fmt 干净。两项 nit 建议在下一次 PR 中联动更正措辞（⑪ 改为"Def 74 声明 isolate 应用于 entry context、但未展开组级继承传播"；⑫ 改为"算法 crate 无解析器依赖/仅 anyhow"），不阻塞合并。
