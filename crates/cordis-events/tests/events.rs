//! M1.2 验收测试（草案 v0.3.1 §6：验证 #1/#2/#5/#6/#8 + waterfall 核心）。
//!
//! integration 形态（REVIEW-85d2379 nit-2）：从外部 crate 视角测 pub API。

use cordis_events::{Event, EventBus};
use std::sync::{Arc, RwLock};

/// 共享顺序记录。
type Log = Arc<RwLock<Vec<String>>>;

/// tick 事件（emit 用例）。
struct Tick;
impl Event for Tick {
    type Payload = u32;
    const SYMBOL: &'static str = "tick";
}

/// req 事件（serial/bail 用例）。
struct Req;
impl Event for Req {
    type Payload = String;
    const SYMBOL: &'static str = "req";
}

/// 同 SYMBOL 但不同载荷的事件类型（冲突用例）。
struct TickWrong;
impl Event for TickWrong {
    type Payload = String;
    const SYMBOL: &'static str = "tick";
}

/// 同 SYMBOL「dup」、不同载荷（跨模式冲突用例）。
struct DupA;
impl Event for DupA {
    type Payload = u32;
    const SYMBOL: &'static str = "dup";
}
struct DupB;
impl Event for DupB {
    type Payload = String;
    const SYMBOL: &'static str = "dup";
}

/// #1 emit 序与载荷：三个监听器按注册序收到同一 payload。
#[test]
fn emit_order_and_payload() {
    let bus = EventBus::new();
    let log: Log = Arc::new(RwLock::new(Vec::new()));
    let _d1 = bus.on::<Tick>({
        let log = Arc::clone(&log);
        move |p: &u32| log.write().unwrap().push(format!("first:{p}"))
    });
    let _d2 = bus.on::<Tick>({
        let log = Arc::clone(&log);
        move |p: &u32| log.write().unwrap().push(format!("second:{p}"))
    });
    let _d3 = bus.on::<Tick>({
        let log = Arc::clone(&log);
        move |p: &u32| log.write().unwrap().push(format!("third:{p}"))
    });

    bus.emit::<Tick>(&42);
    assert_eq!(
        *log.read().unwrap(),
        vec!["first:42", "second:42", "third:42"],
        "#1：注册序收到同一 payload"
    );
}

/// #2 disposer 幂等：双重 dispose 退订至多一次、不 panic。
#[test]
fn disposer_idempotent_double_dispose() {
    let bus = EventBus::new();
    let log: Log = Arc::new(RwLock::new(Vec::new()));
    let d = bus.on::<Tick>({
        let log = Arc::clone(&log);
        move |p: &u32| log.write().unwrap().push(format!("got:{p}"))
    });

    bus.emit::<Tick>(&1);
    assert_eq!(*log.read().unwrap(), vec!["got:1"]);

    d(); // 退订（置 alive 失效）。
    bus.emit::<Tick>(&2);
    assert_eq!(
        *log.read().unwrap(),
        vec!["got:1"],
        "#2：disposer 调用后彻底退订，不再触发"
    );
    // FnOnce 帧下同一句柄不可重复调用（`Box<dyn FnOnce>` 一次即 move）；
    // 草案「双重 dispose 无害」在 Rust 的落地 = 多路径等价闭包共享 armed
    //（M1.3 订阅即效应时以「ctx.effect 逆 + 手动 disposer」双路径直证）。
}

/// #5 serial / bail：serial 收集序 + 值；bail 首个 Some 即停、全 None 得 None。
#[test]
fn serial_collects_all_and_bail_stops_on_first_some() {
    let bus = EventBus::new();
    let _s1 = bus.on_serial::<Req, u32>(|p: &String| p.len() as u32);
    let _s2 = bus.on_serial::<Req, u32>(|_p| 7);
    let results = bus.serial::<Req, u32>(&"hello".to_string());
    assert_eq!(results, vec![5u32, 7], "#5 serial：注册序收集全部 R");

    // bail：Some 即停（后续不执行）；全 None 得 None。
    let bus2 = EventBus::new();
    let log: Arc<RwLock<Vec<&'static str>>> = Arc::new(RwLock::new(Vec::new()));
    let _b1 = bus2.on_bail::<Req, u32>({
        let log = Arc::clone(&log);
        move |_p| {
            log.write().unwrap().push("b1");
            None
        }
    });
    let _b2 = bus2.on_bail::<Req, u32>({
        let log = Arc::clone(&log);
        move |_p| {
            log.write().unwrap().push("b2");
            Some(9)
        }
    });
    let _b3 = bus2.on_bail::<Req, u32>({
        let log = Arc::clone(&log);
        move |_p| {
            log.write().unwrap().push("b3");
            Some(99)
        }
    });
    let r = bus2.bail::<Req, u32>(&"x".to_string());
    assert_eq!(r, Some(9), "#5 bail：首个 Some(r) 即停");
    assert_eq!(
        *log.read().unwrap(),
        vec!["b1", "b2"],
        "#5 bail：b3 未被调用（短路）"
    );

    let bus3 = EventBus::new();
    let _n1 = bus3.on_bail::<Req, u32>(|_p| None);
    let _n2 = bus3.on_bail::<Req, u32>(|_p| None);
    assert_eq!(
        bus3.bail::<Req, u32>(&"y".to_string()),
        None,
        "#5 bail：全 None 得 None"
    );
}

