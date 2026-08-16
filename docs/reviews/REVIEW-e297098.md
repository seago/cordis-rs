# M0 原生闭环——整体代码审查报告（commit `e297098`）

- **审查对象**：`e297098`（M0 走查补记；PR #1–9 + 走查修正的全部累积状态——里程碑级整体审查）
- **审查日期**：2026-08-16（仓库时区）
- **审查方式**：以 `e297098` 为锚点的里程碑整体审查——结构总览、全套门禁、文档-实现一致性抽查、架构与遗留债务评估
- **验证手段**：`cargo test --workspace` **66 测试全绿**（16 个测试块）；`cargo fmt --all -- --check` 干净；`RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets -- -D warnings` 零警告；`cargo run -p hello-plugin` 全部断言通过

---

## 一、门禁独立判定：M0 通过 ✅

作者 7c2f182 的走查结论"门禁判定：通过（含处置清单）"经独立复核**属实**：

| 门禁 | 状态 | 证据 |
|---|---|---|
| 定理测试全绿 | ✅ | 66 单元测试 + property 2000 用例 + Cor 21 穷举 24 排列 + Thm 73(1) canonical form 补测 |
| 走查无未解释偏差 | ✅ | 全部差异已分类记录（实现缺口 2 / 覆盖缺口 2 / 必改项 2 / 结构性差异若干），零"未解释" |
| 质量门（fmt/clippy/-D warnings/unsafe deny/missing_docs deny） | ✅ | 全部干净；全仓零 `unsafe`、零 TODO/FIXME 残留 |
| 端到端验收 | ✅ | hello-plugin（激活 → 级联 → 重连）纳入 CI |

**抽查核验**（对作者走查声明的真实性独立确认）：
1. "`get` 从不咨询 `ι`（interception 仅累积元数据）"——`Context::get` 函数体内 grep `intercept` = 0 ✅ 属实；
2. "L-Raise 整条缺失（`FiberError` 无生产者、outcome 恒 None）"——全仓无 `outcome: Some` 写入点 ✅ 属实；
3. "O-Insert 的 π 前提未在引擎侧执行"——`Runtime::register` 无 parent 参数，`RegistryError::UnknownParent` 仅 property harness 合成 ✅ 属实；
4. 定理覆盖表中 Thm 59/61 明确标注"覆盖缺口"、Cor 62 注明"值维度平凡化"、Thm 66 注明"定量上界未断言" ✅ 自审诚实。

## 二、架构评价

- **分层与依赖**：`cordis-core`（3456 行，零依赖）→ `loader`（565）/`native`（72）/`macro`（130，零依赖 proc-macro）→ `cordis` 门面（glob re-export）。依赖方向严格单向、无环；wasm/hmr 为文档化骨架（M1/M2 填充）。
- **公开 API 面**：core 69 个公开方法，职责边界清晰（context/runtime/fiber/store/keyset/symbol 六模块各司其职）；`interp` 刻意不 root 导出避免命名冲突——既有纪律保持良好。
- **测试体系四层**：单元（66）→ oracle 自检 → oracle×引擎 property（2000 用例 × ≤12 步随机编排）→ 端到端示例，覆盖金字塔结构合理。
- **统一策略**：panic = bug（模块文档声明）、`Box<dyn FnOnce>` 命令式 Disposer（Def 17–19 结构差异已声明）、单线程 `Rc`/`RefCell` 宿主（ADR-0002）、文档注释中文 + 论文符号映射一体。

## 三、遗留债务清单（M1 首批任务，与作者处置清单一致 + 独立补充）

作者处置清单（6 项，均已记录于 THEORY-MAP）：
1. interception 求值形态（Def 30/31 的 provider 函数，`get(k,μ)` 消费 ι）——**当前公开 API 存在但语义半成品**（见下方强调）
2. §5.1.4 Proxy 访问层（Alg 6，规格强制访问）
3. Thm 59/61 直接测试
4. Thm 66 定量上界 `(K+4)(V+1)` 断言
5. L-Raise 落地（含 `is_quiet` 补 ζ 析取）
6. 命令式 Disposer 结构（wasm 句柄化时对齐论文形态）

独立补充（历轮审查记录在案、确认未在 M0 内解决）：
7. **async 化三件套**：`set` 前置检查 TOCTOU（expect → 可传播错误，PR #4 审查 m3）、`relied` guard 显式化（Def 50）、无限迭代支持（PR #3 审查 M-B）
8. **M2 范围**：Entry 的 isolate/intercept 注解、嵌套 group/include、托管 realm（Algorithm 7）、realm 自动生成、loader 组件注销 API、reactor 移除 API（PR #5 m5）
9. **小项**：`Fiber::state()`/`Runtime::store()` 的 Ref 借用警告已文档化（用户教育层面）——API 层 `try_borrow` 化可选

## 四、最值得强调的一个点（非新问题，是风险提示）

**`intercept` 的公开 API 与语义半成品状态**：`Context::intercept`/`intercept_of` 已是公开 API（PR #4），用户可安装并读取拦截元数据；但 `get` 从不咨询 `ι`（拦截对读**无任何效果**）。若外部用户在 M1 之前基于"拦截已生效"的理解使用该 API，将得到静默错误行为。虽然 THEORY-MAP 已诚实记录"实现缺口"，但**公开 API 层面无任何警示**——建议在 `Context::intercept`/`intercept_of` 的 rustdoc 上补一句"当前仅累积元数据，读路径消费由 M1 落地"，或在 M1 前将该 API 标记 `#[doc(hidden)]`/feature gate。

## 五、整体评价

这是罕见的**"论文-文档-实现-测试四线闭环"**高质量项目：9 轮逐 PR 审查发现问题全部闭环（issue #1–#8 全关闭）；M0 走查由作者独立执行且诚实标注 2 个实现缺口 + 2 个覆盖缺口，与独立抽查完全一致；文档纪律（THEORY-MAP 偏差表 + 走查记录 + 里程碑处置清单）在 40+ 条记录中保持"每条差异可追溯、可处置"的格式。**M0 原生闭环：通过。**

**置信度**：高——门禁全部实测；走查声明抽查 4/4 属实；架构事实直接核验；唯一非代码性判断是"风险提示"（基于公开 API 语义完整性视角）。

---

*审查者备注：本轮为里程碑整体审查，无逐行 diff 引用；具体代码级发现均已在历轮 issue #1–#8 中闭环。*
