# 走查记录审查报告：commit `567a770`（M3 走查 §5.3 门禁判定）

- **审查对象**：`567a770468cc729f61736ccfcf083d5d40e855df`（docs 提交：`docs/THEORY-MAP.md` 新增「M3 走查记录」章节 + 里程碑走查记录表 M3 行、`docs/PLAN.md` M3 行标记完成）
- **审查日期**：2026-08-17（仓库时区）
- **审查范围**：三类内容独立核验——（1）论文映射准确性（§5.3 原文逐一对照走查表 11 行）；（2）实现一致性（证据地点真实存在且语义正确）；（3）门禁判定准确性（5/5 门禁证据链完整）。
- **验证手段**：`git show 567a770` 全量 diff；`/tmp/paper_full.txt` §5.3 原文（3480–3610 行）逐段核对；实读 `examples/im-bot/src/main.rs`、`examples/im-bot/src/bin/broker.rs`、`examples/im-bot/src/bin/bench.rs`、`docs/bench/M3-BENCH.md`、三份审查报告（REVIEW-d1263fa / REVIEW-bbb252a / REVIEW-d457b60）；`grep` 逐一定位走查表引用的测试名与符号；实跑 `cargo run --quiet -p im-bot`（exit 0）与 `cargo run --quiet -p im-bot --bin broker`（exit 0）。wasm 构建/测试与全 workspace clippy/test 按纪律跳过。

---

## 一、论文映射准确性（11 行逐条核对）

§5.3 原文与走查表的 11 行「论文段落」逐段比对结果：**引文准确、无张冠李戴、无过度对应**。

| # | 走查表段落 | 论文原文核对 | 判定 |
|---|---|---|---|
| 1 | 规模与代表性（4000+ 插件、IM 适配器/数据库驱动/控制台/终端功能） | §5.3 首段："accumulated over 4000 community-contributed plugins, ranging from instant-messaging (IM) adapters and database drivers to administrative consoles and end-user features" | ✅ 原文逐字吻合 |
| 2 | 表达性：一切功能 = §5.1 上下文原语之上的插件，宿主只贡献领域词汇 | "every feature is realized as a plugin over the context primitives of Section 5.1; Koishi itself contributes only the chatbot-domain vocabulary" | ✅ 吻合 |
| 3 | 通用性：同一模型在第二运行时（web console）重现 | "The same model reappears in a wholly different runtime: Koishi's web console is a second, independent Cordis application…" | ✅ 吻合（且已诚实标注为载体级观察，见下文） |
| 4 | 时间组合①：卸载单插件效果不需重启宿主 | "cannot unload an individual extension's effects without restarting the extension host. Koishi routinely performs this operation… its effects are withdrawn in place" | ✅ 吻合 |
| 5 | 时间组合②：逆自动组合、插件作者无需手写卸载路径（locality of concern） | "their inverses composed automatically… obtains ordered cleanup… without writing an uninstall path. This achieves the locality of concern" | ✅ 引文关键短语 "locality of concern" 逐字命中 |
| 6 | 时间组合③：HMR 保存生效、保留缓存状态与存活连接 | "the HMR engine re-applies edited plugins on save while preserving cache state and live connections" | ✅ 吻合 |
| 7 | 空间组合拓扑：adapter 提供 platform、数据库驱动提供存储、功能插件声明为共效应并访问 | "IM adapters provide access to each messaging platform, database drivers provide persistent storage, and functional plugins declare these as coeffects and access them" | ✅ 逐字命中 |
| 8 | 运行时重配置：只重激活解析依赖变化的依赖者（§3.2） | "reactivates only the dependents whose resolved dependency changed (Section 3.2)" | ✅ 吻合 |
| 9 | 依赖不可用：保持 inactive 直到出现、不报错 | "a plugin whose dependency is unavailable stays inactive until it appears, without erroring" | ✅ 逐字命中（含引号原文） |
| 10 | 跨独立作者代码组合一致（只协调共效应键） | "typically written by different authors who coordinate on nothing beyond the coeffect that connects them" | ✅ 吻合 |
| 11 | Threats to validity：存在性/采纳性结论、量化测量 = future work | "an existence-and-adoption result rather than a quantitative one; measuring… remains future work" | ✅ 吻合 |

### 重点质疑项核查

**（a）M1 双语言 guest 作为「第二运行时通用性」证据是否诚实（走查标注"载体级观察"是否充分）**

