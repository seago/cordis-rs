//! 产品验证线 P-5：原生 agent 插件（events + async + Remote 全栈回路）。
//!
//! 形态：`user/msg` 事件（sync 订阅 → Arc 消息队列）→ agent 组件（async
//! 长驻 loop：取消息 → `spawn_remote` LLM（worker 执行，O-6）→ join 回灌
//! → 发布 `bot/reply` 事件）→ 出口订阅收到。卸载：retire → cancel →
//! loop 检查点退出 → flush。
//!
//! 运行：`cargo run -p agent-plugin`

use cordis_async::{
    AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncRuntime, AsyncStep,
    LocalBoxFuture, Remote, RemoteRequest, TokioRemote,
};
use cordis_core::{Component, Context, KeySet, Runtime};
use cordis_events::{Event, EventBus, EventsKey, EventsProvider, subscribe};
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

/// 事件：用户消息（入口）与机器人回复（出口）。
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

/// 会话日志（flush 断言用）。
type Log = Arc<RwLock<Vec<String>>>;
/// 入口消息队列（事件 listener 写，agent loop 读——listener 须 Send+Sync，
/// 不能捕获 Rc<Context>；Arc 队列为合规通道）。
type Inbox = Arc<RwLock<std::collections::VecDeque<String>>>;

struct AgentBehavior {
    log: Log,
    inbox: Inbox,
    bus: Arc<EventBus>,
}

impl AsyncBehavior for AgentBehavior {
    fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
        Box::new(AgentLoop {
            cx,
            log: Arc::clone(&self.log),
            inbox: Arc::clone(&self.inbox),
            bus: Arc::clone(&self.bus),
            done: false,
        })
    }
}

/// agent 长驻 loop：取消息 → LLM（worker）→ 发布回复；cancel 退出。
struct AgentLoop {
    cx: AsyncCx,
    log: Log,
    inbox: Inbox,
    bus: Arc<EventBus>,
    done: bool,
}

impl AsyncEffectIter for AgentLoop {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done, "单步迭代器至多一步");
        self.done = true;
        let cx = self.cx.clone();
        let log = Arc::clone(&self.log);
        let inbox = Arc::clone(&self.inbox);
        let bus = Arc::clone(&self.bus);
        Box::pin(async move {
            log.write().unwrap().push("agent:start".into());
            loop {
                if cx.cancellation().cancelled() {
                    log.write().unwrap().push("agent:exit@cancel".into());
                    break;
                }
                // 取入口消息（Arc 队列；无消息空转）。
                let msg = inbox.write().unwrap().pop_front();
                if let Some(msg) = msg {
                    // LLM 调用（worker 执行；O-6 隔离）。
                    log.write().unwrap().push(format!("llm:req:{msg}"));
                    let join =
                        cx.spawn_remote(RemoteRequest::boxed(move || format!("reply:{msg}")));
                    let reply = *join.await.downcast::<String>().expect("LLM 回复类型");
                    log.write().unwrap().push(format!("llm:post:{reply}"));
                    log.write().unwrap().push(format!("llm:{reply}"));
                    // 出口：发布 bot/reply 事件（事件层，async 段 emit）。
                    bus.emit::<BotReply>(&reply);
                }
                tokio::task::yield_now().await;
            }
            log.write().unwrap().push("agent:flush".into());
            AsyncStep::Finished(empty_disposer())
        })
    }
}

fn empty_disposer() -> AsyncDisposer {
    Box::new(|| Box::pin(async {}) as LocalBoxFuture<()>)
}

/// 空 sync 壳组件（agent 组件注册用）。
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

fn main() {
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
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
                rt.set_remote(Rc::new(TokioRemote::new(worker.handle().clone())));

                // 直接链路自检（不经组件）：submit → await 回灌。
                let tv = TokioRemote::new(worker.handle().clone());
                let j = tv.submit(RemoteRequest::boxed(|| "probe".to_string()));
                let v = *j.await.downcast::<String>().expect("probe");
                println!("P5 自检：直接 submit→await 回灌 {v}");
                let _ = v;

                let loader = Rc::new(Loader::new(Rc::clone(&core)));
                loader.register_component("events", Rc::new(EventsProvider));
                loader.apply(&[Entry::new("events", "events", Rc::new(()), 0, false)]);
                let bus = Arc::clone(&*ctx.get::<EventsKey>().expect("总线可读"));

                let log: Log = Arc::new(RwLock::new(Vec::new()));
                let inbox: Inbox = Arc::new(RwLock::new(std::collections::VecDeque::new()));
                // 出口订阅：bot/reply 事件（agent 发布）。
                let _s_reply = subscribe::<BotReply>(&ctx, {
                    let log = Arc::clone(&log);
                    move |p: &String| {
                        log.write().unwrap().push(format!("bot:{p}"));
                    }
                })
                .expect("订阅 bot/reply");

                // 入口订阅：user/msg → 入队（listener Send+Sync：Arc 队列）。
                let _s_in = subscribe::<UserMsg>(&ctx, {
                    let inbox = Arc::clone(&inbox);
                    let log = Arc::clone(&log);
                    move |p: &String| {
                        log.write().unwrap().push(format!("in:{p}"));
                        inbox.write().unwrap().push_back(p.clone());
                    }
                })
                .expect("订阅 user/msg");

                // agent 组件（async 长驻 loop）。
                let handle = rt
                    .use_component(
                        &ctx,
                        Rc::new(Dummy),
                        AgentBehavior {
                            log: Arc::clone(&log),
                            inbox: Arc::clone(&inbox),
                            bus: Arc::clone(&bus),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载 agent");

                // 回路：emit user/msg → 入队 → agent loop → LLM(worker) →
                // 回灌 → bot/reply 事件。
                bus.emit::<UserMsg>(&"你好，天气如何？".to_string());
                for _ in 0..1024 {
                    tokio::task::yield_now().await;
                    // 给 worker（LLM）真实执行窗口（纯 yield 快速耗尽预算——
                    // 既有时序模式 1ms sleep）。
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    if log
                        .read()
                        .unwrap()
                        .iter()
                        .any(|l| l.starts_with("llm:reply:你好"))
                    {
                        break;
                    }
                }
                {
                    let l = log.read().unwrap();
                    assert!(
                        l.iter().any(|x| x.starts_with("llm:reply:你好")),
                        "agent 回路：LLM(worker) 回灌（log={l:?}）"
                    );
                    assert!(
                        l.iter().any(|x| x.starts_with("bot:reply:你好")),
                        "agent 回路：bot/reply 事件发布（log={l:?}）"
                    );
                }

                // 卸载：retire → cancel → 检查点退出 → flush。
                rt.retire(&handle);
                rt.settle().await;
                {
                    let l = log.read().unwrap();
                    assert!(l.iter().any(|x| x == "agent:exit@cancel"), "卸载检查点退出");
                    assert!(
                        l.last().map(|x| x.as_str()) == Some("agent:flush"),
                        "卸载 flush"
                    );
                    assert!(rt.is_quiet(), "收账后静止");
                    println!(
                        "P-5 原生 agent 插件通过：事件→LLM(worker)→回灌→bot/reply→flush（log={l:?}）"
                    );
                }
            })
            .await;
    });
    let _ = worker;
}
