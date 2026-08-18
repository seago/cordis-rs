//! M1.5（草案 v0.3.1 §3.2/§4.1/§4.2）：验收 #9（Send+Sync 断言）+ async
//! 监听器投递 + loader 集成。

use cordis_core::{Context, Runtime};
use cordis_events::{Event, EventBus, EventsKey, EventsProvider};
use cordis_loader::{Entry, Loader};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

/// tick 事件（emit 用例）。
struct Tick;
impl Event for Tick {
    type Payload = u32;
    const SYMBOL: &'static str = "tick";
}

/// #9（评审 M-1/M-1'）：`EventBus` 与 `Arc<EventBus>` 满足 `Send + Sync`
/// ——编译期静态断言，防止将来把非 Send/Sync 存储塞回总线。
#[test]
fn send_sync_compile_assert() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EventBus>();
    assert_send_sync::<Arc<EventBus>>();
    // 监听器闭包类型同样 Send+Sync（可进 store 值；§0 核心义务）。
    assert_send_sync::<cordis_events::EmitListener<u32>>();
}

/// async 监听器投递（草案 §4.1）：sync 闭包内 `spawn_local` 投递、不阻塞
/// 派发（C-5 可追溯）。
#[tokio::test]
async fn async_listener_delivery_via_spawn_local() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let bus = Arc::new(EventBus::new());
            let log: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
            let _d = bus.on::<Tick>({
                let log = Arc::clone(&log);
                move |p: &u32| {
                    // 同步派发 + 投递到 LocalSet（async 监听器任务可追溯）。
                    log.write().unwrap().push(format!("sync:{p}"));
                    let pv = *p; // 拷贝为 owned，供 'static async 任务。
                    let log = Arc::clone(&log);
                    tokio::task::spawn_local(async move {
                        log.write().unwrap().push(format!("async:{pv}"));
                    });
                }
            });

            bus.emit::<Tick>(&7);
            assert!(
                log.read().unwrap().iter().any(|l| l == "sync:7"),
                "同步段立即执行（不阻塞派发）"
            );
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            assert!(
                log.read().unwrap().iter().any(|l| l == "async:7"),
                "async 监听器经 spawn_local 投递并完成"
            );
        })
        .await;
}

/// loader 集成（草案 §4.2）：`EventsProvider` 作为根条目经 loader 挂载，
/// 总线绑定可达、订阅/emit 回路跑通。
#[test]
fn events_provider_mounts_via_loader() {
    let runtime = Rc::new(Runtime::new());
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("events", Rc::new(EventsProvider));
    loader.apply(&[Entry::new("events", "events", Rc::new(()), 0, false)]);

    // 总线绑定在 loader 根 ctx（EventsProvider 提供）。ctx 经 runtime 取。
    let ctx: Rc<Context> = runtime.context();
    let bus = Arc::clone(&*ctx.get::<EventsKey>().expect("loader 挂载后总线可读"));

    let log: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
    let _d = bus.on::<Tick>({
        let log = Arc::clone(&log);
        move |p: &u32| log.write().unwrap().push(format!("loader:{p}"))
    });
    bus.emit::<Tick>(&3);
    assert!(
        log.read().unwrap().iter().any(|l| l == "loader:3"),
        "loader 集成：EventsProvider 绑定总线、订阅回路跑通"
    );
    // 收尾：loader teardown（零配置污染由 loader 语义保证）。
    loader.apply(&[]);
}