- **核查结论：诚实的、标注充分。** 论文 §5.3 的通用性论点是"同一模型在**不同运行时**（服务端 TS vs 浏览器 JS）重现，原语固定、含义留给应用"。仓库的 M1-PR14 证据是"M1-PR14 Go guest：标准 go 实现与 Rust 同语义的消费者……与 Rust/native 组件在同一 loader 互通"（THEORY-MAP L140），验证的是**跨语言载体**（Rust/Go guest 落在 wasm 载体上）这一**不同维度**的通用性——它证明"原语语义固定、各应用自行解释"，但**并未**复现论文的"浏览器第二运行时"这一具体论域。
- 走查表第 3 行对照栏明确写「**对应**（载体级证据：浏览器运行时未复现，以 wasm 载体 + 双语言 guest 作通用性观察）」，并在里程碑行与 PLAN M3 行同步标注「浏览器第二运行时未复现——以形态/载体级证据对应，非偏差」。该标注如实区分了"跨语言载体"与"跨运行时"两个轴，没有把载体级证据冒充为运行时级证据，也未把它算作"偏差"或"已复现"。**判定：标注充分、无过度对应。** 与仓库一贯措辞（M1 走查 §6.4 亦将 Go guest 归为"语言无关性"而非"运行时无关性"）一致。

**（b）「范围说明 2 项」是否确实非偏差**

- 范围说明 1（协作规模 4000+ 插件不可复现）：论文 §5.3 首段是**存在性/采纳性结论**的背景描述（"Its scale and diversity make it a representative validation… in a production setting"），其规模是论文观察到的现象，非本仓库可复现的**断言目标**。仓库以可复现迷你案例验证其**形态**（三层拓扑 + 运行时操作），属合理的验证范围收窄，非偏差。
- 范围说明 2（浏览器第二运行时未复现）：同（a），论文并未声称 Rust 实现必须复现浏览器运行时；未复现不构成语义偏差。
- 结论：2 项范围说明**确实非偏差**，判定准确。

---

## 二、实现一致性（证据地点逐一核验）

| 走查引用证据 | 实际位置 | 语义核验 |
|---|---|---|
| main.rs 三场景（切换后端/重连/依赖不可用） | `examples/im-bot/src/main.rs` 场景 1（L140–182）、场景 2（L184–237）、场景 3（L239–288） | ✅ 三层 `PlatformKey`/`DbKey`/`ReplyKey` 键 + adapter/database/bot 组件 `inject`/`provide` 声明与 §5.3 拓扑逐层对应；bot 经 `ctx.get` 访问两层；切换后端 = revision 0→1 重建 database、bot 级联重激活（fiber 不变）、adapter 不受影响；重连 = 退役→移除→重装；依赖不可用 = bot `Inactive` 不 panic、重现自动激活。**实跑通过** |
| broker.rs 注册逆 | `examples/im-bot/src/bin/broker.rs` | ✅ broker `provide = [RegKey, ServiceKey]` + `inject = []`（中央服务不因单一后备增删而重载），Backing `inject = [RegKey]` 经 `ctx.effect` 可逆注册（逆 = 撤销注册、卸载自动执行），Consumer `inject = [ServiceKey]`。**依赖方向已按 §6.2 原文重排**（对比 `git show f706aa6`，REVIEW-d1263fa major1 已修复：原"后备 provide 注册键、broker inject 硬依赖"的反向已纠正）。ExecCount 直证更新/卸载后备全程 broker/消费者效应不重执行。**实跑通过** |
| bench.rs ExecCount 局部性 | `examples/im-bot/src/bin/bench.rs` 场景 B（L447–456） | ✅ `assert_eq!(bot_count - bot_base, 1)`、`assert_eq!(adapter_count - adapter_base, 0)`、bot fiber id 不变、M 个 filler 全程 Active——直证"只重激活解析依赖变化的依赖者、与 M 无关" |
| loader `disabled_toggle_unloads_and_reloads` | `crates/cordis-loader/src/lib.rs:970` | ✅ 存在 |
| hmr `hmr_reload_applies_new_version_keeping_other_components` | `crates/cordis-hmr/tests/hmr.rs:229` | ✅ 存在 |
| `retired_component_persists_across_unchanged_apply` | `crates/cordis-loader/src/lib.rs:1512` | ✅ 存在 |

结论：**全部证据地点真实存在且语义正确，无证据失实、无张冠李戴。**

---

## 三、门禁判定准确性（5/5）

| 门禁项 | 证据链核验 | 判定 |
|---|---|---|
| 案例断言 | main.rs 三场景断言 `cargo run -p im-bot`；REVIEW-d1263fa（major1/2 需修复 → f706aa6 重排后闭环） | ✅ 闭环，实跑通过 |
| broker | `cargo run -p im-bot --bin broker`；REVIEW-d1263fa major1 已按 §6.2 重排并闭环 | ✅ 闭环，实跑通过 |
| bench 产出 | `docs/bench/M3-BENCH.md` + `bench.rs`；REVIEW-bbb252a（3 major + 4 nit → 修复闭环，三层分离测量 + 归因更正） | ✅ 闭环 |
| 处置⑩⑪⑫ 评估 | REVIEW-d457b60（通过，含 2 nit）——⑩ 退役粘滞测试 + ⑪⑫ 评估收尾；THEORY-MAP 处置行已更新 | ✅ 闭环 |
| 走查 §5.3 | 上表 11 行逐条核对无未解释偏差；2 项范围说明非偏差 | ✅ |

