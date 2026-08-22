# 代码审查报告：产品验证线 P-6 出口走查（Gate B）

- **审查对象**：P-6（§6.6 版本化链接 + Progress Thm 66 定量上界 + loader desired-diff 索引化）——commits `bb75b6f`（P6-1）/ `6c2b8fa`（P6-2）/ `5597990`（P6-3）/ `f74c4e8`（EXIT）。
- **验证手段**：静态阅读 + `cargo +1.97.0 test --workspace` + `fmt --check` + `clippy --workspace --all-targets -- -D warnings` + `doc --workspace --no-deps`。
- **范围**：计划 `docs/cordis-PRODUCTVAL-P6-PLAN.md` ↔ 交付 ↔ 门禁，含等价性与"无版本键不误伤"抽查。

---

## 总体结论

**✅ PASS（WITH MINORS）**——三个交付核心正确、门禁主体绿；存在 1 项**计划/交付承诺偏差**（§6.6 命名空间方案未落地）、1 项**EXIT 门禁声明不准确**（doc 非 0）、1 项**覆盖粒度**（升级路径未直证单消费者迁移）。均为文档/覆盖粒度级，不阻塞出口成立，但应按最小修复清单落地。

- **Major**：0
- **Minor**：3（M-1 命名空间未落地 / M-2 doc 声明不准确 / M-3 升级粒度）
- **Nit**：2（上界断言保守 / THEORY-MAP 行内注记格式）

## 抽查（核心直证，全部命中）

### P6-1 §6.6 版本化链接（key@version）
- `versioned_keys_isolate_upgrade_and_conflict`（loader/lib.rs:1814）：`val_provider_v("db@1"/"db@2")` + `consumer_v("db@1"/"db@2")`——**版本隔离**（`db@1`/`db@2` 独立绑定，store 断言 `db@1`/`db@2` 各自 contains）、**升级**（c2 消费 db@2 激活，c1 不受影响）、**冲突**（p3 也提供 db@2 → 后到者 Failed(ProvisionClash)）。`cordis-PLUGIN-GUIDE.md` §8bis 文档（接口漂移防护：消费者声明版本键、升级后旧声明显式 Inactive；约束区间留 v2）。
- **无版本键不误伤**：`@` 编码使 `db@1` 为独立 Symbol——无版本键（如 `db`）是不含 `@` 的独立 Symbol，互不干扰；既有无版本键测试（val_provider 等）回归绿（workspace 无回归，见下）。

### P6-2 Progress Thm 66 定量上界
- `progress_quantitative_upper_bound`（runtime.rs:1134）：K=2 链深 2（A≺B≺C）——`total = count_a+count_b+count_c`，断言 `total >= 6 && total <= 612`。**验证 ΣB(n) 计算**：B(A)=6·(2+0)=12、B(B)=6·(2+12)=84、B(C)=6·(2+84)=516 → ΣB=612 ✓ **数值正确**；`total>=6`（结构最小步）✓。THEORY-MAP 46 行补注"P-6 补测…缺口关闭" ✓（覆盖缺口的确与"未断言→已断言"一致）。

### P6-3 loader desired-diff 索引化
- `apply_into` 阶段一：`index: HashMap<&str,&Entry> = desired.iter().map(|e|(e.id,e)).collect()`（**last-wins：HashMap 重复键保留最后一项**）+ `index.get(id).copied()`（O(1)）。**等价性核实**：原 `desired.iter().rev().find(|e| e.id==id)`（取重复 id 中最后一个）与 `HashMap` 的 last-wins（同 key 覆盖为最后）**等值** ✓；`desired_duplicate_id_last_wins`（"x" 两次出现 first/second）直证 last-wins（仅最后者挂载）。阶段二遍历 desired 每条不改（索引只替换阶段一卸载侧）✓。bench M3-BENCH 已知边界①（O(N²) 逆扫）关闭。

## 门禁实测

- `cargo +1.97.0 fmt --check` ✅ 0
- `clippy --workspace --all-targets -- -D warnings` ✅ 0（grep 计数 0）
- `cargo +1.97.0 test --workspace` ✅ 无回归（WS=0，core 61 / loader 60 等全绿）
- `cargo +1.97.0 doc --workspace --no-deps` ⚠️ **1 告警**（见 M-2）

