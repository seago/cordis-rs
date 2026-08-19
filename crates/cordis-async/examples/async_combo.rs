//! P1.3 R3 组合示例：sync 树 + async 组合线程 + Remote 远端回路共存。
//!
//! 拓扑（`docs/cordis-async-THREADING.md`）：loader 挂 `EventsProvider`
//!（sync 树根条目），async 组件经 `AsyncRuntime::use_component` 挂到同一
//! core ctx；组件行为 `spawn_remote`（Send-future 分池形态）→ join 回灌。
//!
//! 运行：`cargo run -p cordis-async --example async_combo`

use cordis_async::{
    AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncRuntime, AsyncStep,
    LocalBoxFuture, RemoteRequest, RemoteValue, TokioRemote,
};
use cordis_core::{Component, Context, Disposer, EffectIter, KeySet, Runtime, once};
use cordis_events::EventsProvider;
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

/// 空 sync 壳组件（d/p 空；apply 不被 AsyncRegistrar 调用）。
struct Dummy;
impl Component for Dummy {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(once(Box::new(|| Box::new(|| {}) as Disposer)))
    }
}

/// async 组件行为：spawn_remote（Send-future 分池）→ join 回灌计数。
struct ComboBehavior {
    hits: Arc<RwLock<u32>>,
}

impl AsyncBehavior for ComboBehavior {
    fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
        Box::new(ComboIter {
            cx,
            hits: Arc::clone(&self.hits),
            done: false,
        })
    }
}

struct ComboIter {
    cx: AsyncCx,
    hits: Arc<RwLock<u32>>,
    done: bool,
}

impl AsyncEffectIter for ComboIter {
    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
        assert!(!self.done, "单步迭代器至多一步");
        self.done = true;
        let cx = self.cx.clone();
        let hits = Arc::clone(&self.hits);
        Box::pin(async move {
            // 远端 Send-future 计算（worker multi_thread 池；O-6 隔离）。
            let join = cx.spawn_remote(RemoteRequest::from_future(async {
                Box::new(7u32) as RemoteValue
            }));
            let value = join.await;
            let n = *value.downcast::<u32>().expect("远端结果类型");
            *hits.write().unwrap() += n;
            AsyncStep::Finished(empty_disposer())
        })
    }
}

fn empty_disposer() -> AsyncDisposer {
    Box::new(|| Box::pin(async {}) as LocalBoxFuture<()>)
}

fn main() {
    // 卫星 worker（multi_thread：future 池 + blocking 池）。
    let worker = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("worker runtime");
    // 组合线程（current_thread + LocalSet）。
    let combo = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("组合 runtime");

    combo
        .block_on(async {
            tokio::task::LocalSet::new()
                .run_until(async {
                    // sync 树 + async 层共用同一 core runtime / ctx。
                    let core = Rc::new(Runtime::new());
                    let ctx: Rc<Context> = core.context();
                    let rt = AsyncRuntime::new(&ctx);
                    rt.set_remote(Rc::new(TokioRemote::new(worker.handle().clone())));

                    // sync 树：loader 挂 EventsProvider 根条目。
                    let loader = Rc::new(Loader::new(Rc::clone(&core)));
                    loader.register_component("events", Rc::new(EventsProvider));
                    loader.apply(&[Entry::new("events", "events", Rc::new(()), 0, false)]);

                    // async 层：挂 async 组件（同 ctx，共享 store）。
                    let hits: Arc<RwLock<u32>> = Arc::new(RwLock::new(0));
                    let handle = rt
                        .use_component(
                            &ctx,
                            Rc::new(Dummy),
                            ComboBehavior {
                                hits: Arc::clone(&hits),
                            },
                            Rc::new(()) as Rc<dyn Any>,
                        )
                        .expect("挂载 async 组件");

                    // 等 Send-future 回灌落地（轮询预算放宽同并行负载）。
                    for _ in 0..512 {
                        tokio::task::yield_now().await;
                        if *hits.read().unwrap() >= 7 {
                            break;
                        }
                    }
                    assert_eq!(
                        *hits.read().unwrap(),
                        7,
                        "Remote Send-future 回灌（worker 池）"
                    );

                    // 收账：退役（门面 C-4）+ settle。
                    rt.retire(&handle);
                    rt.settle().await;

                    println!(
                        "R3 组合示例通过：sync 树(loader)+async 层(use_component)+Remote 回路共存，hits={}",
                        *hits.read().unwrap()
                    );
                })
                .await;
        });
    // worker 在此 drop（非 async 上下文）✓
}
