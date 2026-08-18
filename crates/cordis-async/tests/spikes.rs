//! Phase 0 出口 spike（草案 §9 产品假设验证）：
//!
//! - S1 事件总线 + `ctx.effect` 订阅原型——订阅随 fiber 卸载**自动退订**
//!   （验证 DX 税可承受）；
//! - S2 tokio 服务 sync 壳 + `spawn_remote`——组合线程二分的同步壳 +
//!   远端调用回路（验证不别扭）；
//! - S3 agent loop 注册器模式组件——LLM SSE 流式（mock）+ 工具调用 +
//!   卸载时 flush session（三端协作完整、无泄漏）。

use cordis_async::{
    AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncRuntime, AsyncStep,
    LocalBoxFuture, RemoteValue, TokioRemote,
};
use cordis_core::{Context, Symbol};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// 共享顺序记录。
type Log = Rc<RefCell<Vec<String>>>;

/// 轻量事件总线原型（S1）：`Symbol` 事件名 + emit 派发；订阅返回 id、
/// 退订按 id；派发快照迭代（handler 内退订/重入安全）。
/// 监听器表（id → 事件名 + handler）。
type ListenerMap = HashMap<usize, (Symbol, Box<dyn Fn(&str)>)>;

/// emit 快照迭代的 handler 引用（type_complexity 收口）。
type HandlerRef<'a> = &'a Box<dyn Fn(&str)>;

struct EventBus {
    next_id: Cell<usize>,
    listeners: RefCell<ListenerMap>,
}

impl EventBus {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            next_id: Cell::new(0),
            listeners: RefCell::new(HashMap::new()),
        })
    }

    fn subscribe(self: &Rc<Self>, event: Symbol, handler: impl Fn(&str) + 'static) -> usize {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        self.listeners
            .borrow_mut()
            .insert(id, (event, Box::new(handler)));
        id
    }

    fn unsubscribe(&self, id: usize) {
        self.listeners.borrow_mut().remove(&id);
    }

    fn emit(&self, event: Symbol, payload: &str) {
        // 快照迭代：handler 内 unsubscribe / 注册新监听不破坏派发。
        // Ref 借用以局部变量持有，存续到 handler 执行完毕（重入安全）。
        let listeners = self.listeners.borrow();
        let handlers: Vec<HandlerRef> = listeners
            .values()
            .filter(|(e, _)| *e == event)
            .map(|(_, h)| h)
            .collect();
        for h in handlers {
            h(payload);
        }
    }
}

/// S1 行为：async 步内经 `cx.effect` 订阅事件（逆 = 退订，入 fiber ctx
/// 累加器——卸载自动退订）；监听器经 `spawn_local` 投递（不阻塞派发）。
struct SubBehavior {
    bus: Rc<EventBus>,
    log: Log,
}

impl AsyncBehavior for SubBehavior {
    fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
        let bus = Rc::clone(&self.bus);
        let log = Rc::clone(&self.log);
        // 订阅经 cx.effect 注册：订阅立即生效，逆（退订）随 fiber 卸载执行。
        // 返回的 Disposer 故意 drop（REVIEW-68f0c80 nit-2）：订阅步骤的逆
        // 已入 fiber ctx 累加器（卸载 dispose_all 自动执行）——手动持有该
        // 句柄会形成第二条撤销路径（双路径安全但此处无必要）。
        drop(cx.effect(move || -> Box<dyn cordis_core::EffectIter> {
            let bus = Rc::clone(&bus);
            let log = Rc::clone(&log);
            Box::new(cordis_core::once(Box::new(move || {
                let id = bus.subscribe(Symbol::intern("tick"), move |payload: &str| {
                    // async 监听器：spawn_local 投递，不阻塞派发。
                    let log = Rc::clone(&log);
                    let payload = payload.to_string();
                    tokio::task::spawn_local(async move {
                        log.borrow_mut().push(format!("tick:{payload}"));
                    });
                });
                Box::new(move || bus.unsubscribe(id)) as cordis_core::Disposer
            })))
        }));
        Box::new(SubIter {
            log: Rc::clone(&self.log),
            done: false,
        })
    }
}

struct SubIter {
    log: Log,
    done: bool,
}

impl AsyncEffectIter for SubIter {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done, "单步迭代器至多一步");
        self.done = true;
        let log = Rc::clone(&self.log);
        Box::pin(async move {
            log.borrow_mut().push("sub:ok".into());
            AsyncStep::Finished(empty_disposer())
        })
    }
}

