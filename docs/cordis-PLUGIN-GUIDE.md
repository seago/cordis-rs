# cordis-rs 插件作者指南（P1.4 DX）

面向在 cordis-rs（Rust）上编写组件/插件的作者。覆盖 sync `cordis-core`、
async 层 `cordis-async`、事件层 `cordis-events` 与 loader 组合的既有语义
（均以冻结草案 + 已审查实现为准，本指南不引入新语义）。

---

## 1. 分层与定位

| 层 | crate | 角色 |
|---|---|---|
| 理论核心 | `cordis-core` | 可逆效应、纤维生命周期（realm 键控）、Thm 63 级联；单线程（ADR-0002），`Rc/RefCell` |
| async 层 | `cordis-async` | 一等 async 效应（AsyncEffectIter / drive）、两阶段卸载（settle/代次）、失败通道、Remote 桥；组合线程 LocalSet（契约 C-3） |
| 事件层 | `cordis-events` | 类型化事件 + 四种 sync 派发（emit/waterfall/serial/bail）+ 订阅即效应 |
| 编排 | `cordis-loader` | 条目树配置、依赖解析、retire/disabled 写回、HMR |

线程拓扑与共存见 `docs/cordis-async-THREADING.md`（P1.3 收口）。

## 2. 值纪律（C-1 / C-1'）

- **Arc 惯例**：凡会被 async 段跨 `await` 使用的服务值，`K::Value` 实现
  `Clone`，惯例用 `Arc<T>`（store 存 Arc，克隆零成本；core `Key::Value:
  Send + Sync` 是强制）。
- **快照纪律（C-1'）**：teardown 窗口（尾巴）需要的数据，在**创建该步时**
  捕获 Arc 克隆；尾巴**不得读运行时活 store**（提供者绑定可能已撤销）。
  async 段 Running 期读活 store 用 `AsyncCx::get_cloned`（立即克隆、释放
  借用）。

## 3. 绑定 vs 资源（C-2）

- `set` 只放**服务绑定**（sync、可逆、参与依赖解析）；其逆在 core 卸载时
  同步执行。
- **async 资源**（连接、任务、订阅）一律表现为 async 步 + 其逆
  （`AsyncDisposer`）。二者不混用，卸载语义才可预测。

## 4. 门面纪律（C-4）

- 生命周期变更（`use_component` / `retire` / `update`）**必须走
  `AsyncRuntime` 门面**（`AsyncFiberHandle` 弱引句柄）；绕过门面直接调
  core sync API 对 sync-only 组件允许，但 **async 尾巴不被 settle 记账**。
- `AsyncFiberHandle` 弱引：不延长 fiber 生命周期；`fiber()` 返回的强引
  仅限瞬时读状态（不长期持有）。
- 显式 `settle()`（P1.2 决策 O-2：框架不提供自动 settle 封装）。

## 5. 订阅即效应（cordis-events）

- 订阅经 `ctx.effect` 落账（`subscribe` / `subscribe_waterfall` /
  `subscribe_serial` / `subscribe_bail`）——**随 fiber 卸载自动退订**。
- 便捷函数返回的 disposer 与 ctx 累加器逆共享 armed（双路径撤销至多一次）。
- `ctx` 应为**订阅者所属 fiber 上下文**（传共享/根 ctx 则随其累加器，需
  手动 dispose）。
- 触发语义：emit 注册序；serial 收集全部返回值；bail 首个 `Some` 即停；
  waterfall around/短路/terminal；E-1 快照（派发中注册本轮不触发、退订
  者本轮跳过）；E-2 空集四断言。

## 6. 事件监听器约束（§0 核心义务）

- 事件总线监听器闭包须 **`Send + Sync + 'static`**（store 值纪律）——
  **不得捕获 `Rc`**；需要服务时经 `Arc` 捕获。线程私有总线（捕获 Rc）不属
  事件层（O-6'）。

## 7. Remote 两形态 + O-6

| 形态 | 构造 | worker 侧 | 适合 |
|---|---|---|---|
| 闭包 | `RemoteRequest::boxed` / `From<FnOnce>` | `spawn_blocking` | 阻塞 / CPU 密集 |
| Send-future | `RemoteRequest::from_future` | `handle.spawn`（池） | 非阻塞异步（IO/流） |

- **O-6**：worker 侧不得触碰组合线程资源（core/LocalSet），否则死锁；
  远端 panic = 宿主 bug 诊断。
- **O-6 桥政策**：v1 禁止 sync 代码 await async 结果（请求 + join/事件
  回灌；`spawn_bridge` 逃生口仅限纯外部 IO）。

## 8. 错误与安静语义（详见 `docs/cordis-ERRORS-QUIET.md`）

- async 失败：`AsyncStep::Failed(e)` → 静止终态 + 自退役（loader 写回
  disabled）+ 编排方重启用复活；settle 恒可完成。
- 组件失败 ≠ panic：panic = 宿主 bug（进入诊断，不级联）。
- 安静判定：`AsyncRuntime::is_quiet()`（无尾巴 ∧ 无 Active async 组件，
  Failed 视为静止）+ shutdown 双真（C-7）。

## 8bis. 版本化链接（§6.6 落地，P-6）

- **键内编码 `key@version`**（v1 精确匹配）：提供者声明 `db@1`、消费者声明 `db@1`——不同版本是**不同键**（版本隔离：`db@1` 与 `db@2` 共存不冲突）；
- **升级** = 消费者迁移到新版本键（依赖切换，旧版本提供者可卸载）；
- **冲突**：同版本键双提供 → `ProvisionClash` 报告（first-wins，错误策略通道）；
- **接口漂移防护**（论文 §6.6）：消费者声明版本键，提供者升级版本后旧声明不再满足（`Inactive`）——显式而非静默漂移；版本约束区间（`db@>=1`）留 v2。

## 9. 组合示例

- `cargo run -p cordis-async --example async_combo`（sync 树 + async 层 +
  Remote 回路）。
- 事件订阅 + agent-loop + 卸载 flush 模板：`cargo run -p cordis-async
  --example plugin_template`（REVIEW-dadc512 minor-1 引用修正）。
