# M3-PR2 基准报告审查（REVIEW-bbb252a）

**审查对象**：`bbb252a`（feat(im-bot)：`examples/im-bot/src/bin/bench.rs` + `.github/workflows/ci.yml`）与 `b8a680d`（docs：`docs/bench/M3-BENCH.md` + `docs/THEORY-MAP.md` PR #25 行 + `docs/PLAN.md` M3 行）

**审查范围**：对照论文 §5.1.2（Algorithm 2/3 notify 传播）与 §5.3（"reactivates only the dependents whose resolved dependency changed"），审查 bench 的方法学严谨性、断言质量、数据诚实性、CI 门禁、docs 一致性与纪律。

**审查依据**：静态通读两个 commit 全量 diff + `git show`；对照 `crates/cordis-core/src/runtime.rs`（`notify_fibers` 全表扫描 + `refresh` early-return，第 304–319/326 行）与 `crates/cordis-loader/src/lib.rs`（`apply_into` 阶段一 `desired.iter().rev().find()` 逐条目逆扫，第 337–354 行；`reconcile_into` 幂等短路第 376–448 行）；实跑 `cargo run --quiet -p im-bot --bin bench`（debug + release 双跑通过）+ 用临时 probe 二进制（已删除，未入库）额外验证 no-op re-apply 与纯 diff 成本。

**验证记录**：`cargo fmt --all -- --check` 干净；`cargo clippy -p im-bot --all-targets -- -D warnings` 干净；bench debug/release 双跑全断言通过。

---

## 逐条发现

### MAJOR-1：场景 A `t_off`/`t_on` 复用同一 loader，中位数测的是 loader 的 O(N²) entry-diff，而非 notify 传播

- **位置**：`examples/im-bot/src/bin/bench.rs` 第 215–236 行；`docs/bench/M3-BENCH.md` L12（"每次重复 **fresh loader**（避免二次 apply 的 no-op 幂等）"）。
- **现象**：文档声称"每次重复 fresh loader 防 no-op"，但该约定只在 `t_act`（第 207–213 行，闭包内每次 `fan_system()` 新建）与场景 B（每轮 `Loader::new`）成立。`t_off`（第 228 行）与 `t_on`（第 236 行）**复用第 215 行创建的同名 loader**：第 1 次 `apply(&off)` 完成真实停用（provider `disabled` 置位 → 级联 N 个 fan Inactive），第 2–5 次命中 `reconcile_into` 的幂等短路（`loaded.disabled == entry.disabled` → `if entry.disabled { return; }`；fan 叶子 component/revision/disabled 均未变 → 落入"仅拦截注解变化"分支只做 config 拷贝），**不触发任何重建/级联**。
- **量证**：临时 probe（已删除）测得 N=1000 时——真实停用第 1 次 ≈4.95ms，其后的纯 no-op re-apply ≈3.16–3.26ms（三次稳定）。即：中位数（5 次采样排序取第 3 个）落在一个 no-op 采样上，测到的是 **loader 协调的 O(N²) diff 成本**（`apply_into` 阶段一对每个已载条目做 `desired.iter().rev().find()`，第 339 行），而非 Algorithm 3 的 notify 传播。报告 release 表"停用 3.60ms / 再激活 3.56ms（N=1000）"≈ 纯 diff 成本（probe2：N=1000 纯 no-op re-apply ≈3.2ms），停用传播本体实际 ≈1.8ms，diff 占约 2/3。
- **理由**：`t_off`/`t_on` 三列中的两列没有测到其目标路径（§5.1.2 的绑定撤销→级联停用 / 恢复→级联激活），测得的是与研究问题无关的 loader 协调开销；且文档对此做了相反陈述（"fresh loader 防 no-op"）。
- **建议修法**：为 `t_off`/`t_on` 也每次 fresh loader——把 `loader.apply(&all)` 与 `loader.apply(&off)`/`loader.apply(&all)` 的序列包进闭包（每次重复新建 loader 并完整走 all→off 或 all→off→all），或直接只测"首 apply 建树 + 单次 transition"的总和再减建树项。修正后重跑 release 数据并更新表格。

### MAJOR-2：超线性残差归因错误（数据不诚实）——"45×" 来自 loader 的 O(N²) diff，而非 Algorithm 3 O(F) 扫描