- **三份审查报告均存在且闭环**：`docs/reviews/REVIEW-d1263fa.md`（需修复 major → f706aa6 修复）、`REVIEW-bbb252a.md`（需修复 major → 修复）、`REVIEW-d457b60.md`（通过）。复核 `git log --oneline`：d1263fa → f706aa6（修复）→ … → d457b60 → 567a770，链路完整。
- **PLAN M3 行"完成"标记与 THEORY-MAP 里程碑行一致**：两处均标记 M3 完成、判定通过（5/5）、处置⑦ 落地 + ⑩⑪⑫ 收口、范围说明 2 项。THEORY-MAP L217（里程碑表 M3 行）与 PLAN L314 措辞一致，无矛盾。

---

## 逐条发现

### nit1. 「稳定态确认：`cargo test --workspace` 33 套测试全绿」的计数口径不可追溯

- **位置**：`docs/THEORY-MAP.md:267`（M3 走查记录程序块）。
- **现象**：文中称"33 套测试全绿"，但该数字**无任何清洁推导**：全仓库 `#[test]` 函数共 118 个（cordis-core 74 / cordis-loader 21 / cordis-wasm 13 / cordis-hmr 9 / cordis-native 1）；集成测试文件 14 个；`cargo test --workspace` 的测试二进制数约 20 个（6 个带单测的 lib + 14 个 integration）；内联 `mod tests` 块 10 个。10 + 14 = 24，10 + 118 ≠ 33，无任何组合恰好等于 33。
- **理由**：该句是走查**稳定态确认**的自报口径，不影响门禁判定本身（走查结论"通过"由 11 行映射 + 5 门禁证据链支撑，非由测试计数支撑），故定为 nit 而非 major。但与前两个里程碑的口径不一致——M1 走查用"80 测试全绿"（L172）、M2 走查用"30 二进制全绿"（L223），M3 又换为"33 套测试"，三者单位不同（测试数 / 二进制数 / 套），且"33 套"在当前仓库无对应实体，读者无法复核。
- **建议修法**：统一三个里程碑走查记录的稳定态确认口径（建议统一为"`cargo test --workspace` 全绿"并附可复算的计数定义，或改为与 M2 相同的"测试二进制"口径并把实际数量写准确），避免出现无法溯源的精确数字。

### nit2.（补充观察，不构成独立缺陷）时间组合②行引用了 `retired_component_persists_across_unchanged_apply` 作为辅助证据，语义略跨主题

- **位置**：走查表「时间组合②」行的实现证据栏末句。
- **现象**：该测试（`crates/cordis-loader/src/lib.rs:1512`）钉死的是**退役粘滞 + 条目权威**（对应处置⑩ 双向写回），与"时间组合② = 逆自动组合 / locality of concern"的主题**并不直接相关**；本行的直接论据应是前句"im-bot/broker 各组件 apply_impl 均无手写 dispose 逻辑——撤销全部经 ctx.set 绑定逆 / ctx.effect 逆自动执行"。
- **理由**：该句不影响行结论的正确性（主证据成立），属辅助证据的边际引用，读者可能疑惑为何一处退役粘滞测试出现在时间组合②。
- **建议修法**：可将该末尾句从时间组合②行移出，留主证据；或将退役粘滞测试归入处置⑩ 的叙事（里程碑行/处置行已有充分覆盖）。

---

## 总体结论

**通过。**

三类核验结论：

1. **论文映射准确性**——11 行论文段落引用逐字吻合、无误引用、无张冠李戴；"载体级观察"标注对 M1 双语言 guest 作为"第二运行时通用性"证据的诚实性充分（如实区分跨语言载体 vs 跨运行时轴，未冒充、未误判为偏差）；"范围说明 2 项"（协作规模不可复现、浏览器第二运行时未复现）**确为非偏差**。
2. **实现一致性**——全部证据地点（main.rs 三场景、broker.rs 注册逆、bench.rs ExecCount、三个测试名）真实存在且语义正确；broker 依赖方向已按 §6.2 重排闭环；`cargo run -p im-bot` 与 `--bin broker` 双 bin 实跑 exit 0。
3. **门禁判定准确性**——5/5 门禁证据链完整（三份审查报告 REVIEW-d1263fa / REVIEW-bbb252a / REVIEW-d457b60 均存在且闭环），PLAN M3 行与 THEORY-MAP 里程碑行标记一致。

发现 **1 项 nit**（"33 套测试"计数口径不可追溯）与 1 条补充观察（时间组合②行辅助证据略跨主题），**无 major**，不阻塞合并。
