# cordis-rs 需求与状态（框架侧）

> 本文 = cordis-rs 仓库**自己的**需求清单与状态账本。产品层需求见 dsh-rs-product-requirements.md（位于 dsh 工作区 docs/，本仓库不含该文件）；跨层决策历史见 cordis-rs-dsh-feasibility.md（位于 dsh 工作区 docs/，本仓库不含该文件）。
>
> 基线：commit `bdb8905`（2026-08-22 审计，backlog 清理线③）。

---

## 一、定位与边界

**框架做什么**：零依赖组合内核 + async 层 + 事件系统 + 声明式加载与错误报告 + wasm 插件边界。任何「生态选择」都不进框架。

**框架不做什么**（边界清单，均移产品层）：

| 项 | 归属 | 理由 |
|---|---|---|
| 配置格式（serde/YAML/TOML） | 产品层 app 配置管道 | 格式是使用方决策；TS 先例（YAML 在 include 插件而非框架） |
| 插件分发格式/打包/签名/发现 | 产品层插件生态 | 生态层决策；TS 先例（npm 是生态）；框架已有加载入口 |
| wit 能力面按能力膨胀（网络/文件 host import） | 产品层以 remote 操作注册 | B 计划：`register_remote` 注册远端操作，wit 世界不膨胀 |
| 任何 UI/前端 | 产品层（TS 前端经 sdk 对接） | Rust UI 无生态，不属框架 |

## 二、能力清单与状态（2026-08-22 审计）

| 能力 | 状态 | 证据 |
|---|---|---|
| 组合内核（Key/realm 隔离/可逆效应/级联卸载） | ✅ 完成 | core 全模块 + oracle property 测试全绿（`engine_matches_oracle`、`thm73`…，core lib 61/61） |
| 声明式加载 + 错误策略 | ✅ 完成 | `EntryError`/`ApplyReport`/first-wins/组校验失败/每次 apply 重试——测试直证（loader 60/60）；**版本化链接**（`key@version` 隔离/升级/冲突，§6.6）随产品验证线 P-6 落地 |
| HMR 数据算法 | ✅ 完成 | cordis-hmr 三算法 + 事务回滚（10/10） |
| async 层 | ✅ 完成 | Phase 0（11 测试 + 3 spike）+ B 计划 A1–A4（`Step::Await`/`advance`/`resumable`）+ P-3 挂起集生产化（`suspended_fibers`/`advance_suspended`/`poll_and_advance`）+ backlog ① 单一事实来源（`is_suspended` 生产化消费；core 内部重构、公开语义不变，无需 THEORY-MAP 授权行——THEORY-MAP P-3 行已注记跟进） |
| 事件系统 | ✅ 完成 | cordis-events 按 v0.3.1 全量实现（四派发/Send+Sync 上界/跨模式载荷检查），验收 1–9 覆盖 + error_bridge/m15 |
| wasm 插件边界 | ✅ 完成 | `context` + `remote`（submit/take）+ Await 挂起/恢复 + err 通道 + go ABI 同步自动化（P-4，`build.sh` 第 0 步 wit-bindgen 重生成）；a2_e2e 直证（3/3 绿，go_guest 已恢复无 ignore） |
| 生命周期/错误通道配套 | ✅ 完成 | `Fiber::target_view()`（O-1）+ loader hook 弱引用修环 + P-7 错误策略 O-1/O-4（越界写升级 `ComponentFailure`；HMR 失败双通道：report 最新 apply 态 + `loader/entry-failed` 事件） |

## 三、框架侧剩余工作

| 项 | 类型 | 状态 |
|---|---|---|
| 冻结协议中的开放问题（O-items） | 待场景 | 见各协议文档 §7/§10——**有真实消费者才落**，不预先实现 |
| 判据 v2（Await 挂起时自报判据 + 显式 `poll_ready`） | 进行中 | backlog 清理线②（本线后置项，完成即更新本账本） |
| `docs/cordis-rs-requirements.md` 基线同步 | 随线维护 | 本账本随里程碑/清理线审计更新 |

**既有遗留清零**：A2b（go ABI）已闭环（`6a714ca` + P-4 `951709d` 自动化，go_guest 恢复绿）；产品验证线 P-1..P-7 全线收官（各线 EXIT 独立走查 PASS）；backlog ①（`is_suspended` 生产化）已闭环（本账本同步审计）。

框架侧**无其他已知待办**：审计结论「符合 dsh 类应用基础框架条件」成立。

## 四、框架纪律（不变的约束）

1. **零依赖**：core/loader/events 不加第三方依赖；async 仅 tokio；
2. **panic 边界**（错误策略 v0.2 冻结 + P-7 扩展）：panic 保留 ⟺ 用户输入不可达；用户输入可达错误走 ComponentFailure / OrchestrationError 通道（O-1 越界写、O-4 HMR 失败双通道为 P-7 定案扩展，见 `cordis-ERRORS-QUIET.md` §3bis）；
3. **无消费者不设计**：O-items、新能力面、任何格式/协议，有真实消费者才动；
4. **审查流程**：每个里程碑 = 独立审查报告入库 + nit 落地 commit（现有 A1–A4/C 探针/B 计划/P 线先例）；core 语义变更需 THEORY-MAP 授权偏离标注；
5. **冻结协议清单**：`cordis-async-protocol-draft.md` v1.4、`cordis-events-protocol-draft.md` v0.3.1、`cordis-rs-error-strategy-draft.md` v0.2——实现即按此、除评审纠错外不修订。

## 五、面向产品层的接口面（app 依赖什么）

产品层只依赖以下公开面（无更深耦合）：

- `cordis-core`：`Key`/`Context`/`Component`/`Runtime`/`Fiber`（组合与生命周期；挂起面：`Fiber::is_suspended`/`Runtime::suspended_fibers`/`advance_suspended`）；
- `cordis-loader`：`Entry`/`Patch`/`Loader::apply -> ApplyReport`（声明式组合与错误报告；版本化键 `key@version` 面）；
- `cordis-async`：`AsyncRuntime`/`AsyncBehavior`/`AsyncCx`/`Remote`（async 效应与桥）；
- `cordis-events`：`Event`/`EventBus`/`subscribe*`（类型化事件）；
- `cordis-wasm`：`WasmComponent::load`/`configure_remote`/`register_remote`/`poll_and_advance`（插件边界 + Await 驱动）。

## 六、评审记录（backlog 清理线③，2026-08-22）

- 逐条核对上述断言 vs 当前 HEAD `bdb8905`：基线由 `8752d0e` 更新；A2b 由"既定遗留"更正为**已闭环**（go_guest 无 ignore、`build.sh` 第 0 步自动化）；wasm 插件边界由"主体完成"更正为**完成**；补 P-3 挂起集生产化、P-6 版本化链接、P-7 O-1/O-4、backlog ① 记录；错误策略冻结条款补 P-7 扩展注记。
- 未发现账本级错误之外的偏差；其余断言（边界清单/冻结协议/公开面）核对属实。
