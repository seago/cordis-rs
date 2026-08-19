# cordis-async 线程拓扑与共存（P1.3 R3 收口文档）

依据：草案 v1.4 §4（线程拓扑）、契约 C-3（组合线程）、O-6（桥政策）、C-1（Arc 惯例）；S2/S3 spike 经验。

## 1. 进程拓扑（唯一组合线程）

```
组合线程（唯一）─────────────────────────────────────────────
  tokio current_thread runtime + LocalSet
    ├── sync core（Rc/RefCell，普通调用）
    ├── async 效应（AsyncRegistrar spawn_local，非 Send future 可持 Rc）
    ├── AsyncRuntime 注册表 / tail 队列 / settle
    └── 事件总线（cordis-events，store 内 `EventsKey` 服务）
                      ▲  channel / Arc 句柄（store 值 Send+Sync）
卫星运行时（可选，多线程 / blocking 池 / wasm guest）
    ├── TokioRemote worker（multi_thread：future 池 + spawn_blocking 池）
    └── WasmRemote（M1 接入点：guest 无自发线程，submit=入队 + 宿主驱动）
```

- **契约 C-3**：进程内 cordis 世界只有**一条组合线程**（宿主该线程的
  LocalSet）；所有生命周期操作（loader apply / retire / settle、事件订阅/
  派发）在该线程 local 上下文内进行，违反 = panic + 明确诊断。
- **Send 服务**（LLM client、DB pool、axum handle）以 `Arc` 存 store，
  跨线程经 channel / join 通信——与 TS dsh 单线程事件循环同构。

## 2. sync 树 与 async 组件的共存（同 loader 树）

- sync-only 组件与 async 组件（`AsyncRuntime::wrap_component` → loader
  `register_component` + `apply`）挂**同一 loader 树**，经 realm 键控共享
  同一 store（core `Runtime`）。
- 依赖解析 / 级联 / 退役全在 core 完成；async 组件的注册器逆把尾巴入
  `AsyncRuntime` 收账队列——**依赖者 async 逆先 settle**（I-3）由 sync
  级联免费获得。
- `EventsProvider` 作为根条目进 loader 树（P1.1），事件订阅经
  `ctx.effect`（P1.1）随 fiber 卸载自动退订。

## 3. AsyncCx 视图边界

| 操作 | 时机 | 语义 |
|---|---|---|
| `get_cloned` | Running 期 | 读**活 store** 快照（立即克隆释放借用）；teardown 窗口改用步创建处的 Arc 捕获（C-1'） |
| `set` | 任意 | sync 绑定（逆在 core 卸载同步执行）；async 资源不放绑定里（C-2） |
| `spawn_remote` | 任意 | 请求交给 worker / 远端，返回可 await join（组合线程 await 回灌） |
| `cancellation` | — | 卸载/目标变更触发的取消标志（drive 步界退场） |

## 4. Remote 两形态（P1.3 R1/R2）

| 形态 | 载荷 | worker 侧执行 | 适合 |
|---|---|---|---|
| 闭包 | `RemoteRequest::boxed` / `From<FnOnce>` | `spawn_blocking`（blocking 池） | 阻塞、CPU 密集 |
| Send-future | `RemoteRequest::from_future` | `handle.spawn`（multi_thread 池） | 非阻塞异步（IO / 拉取式流） |

- 均遵守 **O-6**：worker 侧不触碰组合线程资源（core/LocalSet），否则死锁；
  远端 panic = 宿主 bug 诊断。
- WasmRemote（M1 接入点）：guest 无自发线程，submit = 入队 + 宿主 step
  边界驱动并回填，语义不变。

## 5. O-6 sync→async 桥政策

- v1 **禁止** sync 代码 await async 结果——sync 侧提交请求 + oneshot/事件，
  由 async 世界消费回灌（无死锁）。
- 确需阻塞等待时提供 `spawn_bridge` 逃生口（独立线程 runtime + 阻塞等待）；
  **桥任务不得触碰组合线程资源**——仅限纯外部 IO。

## 6. store 值纪律（C-1）

凡跨 async 段使用的服务值，`K::Value` 实现 `Clone`（Arc 惯例）；`Send+Sync`
是 core `Key::Value` 强制——事件监听器闭包与 Remote 载荷均带 `Send+Sync`
上界（P1.1 §0 核心义务 / P1.3 R1）。