- **位置**：`docs/bench/M3-BENCH.md` L30（"这是 **Algorithm 3 的逐 live fiber 扫描**…"）、L32（"停用/再激活的残差更明显（100→1000 约 45×），同因：每次撤销/重绑各触发一次全表扫描，且级联 teardown 串行化（RefCell 借用）"）；`docs/THEORY-MAP.md` L153（"超线性残差归因 Algorithm 3 O(F) 逐 fiber 扫描"）。
- **现象**：报告把停用/再激活 100→1000 ≈45× 归因于 Algorithm 3 全表扫描及"RefCell 串行化"，但从未提及 loader 本身的 O(N²) entry-diff 这一主导项。
- **反驳**：(1) `notify_fibers`（runtime.rs 第 304–319 行）对**每次** binding 变更只触发**一次** O(F) 全表扫描，且 `f.ctx.resolve_realm(ik)` 是 O(1) HashMap 查（context.rs 第 121–123 行）→ 单次 notify 严格线性，停用只撤回一个绑定只发生一次 notify，O(F)=O(N) 线性，无法解释 45×（10× N 的线性增长应为 10×）。(2) probe 实测证明 45× 的绝大部分是 `apply_into` 阶段一 `desired.iter().rev().find()` 的 O(N²) diff（N=100→500→1000 的纯 no-op re-apply 为 77µs→853µs→3.2ms，呈二次增长）。
- **理由**：报告的**核心定量卖点**（"对 §5.3 future work 的量化补充"）的归因与实现不符，会把读者引导到错误结论（误以为运行时 notify 传播是二次代价）。这是数据不诚实（omit 主导项 + 错误归因），非表述瑕疵。
- **建议修法**：补测并如实记录 diff 主导项；若目标是量化 §5.1.2 notify 传播，则隔离测量（fresh loader + 单 transition，并从测得值中减去纯 diff 基线），并对"超线性残差"给出与实现一致的归因（loader 阶段一 O(N²) diff；notify 本体为 O(F) 线性扫描，另记录在案）。

### MAJOR-3：场景 B "切换延迟" 同样被 O(N²) diff 主导，"Algorithm 3 O(F) 扫描主导"归因错误

- **位置**：`docs/bench/M3-BENCH.md` L34（"切换延迟随 M 增长（≈ 2.5 µs/组件）来自 Algorithm 3 的全表扫描：每个无关填充组件至多被 O(1) 的目标比较扫描并 early-return"）、L40（"每填充组件边际成本 ≈ 2.5 µs/组件"）。
- **现象**：M=500 实测 1.11ms（probe2：M=500 纯 no-op identical re-apply ≈0.85ms），同量级；per-filler 边际成本 100→500 由 ≈1.0µs 抬升到 ≈2.49µs，呈超线性——与"O(F) 线性扫描"不符，与 O(N²) diff 相符（阶段一逐条目逆扫 desired）。切换 apply 的 `switched` 除 database revision 0→1 外还带全部条目，`apply(&switched)` 必然执行阶段一 O(N²) diff。
- **理由**：`ExecCount` 直证的定性结论（bot 恰 1 次、adapter 0 次、填充全程 Active、与 M 无关）是**正确且诚实**的，重激活局部性成立；但"切换延迟随 M 增长的**定量**来源是 O(F) 扫描"这一归因是错的——增长主要来自 loader diff，不是 notify 扫描。
- **建议修法**：定量段落改为"切换 apply 总成本由 loader 阶段一 O(N²) entry-diff 主导；notify 重激活本体（O(F) 扫描 + O(受影响者) 重激活）经 ExecCount 与 M 无关的第三条路径单独直证"。或附加一个不触碰 diff 的隔离测量（如直接 `runtime.notify_fibers` / `refresh` 微基准）来量化 O(F) 扫描本身。

---

### NIT-1：「debug 余量 >60×」失实

- **位置**：`docs/bench/M3-BENCH.md` L54（"取自 debug 实测余量 >60×"）；`docs/THEORY-MAP.md` L153（"debug 余量 >60×"）。
- **现象**：按报告自报 debug 实测（N=1000 激活 ≈101ms 对 500ms 上界 ≈5.0×；M=500 切换 ≈6.9ms 对 200ms 上界 ≈29×），两者均远小于 60×，没有任何一路达到 60×。实跑 debug 亦印证（94.9ms / 7.10ms）。
- **建议**：改为真实余量（"N=1000 约 5×、M=500 约 29×"），或删去该表述。

### NIT-2：近线性门禁过松且只覆盖干净的 `t_act`

- **位置**：`examples/im-bot/src/bin/bench.rs` 第 247–252 行；`docs/bench/M3-BENCH.md` L15。
- **现象**：`t_act < prev_act * 25 + 20ms` 只对 `t_act` 生效：(1) `+20ms` 绝对偏移使 N=10→100 档（`prev_act`≈55µs、`t_act`≈534µs）的门禁阈值 ≈21.4ms，远大于实测，形同虚设；(2) 25× 倍数能拦截 O(N²)（100×/十倍增），但拦不住 O(N^1.5)（≈31.6×/十倍增）等中等超线性；(3) 对被污染的 `t_off`/`t_on` 无任何 scaling 断言，报告里"停用/再激活近线性"的图表其实是 diff 主导且无门禁约束。
- **建议**：若保留此门禁，收紧倍数并说明其只能防"二次型回归"这一有限目标；为修正后的 `t_off`/`t_on` 增补同类近线性断言；绝对上界作为 CI 防抖安全网是合理的（约 5×/29× 余量足以吸收抖动），但其"余量 >60×"的配套声称需按 NIT-1 更正。

