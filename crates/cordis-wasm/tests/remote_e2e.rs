//! M1 wasm 桥 W2 端到端：guest 经 wit `remote` 提交 → 宿主 pump 提交到注入
//! TokioRemote worker → worker 执行 → 宿主 `remote_result` 轮询真实回填。
//!
//! **时序边界（W2 记录）**：核心 `execute` 为同步一口气循环（无步间暂停），
//! guest 的 `handle.take()` 无法在单次激活内等到异步 worker 完成——take
//! 回填的时序需两次驱动（M2 async 驱动/核心异步化解锁）。本测试以宿主
//! `remote_result` 断言"提交→worker→回填"真实链路；guest 的 take 契约为
//! 接口面（编译/语义）。

use cordis_async::TokioRemote;
use cordis_core::component::Component;
use cordis_core::runtime::Runtime;
use cordis_wasm::Value;
use cordis_wasm::WasmComponent;
use std::rc::Rc;
use std::sync::Arc;

fn guest_wasm_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .join("../../examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm")
}

#[test]
fn guest_remote_submits_to_worker_and_backfills() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path())
        .expect("先构建 guest：cargo build -p wasm-plugin-rust --target wasm32-wasip2");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");

    // 注入远端桥 + 注册操作（llm 返回 worker 线程 id → O-6 隔离断言）。
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    // P-5 多轮 guest：远端操作名 llm（round0 提交 rep0）。
    comp.register_remote(
        "llm",
        Arc::new(|_params: Vec<Value>| Value::Text(format!("{:?}", std::thread::current().id()))),
    );

    // 激活（RemoteProbe 组件）：execute 跑完 step0（submit echo → pump 提交）
    // 与后续 take 轮询（execute 内取不到异步结果，未就绪跳过）。
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let _fiber = root
        .use_component(Rc::clone(&comp) as Rc<dyn Component>, Rc::new(()))
        .expect("激活 RemoteProbe");

    // 宿主侧轮询 remote_result(0)：提交 → worker 完成 → 回填（真实链路）。
    let mut backfilled = None;
    for _ in 0..4000 {
        comp.poll_remotes();
        if let Some(Some(Ok(Value::Text(tid)))) = comp.remote_result(0) {
            backfilled = Some(tid);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let tid = backfilled.expect("回填就绪（真实 worker 回灌）");
    let combo_tid = format!("{:?}", std::thread::current().id());
    assert_ne!(
        tid, combo_tid,
        "O-6：远端在 worker 池线程执行（非组合线程）：{tid}"
    );

    // 退役 → 级联 → 静止（W3 清理语义：worker 完成即弃、无残留）。
    let _ = &comp;
    drop(_fiber);
    assert!(runtime.is_quiet(), "退役后静止（远端桥不残留任务）");
    let _ = worker;
}
