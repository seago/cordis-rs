//! P-5 全栈串联（产品验证线 P-5）：events + async + wasm + Remote + Await
//! 四层共场协作回路——
//!  `user/msg` 事件 →（Arc 队列，listener Send+Sync 合规）→（async LLM
//!  组件，长驻）spawn_remote LLM（worker）→ 发布 `bot/reply` 事件 →
//!  （订阅记录）→（main 桥接）注入 `wasm/in` →（wasm agent 多轮，经注入
//!  键读输入）远端 → 落盘 `probe`。协作序断言：原生回复 → wasm 输入 →
//!  wasm 多轮完成。

use cordis_async::{
    AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncRuntime, AsyncStep,
    LocalBoxFuture, RemoteRequest, TokioRemote,
};
use cordis_core::{Component, Context, KeySet, Runtime};
use cordis_events::{Event, EventBus, EventsKey, EventsProvider, subscribe};
use cordis_loader::{Entry, Loader};
use cordis_wasm::Value as WValue;
use cordis_wasm::WasmComponent;
use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

struct UserMsg;
impl Event for UserMsg {
    type Payload = String;
    const SYMBOL: &'static str = "user/msg";
}
struct BotReply;
impl Event for BotReply {
    type Payload = String;
    const SYMBOL: &'static str = "bot/reply";
}

type Log = Arc<RwLock<Vec<String>>>;
type Queue = Arc<RwLock<std::collections::VecDeque<String>>>;

/// async LLM 组件：读输入队列 → spawn_remote LLM（worker）→ 事件输出。
struct LlmBehavior {
    log: Log,
    input: Queue,
    bus: Arc<EventBus>,
}
impl AsyncBehavior for LlmBehavior {
    fn apply_async(&self, cx: AsyncCx, _c: &dyn Any) -> Box<dyn AsyncEffectIter> {
        Box::new(LlmLoop {
            cx,
            log: Arc::clone(&self.log),
            input: Arc::clone(&self.input),
            bus: Arc::clone(&self.bus),
            done: false,
        })
    }
}
struct LlmLoop {
    cx: AsyncCx,
    log: Log,
    input: Queue,
    bus: Arc<EventBus>,
    done: bool,
}
impl AsyncEffectIter for LlmLoop {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done);
        self.done = true;
        let cx = self.cx.clone();
        let log = Arc::clone(&self.log);
        let input = Arc::clone(&self.input);
        let bus = Arc::clone(&self.bus);
        Box::pin(async move {
            loop {
                if cx.cancellation().cancelled() {
                    log.write().unwrap().push("llm:exit@cancel".into());
                    break;
                }
                let msg = input.write().unwrap().pop_front();
                if let Some(msg) = msg {
                    let join =
                        cx.spawn_remote(RemoteRequest::boxed(move || format!("reply:{msg}")));
                    let reply = *join.await.downcast::<String>().expect("LLM 回复");
                    log.write().unwrap().push(format!("llm:{reply}"));
                    bus.emit::<BotReply>(&reply);
                }
                tokio::task::yield_now().await;
            }
            AsyncStep::Finished(empty())
        })
    }
}
fn empty() -> AsyncDisposer {
    Box::new(|| Box::pin(async {}) as LocalBoxFuture<()>)
}

/// 空壳组件（async 组件注册用；协作输入经 preseed 镜像通道）。
struct Dummy;
impl Component for Dummy {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _c: Rc<Context>, _cfg: &dyn Any) -> Box<dyn cordis_core::EffectIter> {
        Box::new(cordis_core::once(Box::new(|| {
            Box::new(|| {}) as cordis_core::Disposer
        })))
    }
}

