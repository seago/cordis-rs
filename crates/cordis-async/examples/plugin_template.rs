//! P1.4 DX 插件模板：长驻 agent-loop async 组件 + 事件订阅联动 + 卸载 flush。
//!
//! 形态（复用 spikes S3）：一步 = 长驻循环（检查 `cancel` 检查点；mock 流
//! 逐 token 处理 + 工具调用识别）+ 逆 = **flush session**（await 收尾）。
//! 编排方订阅事件（`ctx.effect` 落账，示例演示 emit 联动），退役 agent →
//! cancel → 检查点退出 → flush 收账。
//!
//! 运行：`cargo run -p cordis-async --example plugin_template`

use cordis_async::{
    AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncRuntime, AsyncStep, LocalBoxFuture,
};
use cordis_core::{Context, Runtime};
use cordis_events::{Event, EventsKey, EventsProvider, subscribe};
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

/// 事件：外部请求（编排方 emit）。
struct Tick;
impl Event for Tick {
    type Payload = u32;
    const SYMBOL: &'static str = "req/tick";
}

/// agent-loop 行为：一步 = 循环（检查点 + mock 流处理）+ 逆 flush。
struct AgentLoopBehavior {
    log: Arc<RwLock<Vec<String>>>,
}

impl AsyncBehavior for AgentLoopBehavior {
    fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
        Box::new(AgentLoopIter {
            cx,
            log: Arc::clone(&self.log),
            done: false,
        })
    }
}

struct AgentLoopIter {
    cx: AsyncCx,
    log: Arc<RwLock<Vec<String>>>,
    done: bool,
}

impl AsyncEffectIter for AgentLoopIter {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done, "单步迭代器至多一步");
        self.done = true;
        let cx = self.cx.clone();
        let log = Arc::clone(&self.log);
        Box::pin(async move {
            // 长驻注册器循环（草案 §6：只在 cancel 检查点退出）。
            // mock SSE 流逐 token 处理（每 token 一拍模拟流式）。
            let tokens = ["user:你好", "tool:get_weather"];
            let mut i = 0usize;
            loop {
                // 检查点：卸载（cancel）时退场。
                if cx.cancellation().cancelled() {
                    log.write().unwrap().push("loop:exit@cancel".into());
                    break;
                }
                tokio::task::yield_now().await;
                log.write()
                    .unwrap()
                    .push(format!("token:{}", tokens[i % tokens.len()]));
                i += 1;
            }
            // 循环结束（仅取消）→ 逆 = flush session（await 收尾）。
            AsyncStep::Finished(flush_disposer(&log))
        })
    }
}

fn flush_disposer(log: &Arc<RwLock<Vec<String>>>) -> AsyncDisposer {
    let log = Arc::clone(log);
    Box::new(move || {
        let log = Arc::clone(&log);
        Box::pin(async move {
            // flush：await 收尾（保存 session / 杀 subprocess 的模拟）。
            tokio::task::yield_now().await;
            log.write().unwrap().push("flush:session".into());
        }) as LocalBoxFuture<()>
    }) as AsyncDisposer
}

struct Dummy;
impl cordis_core::Component for Dummy {
    fn inject(&self) -> cordis_core::KeySet {
        cordis_core::KeySet::new()
    }
    fn provide(&self) -> cordis_core::KeySet {
        cordis_core::KeySet::new()
    }
    fn apply(&self, _c: Rc<Context>, _cfg: &dyn Any) -> Box<dyn cordis_core::EffectIter> {
        Box::new(cordis_core::once(Box::new(|| {
            Box::new(|| {}) as cordis_core::Disposer
        })))
    }
}

fn main() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .expect("worker runtime");
    let combo = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("组合 runtime");

    combo.block_on(async {
        tokio::task::LocalSet::new()
            .run_until(async {
                let core = Rc::new(Runtime::new());
                let ctx: Rc<Context> = core.context();
                let rt = AsyncRuntime::new(&ctx);

                // sync 树：loader 挂 EventsProvider。
                let loader = Rc::new(Loader::new(Rc::clone(&core)));
                loader.register_component("events", Rc::new(EventsProvider));
                loader.apply(&[Entry::new("events", "events", Rc::new(()), 0, false)]);
                let bus = Arc::clone(&*ctx.get::<EventsKey>().expect("总线可读"));

                // 编排方订阅事件（ctx.effect 落账；示例演示 emit 联动）。
                let log: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
                let _sub = subscribe::<Tick>(&ctx, {
                    let log = Arc::clone(&log);
                    move |p: &u32| {
                        log.write().unwrap().push(format!("tick:{p}"));
                    }
                })
                .expect("订阅");

                // 挂 agent-loop async 组件（AsyncFiberHandle 门面）。
                let handle = rt
                    .use_component(
                        &ctx,
                        Rc::new(Dummy),
                        AgentLoopBehavior {
                            log: Arc::clone(&log),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载 agent");

                // 事件联动：订阅收到（sync 段）。
                bus.emit::<Tick>(&7);
                assert!(
                    log.read().unwrap().iter().any(|l| l == "tick:7"),
                    "DX 模板：事件订阅收到"
                );

                // 让 agent 的 drive 先 poll（无限注册器循环开始处理 token）。
                let mut saw_token = false;
                for _ in 0..512 {
                    tokio::task::yield_now().await;
                    if log.read().unwrap().iter().any(|l| l.starts_with("token:")) {
                        saw_token = true;
                        break;
                    }
                }
                assert!(saw_token, "DX 模板：agent loop 已运行（token 处理）");

                // 退役 agent：cancel → 检查点退出 → flush 收账。
                rt.retire(&handle);
                rt.settle().await;
                let l = log.read().unwrap();
                assert!(
                    l.iter().any(|x| x == "loop:exit@cancel"),
                    "DX 模板：卸载检查点退出"
                );
                assert!(
                    l.last().map(|x| x.as_str()) == Some("flush:session"),
                    "DX 模板：卸载 flush session（逆 await 收尾）"
                );
                assert!(rt.is_quiet(), "DX 模板：收账后静止");

                println!("P1.4 插件模板通过：事件订阅 + agent-loop + 卸载 flush（log={l:?}）");
            })
            .await;
    });
    let _ = worker; // worker 在此 drop（非 async 上下文）✓
}
