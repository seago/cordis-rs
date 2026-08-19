//! B 计划 A2 端到端：guest 经 multi-step take 完整 await 远端结果——
//! step0 submit（done:false）→ 宿主 `Step::Await` 挂起（core `Runtime::advance`
//! 恢复）→ worker 回填后宿主 `poll_remotes` + `advance` → guest take 读到
//! 结果继续（probe 落盘）。错误通道：op panic → err 回填 → probe_err。

use cordis_async::TokioRemote;
use cordis_core::component::Component;
use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_loader::{Entry, Loader};
use cordis_wasm::WasmComponent;
use cordis_wasm::wit::cordis::core::context::Value;
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

/// 主链路：submit → Await 挂起 → 回填 → advance → guest 自取 take → probe。
#[test]
fn guest_awaits_remote_join_and_continues() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let engine = wasmtime::Engine::default();
    let bytes = std::fs::read(guest_wasm_path()).expect("先构建 guest（A2 多步 take）");
    let comp = WasmComponent::load(&engine, &bytes).expect("组件加载");
    comp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
    comp.register_remote(
        "echo",
        Arc::new(|_params: Vec<Value>| {
            // 返回 worker 线程 tid → 宿主断言 O-6 隔离（≠ 组合线程）。
            Value::Text(format!("{:?}", std::thread::current().id()))
        }),
    );

    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::clone(&comp) as Rc<dyn Component>);
    let report = loader.apply(&[Entry::new("p", "db", Rc::new(()), 0, false)]);
    assert!(report.ok(), "激活无 OrchestrationError");

    // 挂起断言：fiber Active + resumable 有值（guest 提交后等待）+ db 已绑定。
    let fid = loader.fiber("p").expect("条目 p 已挂载").id();
    let fiber = runtime.fiber(fid).expect("fiber");
    assert!(
        matches!(&*fiber.state(), cordis_core::FiberState::Active { .. }),
        "挂起于 Await 的 fiber 视作 Active"
    );
    assert!(
        fiber.is_suspended(),
        "Await 挂起：保留可恢复上下文（guest 在等远端回填）"
    );
    assert!(
        runtime.store().contains(Symbol::intern("db")),
        "step0 副作用已发生（db 绑定）"
    );

    // 宿主：轮询回填 → advance 恢复 → guest take 读到结果 → probe 落盘。
    for _ in 0..4000 {
        comp.poll_remotes();
        if let Some(Some(Ok(_))) = comp.remote_result(0) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    runtime.advance(fid);

    let probe = store_value(&runtime, "probe").expect("guest 自取结果落盘 probe");
    let probe_tid = match probe {
        Value::Text(t) => t,
        other => panic!("probe 应为 Text（worker tid）：{other:?}"),
    };
    let combo_tid = format!("{:?}", std::thread::current().id());
    assert_ne!(
        probe_tid, combo_tid,
        "O-6：guest 取到的远端结果在 worker 池线程执行（非组合线程）"
    );
    assert!(
        !runtime.fiber(fid).expect("fiber").is_suspended(),
        "恢复完成：挂起上下文已消费"
    );

    // 退役 → 静止（逆 LIFO 收账）。
    runtime.fiber(fid).expect("fiber").retire();
    assert!(runtime.is_quiet(), "退役后静止");

    let _ = worker;
}

/// 错误通道：op panic → err 回填 → guest take 走 Err → probe_err。
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
    comp.register_remote("echo", Arc::new(|_params: Vec<Value>| panic!("op boom")));

    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("db", Rc::clone(&comp) as Rc<dyn Component>);
    let report = loader.apply(&[Entry::new("p", "db", Rc::new(()), 0, false)]);
    assert!(report.ok());

    let fid = loader.fiber("p").expect("条目 p").id();
    for _ in 0..4000 {
        comp.poll_remotes();
        if comp.remote_result(0).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    runtime.advance(fid);

    let err = store_value(&runtime, "probe_err").expect("guest 取到 err → probe_err 落盘");
    assert!(
        matches!(err, Value::Text(ref t) if t.contains("boom")),
        "op panic → err 经 take 达 guest：{err:?}"
    );
    assert!(
        store_value(&runtime, "probe").is_none(),
        "失败路径不落 probe"
    );

    let _ = worker;
}