#[test]
fn full_stack_events_async_wasm_remote_await() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    let combo = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("组合 runtime");

    combo
        .block_on(async {
            tokio::task::LocalSet::new()
                .run_until(async {
                    let core = Rc::new(Runtime::new());
                    let ctx: Rc<Context> = core.context();
                    let rt = AsyncRuntime::new(&ctx);
                    rt.set_remote(Rc::new(TokioRemote::new(worker.handle().clone())));

                    // events 总线（error_bridge 同款直接挂载）。
                    ctx.use_component(Rc::new(EventsProvider), Rc::new(()))
                        .expect("events 挂载");
                    let bus = Arc::clone(&*ctx.get::<EventsKey>().expect("总线可读"));

                    // wasm agent（多轮；注入 wasm/in）。
                    let engine = wasmtime::Engine::default();
                    let bytes = std::fs::read(
                        concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/../../examples/wasm-plugin-rust/target/wasm32-wasip2/debug/wasm_plugin_rust.wasm"
                        ),
                    )
                    .expect("guest（P-5 多轮）");
                    let wcomp = WasmComponent::load(&engine, &bytes).expect("组件加载");
                    wcomp.configure_remote(Some(Rc::new(TokioRemote::new(worker.handle().clone()))));
                    wcomp.register_remote(
                        "llm",
                        Arc::new(|params: Vec<WValue>| {
                            let p = params.first().cloned().unwrap_or(WValue::Count(0));
                            WValue::Text(format!("w:{p:?}"))
                        }),
                    );
                    let loader = Rc::new(Loader::new(Rc::clone(&core)));
                    loader.register_component("wasm-db", Rc::clone(&wcomp) as Rc<dyn Component>);

                    let log: Log = Arc::new(RwLock::new(Vec::new()));
                    let inbox: Queue = Arc::new(RwLock::new(std::collections::VecDeque::new()));
                    let botq: Queue = Arc::new(RwLock::new(std::collections::VecDeque::new()));
                    // 订阅：user/msg → 输入队列；bot/reply → 记录 + 队列。
                    let _s1 = subscribe::<UserMsg>(&ctx, {
                        let inbox = Arc::clone(&inbox);
                        move |p: &String| {
                            inbox.write().unwrap().push_back(p.clone());
                        }
                    })
                    .expect("订阅 user/msg");
                    let _s2 = subscribe::<BotReply>(&ctx, {
                        let log = Arc::clone(&log);
                        let botq = Arc::clone(&botq);
                        move |p: &String| {
                            log.write().unwrap().push(format!("bot:{p}"));
                            botq.write().unwrap().push_back(p.clone());
                        }
                    })
                    .expect("订阅 bot/reply");

                    // async LLM 组件。
                    let h1 = rt
                        .use_component(
                            &ctx,
                            Rc::new(Dummy),
                            LlmBehavior {
                                log: Arc::clone(&log),
                                input: Arc::clone(&inbox),
                                bus: Arc::clone(&bus),
                            },
                            Rc::new(()) as Rc<dyn Any>,
                        )
                        .expect("挂载 llm 组件");

                    // 触发：user/msg。
                    bus.emit::<UserMsg>(&"你好".to_string());
                    let mut injected = false;
                    // 驱动预算（REVIEW-CI-FIXES 同族加固）：wasm_agent 同款
                    // 4000——判据 v2 精确驱动（实际 ~6 循环完成多轮），冗余
                    // 预算留 CI 负载余量。
                    for _ in 0..4096 {
                        tokio::task::yield_now().await;
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        // 原生回复 → preseed 镜像注入 wasm/in → 延迟挂载 wasm agent
                        //（协作序：输入就绪后激活，guest 首轮即用输入参数）。
                        if !injected
                            && let Some(reply) = botq.write().unwrap().pop_front()
                        {
                            // preseed 镜像注入协作输入 → 延迟挂载 wasm（guest
                            // 无注入依赖；镜像非注入键不被 sync 清理）。
                            wcomp.preseed_mirror("wasm/in", WValue::Text(reply));
                            let r = loader.apply(&[Entry::new("wp", "wasm-db", Rc::new(()), 0, false)]);
                            assert!(r.ok(), "wasm 挂载成功");
                            injected = true;
                        }
                        // 驱动 wasm 多轮（Await 回路）。
                        if injected {
                            wcomp.poll_and_advance(ctx.runtime());
                        }
                        // wasm 多轮完成？
                        if let Some(WValue::Text(t)) = ctx
                            .runtime()
                            .store()
                            .get_value(cordis_core::symbol::Symbol::intern("probe"))
                            .and_then(|v| v.downcast_ref::<WValue>())
                        {
                            log.write().unwrap().push(format!("wasm-probe:{t}"));
                            break;
                        }
                    }
                    {
                        let l = log.read().unwrap();
                        assert!(
                            l.iter().any(|x| x.starts_with("llm:reply:你好")),
                            "原生 async+Remote 回路（log={l:?}）"
                        );
                        assert!(
                            l.iter().any(|x| x.starts_with("bot:reply:你好")),
                            "事件回路（log={l:?}）"
                        );
                        assert!(
                            l.iter().any(|x| x.starts_with("wasm-probe:")),
                            "wasm 多轮完成（四层协作）：{l:?}"
                        );
                    }
                    assert!(injected, "协作注入 wasm/in 发生");

                    rt.retire(&h1);
                    rt.settle().await;
                    assert!(rt.is_quiet(), "收账后静止");
                    let _ = worker;
                })
                .await;
        });
}