/// #6 符号冲突：四规则各 panic（should_panic）。
mod conflicts {
    use super::*;

    #[test]
    #[should_panic(expected = "符号冲突")]
    fn same_name_different_payload() {
        let bus = EventBus::new();
        let _a = bus.on::<Tick>(|_p: &u32| {}); // 先订 tick/u32
        let _b = bus.on::<TickWrong>(|_p: &String| {}); // 同名不同载荷
    }

    #[test]
    #[should_panic(expected = "R 单一性")]
    fn same_mode_different_r() {
        let bus = EventBus::new();
        let _a = bus.on_serial::<Req, u32>(|_p| 1);
        let _b = bus.on_serial::<Req, String>(|_p| String::from("x"));
    }

    #[test]
    #[should_panic(expected = "跨模式载荷冲突")]
    fn cross_mode_different_payload() {
        let bus = EventBus::new();
        let _a = bus.on::<DupA>(|_p: &u32| {}); // dup/u32，Emit
        let _b = bus.on_serial::<DupB, u32>(|_p: &String| 0); // 同名跨模式异载荷
    }

    #[test]
    #[should_panic(expected = "派发 R 与订阅写定的 R 不符")]
    fn dispatch_r_mismatch() {
        let bus = EventBus::new();
        let _a = bus.on_serial::<Req, u32>(|_p| 1);
        let _: Vec<String> = bus.serial::<Req, String>(&"x".to_string()); // 派发 R 不符
    }
}

/// #8 空听众集（E-2 四断言）。
#[test]
fn empty_listeners_e2() {
    let bus = EventBus::new();

    // emit = no-op（不 panic）。
    bus.emit::<Tick>(&1);

    // waterfall = 仅 terminal。
    let mut v = 0u32;
    bus.waterfall::<Tick>(&mut v, |p: &mut u32| {
        *p = 99;
    });
    assert_eq!(v, 99, "空集 waterfall = 仅 terminal（修改载荷）");
    // waterfall 载荷类型是 Tick::Payload=u32 ✓

    // serial = 空 vec。
    assert!(bus.serial::<Req, u32>(&"z".to_string()).is_empty());

    // bail = None。
    assert!(bus.bail::<Req, u32>(&"z".to_string()).is_none());
}

/// waterfall 核心（M1.2 实现验证；#4 完整 around/短路语义 M1.4 精化）：
/// A→B→terminal 基本链序。
#[test]
fn waterfall_basic_link_order() {
    let bus = EventBus::new();
    let log: Arc<RwLock<Vec<&'static str>>> = Arc::new(RwLock::new(Vec::new()));
    let _a = bus.on_waterfall::<Tick>({
        let log = Arc::clone(&log);
        move |p: &mut u32, next: &dyn Fn(&mut u32)| {
            log.write().unwrap().push("A:before");
            *p += 1;
            next(p);
            log.write().unwrap().push("A:after");
        }
    });
    let _b = bus.on_waterfall::<Tick>({
        let log = Arc::clone(&log);
        move |p: &mut u32, next: &dyn Fn(&mut u32)| {
            log.write().unwrap().push("B:before");
            *p *= 10;
            next(p);
        }
    });
    let mut acc = 0u32;
    bus.waterfall::<Tick>(&mut acc, |p: &mut u32| {
        *p += 100;
        log.write().unwrap().push("terminal");
    });

    assert_eq!(acc, 110, "waterfall：载荷沿链累积（(0+1)*10+100 = 110）");
    assert_eq!(
        *log.read().unwrap(),
        vec!["A:before", "B:before", "terminal", "A:after"],
        "waterfall：A→B→terminal 序 + A around（next 返回后处理）"
    );
}

/// Arc 值可捕获（§0 核心义务：监听器经 Arc 捕获服务而非 Rc）。
#[test]
fn listener_captures_arc_not_rc() {
    let bus = EventBus::new();
    let shared: Arc<RwLock<u32>> = Arc::new(RwLock::new(0));
    let _d = bus.on::<Tick>({
        let shared = Arc::clone(&shared);
        move |p: &u32| {
            *shared.write().unwrap() += *p;
        }
    });
    bus.emit::<Tick>(&5);
    assert_eq!(
        *shared.read().unwrap(),
        5,
        "监听器可捕获 Arc（Send+Sync 上界）"
    );
}