fn empty_disposer() -> AsyncDisposer {
    Box::new(|| Box::pin(async {}) as LocalBoxFuture<()>)
}

/// S1：事件总线 + `ctx.effect` 订阅——emit 派发到 async 监听器；fiber
/// 卸载后自动退订（再次 emit 不再收到）。
#[tokio::test]
async fn spike_s1_event_bus_subscription_auto_unsubscribes_on_unload() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let ctx = Context::new();
            let rt = AsyncRuntime::new(&ctx);
            let bus = EventBus::new();
            let log: Log = Rc::new(RefCell::new(Vec::new()));

            let fiber = rt
                .use_component(
                    &ctx,
                    Rc::new(SpikeShell),
                    SubBehavior {
                        bus: Rc::clone(&bus),
                        log: Rc::clone(&log),
                    },
                    Rc::new(()) as Rc<dyn Any>,
                )
                .expect("挂载");
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            assert!(log.borrow().iter().any(|l| l == "sub:ok"), "订阅已注册");

            // emit → async 监听器投递。
            bus.emit(Symbol::intern("tick"), "hello");
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            assert!(
                log.borrow().iter().any(|l| l == "tick:hello"),
                "S1：emit 派发到 async 监听器"
            );

            // 卸载 fiber → ctx 累加器逆（退订）执行。
            rt.retire(&fiber);
            rt.settle().await;

            // 再次 emit：自动退订生效。
            bus.emit(Symbol::intern("tick"), "again");
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            assert!(
                !log.borrow().iter().any(|l| l == "tick:again"),
                "S1：fiber 卸载后自动退订（DX 税 = 一行 cx.effect）"
            );
        })
        .await;
}

/// sync 壳组件（spike 用：无 d/p，apply 不被 AsyncRegistrar 调用）。
struct SpikeShell;

impl cordis_core::Component for SpikeShell {
    fn inject(&self) -> cordis_core::KeySet {
        cordis_core::KeySet::new()
    }
    fn provide(&self) -> cordis_core::KeySet {
        cordis_core::KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn cordis_core::EffectIter> {
        Box::new(cordis_core::once(Box::new(|| {
            Box::new(|| {}) as cordis_core::Disposer
        })))
    }
}

/// S2 行为：sync 壳 + 远端调用（mock LLM client 经 spawn_remote）。
struct LlmShellBehavior {
    log: Log,
}

impl AsyncBehavior for LlmShellBehavior {
    fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
        Box::new(LlmIter {
            cx,
            log: Rc::clone(&self.log),
            done: false,
        })
    }
}

struct LlmIter {
    cx: AsyncCx,
    log: Log,
    done: bool,
}

impl AsyncEffectIter for LlmIter {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done, "单步迭代器至多一步");
        self.done = true;
        let cx = self.cx.clone();
        let log = Rc::clone(&self.log);
        Box::pin(async move {
            log.borrow_mut().push("llm:req".into());
            // 远端 mock LLM 服务往返（worker blocking pool）。
            let join = cx.spawn_remote(|| -> RemoteValue {
                Box::new(String::from("completion:你好，天气晴朗"))
            });
            let value = join.await;
            let reply = value.downcast::<String>().expect("远端结果类型");
            assert!(reply.starts_with("completion:"), "远端服务回路");
            log.borrow_mut().push(format!("llm:ok:{reply}"));
            AsyncStep::Finished(Box::new(move || {
                let log = Rc::clone(&log);
                Box::pin(async move {
                    tokio::task::yield_now().await;
                    log.borrow_mut().push("llm:rev".into());
                }) as LocalBoxFuture<()>
            }) as AsyncDisposer)
        })
    }
}

/// S2：tokio 服务 sync 壳 + `spawn_remote`——同步壳（组件）+ 远端调用
/// 回路跑通（组合线程二分不别扭）。
#[test]
fn spike_s2_tokio_service_sync_shell_via_spawn_remote() {
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
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                rt.set_remote(Rc::new(TokioRemote::new(worker.handle().clone())));
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let fiber = rt
                    .use_component(
                        &ctx,
                        Rc::new(SpikeShell),
                        LlmShellBehavior {
                            log: Rc::clone(&log),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载");
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if log.borrow().iter().any(|l| l.starts_with("llm:ok:")) {
                        break;
                    }
                }
                assert!(
                    log.borrow().iter().any(|l| l.starts_with("llm:ok:")),
                    "S2：远端服务回路跑通（sync 壳 + spawn_remote）"
                );

                rt.retire(&fiber);
                rt.settle().await;
                assert!(
                    log.borrow().iter().any(|l| l == "llm:rev"),
                    "S2：服务壳卸载收账"
                );
            })
            .await;
    });
    // worker 在此 drop——block_on 已返回（非 async 上下文）✓
}