### NIT-3：场景 A 缺 `assert_quiet`，与场景 B 不对称

- **位置**：`examples/im-bot/src/bin/bench.rs` 第 228–243 行之间。
- **现象**：停用/再激活后只校验了 fan 个体的 `FiberState`，未像场景 B 那样 `assert_quiet(&runtime, …)` 校验终态静止；状态机是否完全收敛到静止未被覆盖。
- **建议**：停用后、再激活后各加一次 `assert_quiet`（需在闭包外持有 runtime 引用，或改为 fresh-loader 方案后一并校验）。

### NIT-4：THEORY-MAP 错别字 + 归因/余量表述需要同步修正

- **位置**：`docs/THEORY-MAP.md` L153。
- **现象**："电机重激活局部性"实为"直证重激活局部性"（"电/直"形近误植）；同句还带入了 MAJOR-2 的错误归因（"超线性残差归因 Algorithm 3 O(F) 逐 fiber 扫描"）与 NIT-1 的"debug 余量 >60×"。
- **建议**：修复错别字；在上述 major/nit 修复后同步更正该行归因与余量表述，保持 THEORY-MAP 与 M3-BENCH.md 一致（仓库惯例：改代码与改报告拆两 commit，docs 联动更正）。

---

## docs 一致性核验

- **数据与实跑一致**：M3-BENCH.md release 表与我实跑 release 输出吻合（激活 12.3/55.1/534.5µs/9.24ms vs 实测 11.76/56.5/536/9.71ms；切换 14.8/115.3µs/1.11ms vs 实测 14.32/115.5/1.147ms；debug ≈101ms/≈6.9ms vs 实测 94.9/7.10ms）——数字本身是诚实录录的，**问题在归因而非数值**。
- **THEORY-MAP PR #25 行 / PLAN M3 行**：与提交事实一致（PR #25 标记完成、PLAN M3 标注基准报告完成），仅 L153 有 NIT-4 错别字与需联动更正的归因/余量表述。
- **§5.3 引文**：报告 L9 引用 "reactivates only the dependents whose resolved dependency changed" 与论文 §5.3 原文一致；L8 引用 "existence-and-adoption result… future work" 与论文 Threats to validity 段落一致。

## 纪律核验

- **零第三方依赖**：`examples/im-bot/Cargo.toml` 仅依赖工作区内部 `cordis`/`cordis-core`/`cordis-loader`，未引入 criterion，符合仓库纪律。✓
- **fmt/clippy**：`cargo fmt --all -- --check` 与 `cargo clippy -p im-bot --all-targets -- -D warnings` 均干净。✓
- **CI 门禁 step**：在现有 `run im-bot (M3 案例)` 之后追加 `cargo run --quiet -p im-bot --bin bench`，语法正确；debug 下 N=1000≈95ms、M=500≈7ms，5+3+5 次重复总量约零点几秒，数量级完全可接受。✓
- **未运行**：wasm 构建/测试与全 workspace clippy/test 按纪律跳过（静态审查 + 单 bin 验证足够）。

---

## 总体结论

**需修复 major（3 条）。**

核心问题集中在**方法学（MAJOR-1）与数据归因（MAJOR-2/3）**：`t_off`/`t_on` 复用 loader 使中位数测到的是 loader 的 O(N²) entry-diff 而非 notify 传播；报告随后把这 45× 超线性残差与场景 B 切换延迟的 M-增长都归因于"Algorithm 3 O(F) 全表扫描"，而实现在 `crates/cordis-core/src/runtime.rs::notify_fibers` 中确为**单次 O(F) 线性扫描**（`resolve_realm` O(1)），O(F) 无法解释 45×——真实主导项是 `crates/cordis-loader/src/lib.rs::apply_into` 阶段一 `desired.iter().rev().find()` 的 O(N²) diff，报告从头到尾未提及。

值得肯定的是：`ExecCount` 直证"bot 恰 1 次、adapter 0 次、fiber 不变、与 M 无关"的**重激活局部性定性结论是正确且诚实**的（实跑全断言通过）；`t_act`（fresh loader）的近线性测量是干净且成立的；数据数值本身如实录录；零依赖、fmt/clippy、CI step 皆纪律合规。

修复按仓库惯例拆 **code + docs 两 commit**：code commit 修正 `t_off`/`t_on`（fresh loader 或隔离基线）并补 `assert_quiet`；docs commit 更新 M3-BENCH.md 数据（重跑 release）+ 更正归因（把 O(N²) diff 主导项如实记录、notify O(F) 扫描与重激活局部性分别直证）+ 更正"余量 >60×"+ THEORY-MAP 错别字与联动表述 + PLAN 不变。
