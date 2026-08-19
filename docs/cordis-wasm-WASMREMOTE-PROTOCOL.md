# WasmRemote 宿主驱动协议细化（M1 专项 W0）

依据 `docs/cordis-wasm-WASMREMOTE-PLAN.md` §1 的 W-D1..D4（默认采纳）与既有桥基础设施（`crates/cordis-wasm` pending/step 机制）。本文为实现前协议细化——不含实现代码。

## 1. 职责与接线（W-D1）

- **guest 不实现 `Remote` trait、不跑 cordis-async**（同步 step 模型）；`cordis-async` 的 `WasmRemote` 占位保持为**桥接线注记**（W3 更新 doc：接入点即 wit `remote` import + 宿主注入的 `Remote`）。
- 宿主侧 `InstanceState` 增 `remote: Option<Rc<dyn Remote>>`（注入复用 [`cordis_async::Remote`]——v1 即 `TokioRemote`）；`WasmComponent`/桥提供 `register_host_remote(Rc<dyn Remote>)`。
- O-6：guest 提交不触碰组合线程资源（worker 执行由注入 Remote 完成）；宿主在 **step 边界**驱动、不阻塞。

## 2. wit `remote` 接口（W1 落地对象）

```wit
interface remote {
    use context.{value};
    resource handle {
        /// 取结果（轮询）：未就绪 → none；就绪 → ok(value) / err(string)。
        /// guest 在后续 step 调用；host 保证不 panic。
        take: func() -> option<result<value, string>>;
    }
    /// 提交宿主已注册的远端操作（W-D2）：操作名 + 参数值 → 句柄。
    submit: func(name: string, params: list<value>) -> handle;
}
// world cordis 增 `import remote;`
```

- **handle 语义（W-D3）**：`submit` 返回资源句柄——宿主在组合线程侧（step 边界）经注入 `Remote` 对 `RemoteRequest` 封装提交（v1：`RemoteRequest::boxed(|params| 宿主注册操作执行)`）；结果按句柄登记。
- **注册表（W-D2）**：宿主侧 `register_remote(name, impl Fn(Vec<Value>) -> RemoteValue + Send + Sync + 'static)`；guest 只持名称 + 参数。未知名/错参 → 句柄 `err(...)`（不 panic 宿主）。

## 3. 回填与轮询（W-D3）

- 组合线程**不阻塞地** await 注入 `Remote` 的 join（tokio `JoinHandle::is_finished` 轮询 / 或由注入桥自行回填）——宿主在 **step 边界 / take 调用时**检查句柄就绪。
- guest `take()` 轮询语义 = `join.await` 的步进等价物（「等」 = 后续 step 轮询）。

## 4. 值传递（W-D4）

- 载荷/参数/结果走 wit `value`（`bool/u64/i64/string/blob`）。
- `value ↔ RemoteValue` 适配器在宿主侧：可序列化子集（对标量/文本/blob）；不可序列化 `RemoteValue` → `err("结果不可序列化")`。
- **已知边界**：跨边界 `box<dyn Any>` 泛型值不下沉（core 零改动；`Value` 不下沉 core 的边界保持，THEORY-MAP 既有行）。

## 5. 沙箱与错误通道

- guest 恶意输入（未知操作名、参数值类型不匹配、错误 handle）→ 句柄 `err` 或 `none`；**宿主不 panic**（沿 PR #14 沙箱纪律）。
- guest 崩溃/越界不伤宿主（既有沙箱回归保持）。

## 6. 清理（W3）

- guest 迭代结束/退役 → 未取句柄丢弃：宿主 worker 任务**完成即弃**（pending-set 泛化语义）；句柄表条目由宿主在 step 边界/卸载路径清扫（契约 C-5：无野任务——注入 Remote 的 join 被持有至完成）。
- 测试断言：卸载后 worker 完成不泄漏（句柄表空、无任务挂起）。

## 7. 验收（W1–W4 对应）

| 里程碑 | 验收点 |
|---|---|
| W1 | wit remote 编译 + 宿主单测（fake Remote 执行、回填时序、注册表映射、适配器标量往返） |
| W2 | 端到端：guest 提交 → 宿主 worker（TokioRemote，O-6 线程隔断言）→ 后续 step take 回灌值；错误通道（未知名 → err） |
| W3 | 退役清理（句柄表回收）+ 沙箱/双后端/go 回归 + 占位 doc 重定位 |
| W4 | 出口走查 + EXIT |