## 发现

### Minor

### M-1（计划/交付偏差）：§6.6 的“键命名空间化”（key namespacing）未落地——交付仅版本化链接（@version，drift 维度）
- **位置**：计划 §1“键命名空间化（`name@ns`，键碰撞隔离）+ 版本化链接”——交付只做 `key@version`。§6.6 的两个问题（**interface drift** 由 @version 覆盖；**key collision**——无关提供者同键名）在交付中**未以命名空间隔离**，只靠既有 `ProvisionClash`（first-wins 报告，确定性但非隔离）。
- **证据**：EXIT §1 只提“版本隔离 db@1/db@2 共存”（版本维度），未提命名空间；PLUGIN-GUIDE §8bis 亦仅 @version。计划 §1 的命名空间方案未实现、EXIT 未明示此缺口。
- **建议**：最小修复 = 在 EXIT/§8bis 补一句“key namespacing（`name@ns`，防无关生态键碰撞）留后续（collision 现由 first-wins 报告兜底）”，或补命名空间编码实现。

### M-2（EXIT 门禁声明不准确）：EXIT §1 声称“doc 0 告警”，实测 1 告警
- **位置**：`docs/cordis-PRODUCTVAL-P6-EXIT.md` §1“clippy/fmt/doc 0”。
- **证据**：`doc --workspace --no-deps` 实测 **1 告警**——`examples/agent-plugin/src/main.rs:36` rustdoc 未闭合 HTML 标签 `<Context>`（`/// 不能捕获 Rc<Context>；...`——`<Context>` 被解析为 HTML 标签）。**来源为 P-5（agent-plugin）而非 P-6**，但 P-6 EXIT 的“doc 0”与实测不符。
- **建议**：最小修复 = `main.rs:36` 把 `Rc<Context>` 反引号化（`Rc<`Context`>`）→ doc 归 0；P-6 EXIT 门禁措辞可补“（doc 1 条历史告警见备注）”。

### M-3（覆盖粒度）：P6-1“升级”直证为“新增 c2 消费 db@2”（双版本共存），非“单消费者从 db@1 迁移 db@2”
- **位置**：`versioned_keys_isolate_upgrade_and_conflict`（1835 行）——升级 = 注册新 consumer_v("db@2") 条目 c2，而非**同一消费者条目**切换版本。
- **证据**：测试断言 c1（db@1）与 c2（db@2）共存；真实“升级”（一个 entry 的版本从 1→2）未直证（其表现为旧声明不满足 → Inactive，§8bis 描述但测试未覆盖单消费者迁移）。
- **建议**：可补一个“单消费者版本迁移”（同 entry revision 版本键切换 → 旧版 Inactive/新版激活）断言，或记录为覆盖粒度（现状足以直证版本隔离/冲突，升级语义在 §8bis 描述）。

### Nit
- **N-1**：P6-2 上界断言保守（`total=6 ≤ 612` 恒真——上界定理本质宽松；实际 6 远小于界）。可辩护（上界直证），但断言有效性弱；若要更强可断言“精确等于结构步数 + 上界成立”两段。
- **N-2**：THEORY-MAP 46 行的补测注记为**行内追加**（“记录为覆盖缺口）| **（P-6 补测…缺口关闭）**”）——同一行内状态变化，格式上可接受但可读性一般（可考虑分列/独立行）。

---

## 判定

**P-6 出口成立（PASS WITH MINORS）**——三个交付核心正确（@version 隔离/升级/冲突直证、ΣB=612 上界数值正确、last-wins 索引与 rev().find() 等值）+ 门禁主体绿（fmt/clippy/workspace 0）+ 产品验证线全线收官声明无夸大。3 项 Minor（命名空间缺口 / doc 声明 / 升级粒度）建议按最小修复清单落地，不阻塞出口。

**最小修复清单**（给委派方）：
1. `M-1`：EXIT/§8bis 补“key namespacing 留后续（collision 现由 first-wins 报告兜底）”注记，或实现命名空间。
2. `M-2`：`examples/agent-plugin/src/main.rs:36` 反引号化 `<Context>`（doc 归 0）。
3. `M-3`：可选——补“单消费者版本迁移”断言或记录覆盖粒度。
