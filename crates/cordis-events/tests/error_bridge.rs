//! 错误策略 E2：loader `EntryFailedHook` 经 events 发射 `loader/entry-failed`
//!（integration 点——loader 零依赖 events，事件发射由注入 hook 完成）。

use cordis_core::{Component, Context, KeySet, Runtime};
use cordis_events::{Event, EventBus, EventsKey, EventsProvider, subscribe};
use cordis_loader::{Entry, EntryError, EntryErrorKind, Loader};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

/// 事件：条目失败（载荷 = 类型化 `EntryError`）。
struct EntryFailed;
impl Event for EntryFailed {
    type Payload = EntryError;
    const SYMBOL: &'static str = "loader/entry-failed";
}

/// 空组件（loader 注册表用）。
struct Dummy;
impl Component for Dummy {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _c: Rc<Context>, _cfg: &dyn std::any::Any) -> Box<dyn cordis_core::EffectIter> {
        Box::new(cordis_core::once(Box::new(|| {
            Box::new(|| {}) as cordis_core::Disposer
        })))
    }
}

#[test]
fn entry_failed_hook_bridges_to_events() {
    // loader + events 同 core ctx。
    let runtime = Rc::new(Runtime::new());
    let ctx: Rc<Context> = runtime.context();
    let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
    loader.register_component("events", Rc::new(EventsProvider));
    loader.register_component("ok", Rc::new(Dummy));
    loader.apply(&[Entry::new("events", "events", Rc::new(()), 0, false)]);

    let bus = Arc::clone(&*ctx.get::<EventsKey>().expect("总线可读"));
    let log: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
    let _sub = subscribe::<EntryFailed>(&ctx, {
        let log = Arc::clone(&log);
        move |e: &EntryError| {
            log.write().unwrap().push(e.to_string());
        }
    })
    .expect("订阅 loader/entry-failed");

    // loader 失败 hook → events 发射。
    loader.register_entry_failed_hook(Some(Rc::new(move |e: &EntryError| {
        bus.emit::<EntryFailed>(e);
    })));

    // 触发 OrchestrationError（未知组件）。
    let report = loader.apply(&[
        Entry::new("ok", "ok", Rc::new(()), 0, false),
        Entry::new("x", "ghost", Rc::new(()), 0, false),
    ]);
    assert!(!report.ok(), "apply 报告失败（未知组件）");

    // 事件已发射并被订阅收到（emit 同步派发）。
    let l = log.read().unwrap();
    assert!(
        l.iter()
            .any(|s| s.contains("ghost") && s.contains("未知组件")),
        "loader/entry-failed 事件载荷含组件名与原因（经 hook 发射）：{l:?}"
    );
    drop(l);
}
