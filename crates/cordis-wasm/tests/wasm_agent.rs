//! P-5 wasm agent 多轮 take-await（产品验证线 P-5）：guest 多轮 LLM 会话
//!（submit→Await→take→累积→下一轮）经 `poll_and_advance` 驱动；失败轮
//! 终止并落盘 probe_err。

use cordis_async::TokioRemote;
use cordis_core::component::Component;
use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_loader::{Entry, Loader};
use cordis_wasm::Value;
use cordis_wasm::WasmComponent;
use std::rc::Rc;
use std::sync::Arc;

fn guest_wasm_path() -> std::path::PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .join("../../examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm")
}

fn store_value(runtime: &Rc<Runtime>, key: &str) -> Option<Value> {
    runtime
        .store()
        .get_value(Symbol::intern(key))
        .and_then(|v| v.downcast_ref::<Value>())
        .cloned()
}

fn setup(
    op: Arc<cordis_wasm::RemoteOp>,
) -> (Rc<Runtime>, Rc<WasmComponent>, tokio::runtime::Runtime) {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("guest（P-5 多轮）");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    comp.register_remote("llm", op);
    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::clone(&comp) as Rc<dyn Component>);
    let report = loader.apply(&[Entry::new("p", "db", Rc::new(()), 0, false)]);
    assert!(report.ok());
    (runtime, comp, worker)
}

/// 主链路：3 轮 LLM 会话（r0|r1|r2 累积）经 poll_and_advance 驱动完成。
#[test]
fn wasm_agent_multi_round_take_await() {
    let op: Arc<cordis_wasm::RemoteOp> = Arc::new(|params: Vec<Value>| {
        let n = match params.first() {
            Some(Value::Count(c)) => *c,
            _ => 0,
        };
        Value::Text(format!("r{n}"))
    });
    let (runtime, comp, _worker) = setup(op);
    // 直接轮询驱动（用 store probe 就绪判断——不依赖 fiber id）。
    for _ in 0..4000 {
        comp.poll_and_advance(&runtime);
        if store_value(&runtime, "probe").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let probe = store_value(&runtime, "probe").expect("多轮累积落盘 probe");
    assert!(
        matches!(probe, Value::Text(ref t) if t == "r0|r1|r2"),
        "3 轮 LLM 会话累积：{probe:?}"
    );
    assert!(
        store_value(&runtime, "probe_err").is_none(),
        "成功路径无 err"
    );
}

/// 失败轮：第 2 轮 worker panic → take Err → probe_err 终止。
#[test]
fn wasm_agent_failure_round_terminates() {
    let op: Arc<cordis_wasm::RemoteOp> = Arc::new(|params: Vec<Value>| {
        let n = match params.first() {
            Some(Value::Count(c)) => *c,
            _ => 0,
        };
        if n == 1 {
            panic!("llm boom");
        }
        Value::Text(format!("r{n}"))
    });
    let (runtime, comp, _worker) = setup(op);
    for _ in 0..4000 {
        comp.poll_and_advance(&runtime);
        if store_value(&runtime, "probe_err").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let err = store_value(&runtime, "probe_err").expect("失败轮落盘 probe_err");
    assert!(
        matches!(err, Value::Text(ref t) if t.contains("boom")),
        "worker 失败经 take err 达 guest：{err:?}"
    );
    assert!(
        store_value(&runtime, "probe").is_none(),
        "失败终止不落 probe"
    );
}
