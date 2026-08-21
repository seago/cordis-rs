//! B 计划 A2 端到端 + P-5 多轮升级：guest 经 multi-step take 完整 await——
//! 多轮 LLM 会话（submit→Await→take→累积→下一轮）经 `poll_and_advance`
//! 驱动；O-6 隔离（回复携带 worker tid）；错误轮终止（probe_err）。

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

fn drive_until(
    runtime: &Rc<Runtime>,
    comp: &Rc<WasmComponent>,
    done: impl Fn(&Rc<Runtime>) -> bool,
) {
    for _ in 0..4000 {
        comp.poll_and_advance(runtime);
        if done(runtime) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("多轮会话 4000 拍未收敛");
}

/// 主链路：3 轮 LLM 会话（回复带 worker tid → O-6 隔离断言）。
#[test]
fn guest_awaits_remote_join_and_continues() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("guest（P-5 多轮）");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    comp.register_remote(
        "llm",
        Arc::new(|params: Vec<Value>| {
            let n = match params.first() {
                Some(Value::Count(c)) => *c,
                _ => 0,
            };
            // O-6：回复携带 worker tid。
            Value::Text(format!("{:?}:r{n}", std::thread::current().id()))
        }),
    );

    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::clone(&comp) as Rc<dyn Component>);
    let report = loader.apply(&[Entry::new("p", "db", Rc::new(()), 0, false)]);
    assert!(report.ok());

    let fid = loader.fiber("p").expect("条目 p").id();
    let fiber = runtime.fiber(fid).expect("fiber");
    assert!(fiber.is_suspended(), "Await 挂起（guest 在等远端回填）");
    assert!(
        runtime.store().contains(Symbol::intern("db")),
        "step0 副作用已发生（db 绑定）"
    );

    drive_until(&runtime, &comp, |r| store_value(r, "probe").is_some());
    assert!(
        !runtime.fiber(fid).expect("fiber").is_suspended(),
        "会话完成"
    );

    let probe = store_value(&runtime, "probe").expect("多轮累积落盘");
    let probe_t = match probe {
        Value::Text(t) => t,
        other => panic!("probe 应为 Text：{other:?}"),
    };
    // r0|r1|r2 各带同一 worker tid（O-6：非组合线程）。
    let combo_tid = format!("{:?}", std::thread::current().id());
    assert!(
        !probe_t.starts_with(&format!("{combo_tid}:r0")) && probe_t.ends_with(":r2"),
        "O-6 隔离（非组合线程）+ 3 轮累积（{probe_t}）"
    );
    // 精确：probe 各段 tid 均 ≠ 组合线程。
    for seg in probe_t.split('|') {
        let tid = seg.split(':').next().expect("tid 段");
        assert_ne!(tid, combo_tid, "O-6：远端在 worker 池线程执行");
    }

    runtime.fiber(fid).expect("fiber").retire();
    assert!(runtime.is_quiet(), "退役后静止");
    let _ = worker;
}

/// 错误通道：第 0 轮 worker panic → take Err → probe_err 终止。
#[test]
fn guest_take_receives_remote_err() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("guest");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    comp.register_remote("llm", Arc::new(|_p: Vec<Value>| panic!("llm boom")));

    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::clone(&comp) as Rc<dyn Component>);
    let report = loader.apply(&[Entry::new("p", "db", Rc::new(()), 0, false)]);
    assert!(report.ok());

    drive_until(&runtime, &comp, |r| store_value(r, "probe_err").is_some());
    let err = store_value(&runtime, "probe_err").expect("失败轮落盘 probe_err");
    assert!(
        matches!(err, Value::Text(ref t) if t.contains("boom")),
        "op panic → err 经 take 达 guest：{err:?}"
    );
    assert!(
        store_value(&runtime, "probe").is_none(),
        "失败终止不落 probe"
    );
    let _ = worker;
}

/// 统一驱动回路：挂起 → `poll_and_advance` 循环（回填+恢复）→ 完成。
#[test]
fn poll_and_advance_drives_suspend_loop() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("guest（P-5 多轮）");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    comp.register_remote(
        "llm",
        Arc::new(|params: Vec<Value>| {
            let n = match params.first() {
                Some(Value::Count(c)) => *c,
                _ => 0,
            };
            Value::Text(format!("r{n}"))
        }),
    );

    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::clone(&comp) as Rc<dyn Component>);
    let report = loader.apply(&[Entry::new("p", "db", Rc::new(()), 0, false)]);
    assert!(report.ok());

    let fid = loader.fiber("p").expect("条目 p").id();
    assert!(runtime.fiber(fid).expect("fiber").is_suspended(), "初挂起");

    // 统一驱动回路：poll 回填 → advance 恢复（循环直至 guest 完成）。
    drive_until(&runtime, &comp, |r| store_value(r, "probe").is_some());
    assert!(
        !runtime.fiber(fid).expect("fiber").is_suspended(),
        "回路驱动完成"
    );
    let probe = store_value(&runtime, "probe").expect("guest 自取结果落盘");
    assert!(
        matches!(probe, Value::Text(ref t) if t == "r0|r1|r2"),
        "回填值：{probe:?}"
    );

    let _ = worker;
}