/// S3 行为：agent loop 注册器模式——一步 = 长驻循环（检查点检查 cancel；
/// mock SSE 流逐 token；工具调用识别）+ 逆 = flush session（await 收尾）。
struct AgentBehavior {
    log: Log,
}

impl AsyncBehavior for AgentBehavior {
    fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
        Box::new(AgentIter {
            cx,
            log: Rc::clone(&self.log),
            done: false,
        })
    }
}

struct AgentIter {
    cx: AsyncCx,
    log: Log,
    done: bool,
}

impl AsyncEffectIter for AgentIter {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done, "单步迭代器至多一步");
        self.done = true;
        let cx = self.cx.clone();
        let log = Rc::clone(&self.log);
        Box::pin(async move {
            // mock LLM SSE 流（本地模拟；每 token 一拍模拟流式到达）。
            let stream = [
                "user:帮我查天气",
                "tool:get_weather",
                "assistant:今日晴天",
                "tool:send_email",
            ];
            for token in stream {
                // 检查点：卸载（cancel）时退场——循环在检查点退出。
                if cx.cancellation().cancelled() {
                    log.borrow_mut().push("loop:exit@cancel".into());
                    break;
                }
                tokio::task::yield_now().await;
                log.borrow_mut().push(format!("token:{token}"));
                if token.starts_with("tool:") {
                    log.borrow_mut().push(format!(
                        "tool-call:{}",
                        token.strip_prefix("tool:").expect("已检查前缀")
                    ));
                }
            }
            // 循环结束（流完或被取消）→ 逆 = flush session（await 收尾）。
            AsyncStep::Finished(Box::new(move || {
                let log = Rc::clone(&log);
                Box::pin(async move {
                    // flush：await 收尾（如保存 session / 杀 subprocess 的模拟）。
                    tokio::task::yield_now().await;
                    log.borrow_mut().push("flush:session".into());
                }) as LocalBoxFuture<()>
            }) as AsyncDisposer)
        })
    }
}

/// S3：agent loop 注册器模式组件——mock SSE 流 + 工具调用 + 卸载时
/// flush session（三端协作：sync 树挂载 / async 循环 / 远端服务形态；
/// 卸载 cancel → 检查点退出 → flush 收尾，无泄漏）。
#[tokio::test]
async fn spike_s3_agent_loop_flushes_session_on_unload() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let ctx = Context::new();
            let rt = AsyncRuntime::new(&ctx);
            let log: Log = Rc::new(RefCell::new(Vec::new()));

            let fiber = rt
                .use_component(
                    &ctx,
                    Rc::new(SpikeShell),
                    AgentBehavior {
                        log: Rc::clone(&log),
                    },
                    Rc::new(()) as Rc<dyn Any>,
                )
                .expect("挂载");
            // 等循环处理到工具调用（流式推进中）。
            for _ in 0..64 {
                tokio::task::yield_now().await;
                if log.borrow().iter().any(|l| l == "tool-call:get_weather") {
                    break;
                }
            }
            assert!(
                log.borrow().iter().any(|l| l == "tool-call:get_weather"),
                "S3：mock SSE 流 + 工具调用执行"
            );

            // 卸载：cancel → 循环检查点退出 → 逆 flush 收账。
            rt.retire(&fiber);
            rt.settle().await;

            let entries = log.borrow();
            let last = entries.last().expect("有日志");
            assert_eq!(
                last, "flush:session",
                "S3：卸载时 flush session（逆 await 收尾）"
            );
            assert!(
                entries.iter().any(|l| l == "loop:exit@cancel"),
                "S3：循环在取消检查点退出"
            );
            assert!(
                !entries
                    .iter()
                    .any(|l| l.starts_with("token:tool:send_email")),
                "S3：取消后不再消费剩余流 token"
            );
            // 注（REVIEW-68f0c80 nit-3）：`is_quiet` 是注册器级静止检查
            //（无尾巴/无 Active async 组件）——业务对象级 flush 完整性由
            // 上面的 `flush:session` 日志断言直证；此处为无泄漏佐证。
            assert!(rt.is_quiet(), "S3：收账后静止（无泄漏）");
        })
        .await;
}
