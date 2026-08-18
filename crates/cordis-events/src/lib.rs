//! cordis-events 层（草案 v0.3.1 冻结，Phase 1 第一交付物）。
//!
//! 定位：cordis-rs 架构决策 C 的事件系统层。纯 sync、零依赖（仅依赖
//! `cordis-core`）——类型化事件（`Key` 的镜像）、四种 sync 派发
//! （emit / waterfall / serial / bail；`parallel` 推迟到 async 层，
//! 见草案 §0/§5）、订阅即效应（订阅经 `ctx.effect` 注册，fiber 卸载
//! 自动退订，M1.3 落地）。
//!
//! **核心义务（草案 §0）**：事件总线监听器闭包须带 `Send + Sync + 'static`
//! 上界——总线是 store 内全局服务，`EventsKey::Value = Arc<EventBus>` 必须
//! 满足 core `Key::Value: Send+Sync`。代价：监听器**不得捕获 `Rc`**
//! （Context/Fiber 等）——需要服务时经 `Arc` 捕获（C-1 惯例）。捕获 `Rc`
//! 的线程私有总线不属本层（另设非 Send+Sync 变体，草案 O-6'）。
//!
//! **实现注记（M1.2）**：
//! - 内部监听器以 `Arc<dyn Fn …>` 存储（草案 §3.2 的 `Box` 为示意）——
//!   `release-then-invoke`（草案 M-1 建议）要求派发在**释放锁后**调用闭包，
//!   快照须持有可复制的闭包句柄（`Arc` 克隆）；`Box` 不可克隆，故存 `Arc`。
//! - 退订 = 置 `alive` 标志失活（幂等）；表项作 tombstone 由后续订阅复用，
//!   表大小 ≤ 峰值活跃订阅数（无泄漏）。
//!
//! 依据：`docs/cordis-events-protocol-draft.md` v0.3.1（冻结）；
//! 执行计划 `docs/cordis-events-PHASE1-PLAN.md`（M1.2 订阅/派发核心）。

#![deny(missing_docs)]

use cordis_core::{Component, Context, Disposer, EffectIter, Key, KeySet, Symbol};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// 类型化事件（草案 §1：与 core `Key` 完全同构——身份 = 事件 `SYMBOL`，载荷 =
/// 关联类型；命名对齐 `Key::SYMBOL`，内部经 [`Symbol::intern`] 与 Key 同
/// 驻留）。
///
/// 义务：两个事件类型**不得**声明相同的 [`Event::SYMBOL`]（与 Key 的符号
/// 纪律同款；违反在订阅点 panic = bug）。
pub trait Event: 'static {
    /// 载荷类型（值传递/可变借用，见各派发模式）。
    type Payload: 'static;

    /// 事件名（驻留为 core `Symbol`）。
    const SYMBOL: &'static str;
}

// ── 监听器类型（草案 §2.1）──────────────────────────────────────────

/// emit 监听器（观察：`&P` 只读）。
pub type EmitListener<P> = Box<dyn Fn(&P) + Send + Sync + 'static>;

/// waterfall 监听器（around 中间件：`&mut P` 载荷 + `next` 链委托下游；
/// 不调 next = 短路）。
pub type WaterfallListener<P> = Box<dyn Fn(&mut P, &dyn Fn(&mut P)) + Send + Sync + 'static>;

/// serial 监听器（`Fn(&P) -> R`，串行收集全部返回值）。
pub type SerialListener<P, R> = Box<dyn Fn(&P) -> R + Send + Sync + 'static>;

/// bail 监听器（`Fn(&P) -> Option<R>`：`Some(r)` = 作答并停止派发；
/// `None` = 不答、继续下一听众）。
pub type BailListener<P, R> = Box<dyn Fn(&P) -> Option<R> + Send + Sync + 'static>;

// ── EventBus 核心（草案 §2/§3.2）────────────────────────────────────

/// 派发模式（草案 §3.2 `Mode` 枚举；serial 与 bail 的 R 相互独立）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Mode {
    Emit,
    Waterfall,
    Serial,
    Bail,
}

/// 内部擦除的 emit 闭包（release-then-invoke 快照句柄；`Arc` 可复制）。
type EmitAnyFn = Arc<dyn Fn(&dyn Any) + Send + Sync>;
/// 内部擦除的 waterfall 闭包。
type WaterfallAnyFn = Arc<dyn Fn(&mut dyn Any, &dyn Fn(&mut dyn Any)) + Send + Sync>;
/// 内部擦除的 reply 闭包（serial/bail；`Box<dyn Any+Send+Sync>` 装箱）。
type ReplyAnyFn = Arc<dyn Fn(&dyn Any) -> Box<dyn Any + Send + Sync> + Send + Sync>;

/// 注册规格：冲突检测所需元数据（载荷/R 的 TypeId + 类型名）。
struct ModeSpec {
    type_id: TypeId,
    type_name: &'static str,
    r_type_id: Option<TypeId>,
    r_name: Option<&'static str>,
}

/// 内部监听器条目（`Arc<dyn Fn …>` 存储以便 release-then-invoke 快照克隆；
/// `alive` 支持幂等退订与 E-1 快照跳过）。
enum ListenerEntry {
    // 载荷/R 的 TypeId 由 modes 表权威记录（冲突检测）；条目本身不冗余携带。
    Emit {
        alive: Arc<AtomicBool>,
        f: EmitAnyFn,
    },
    Waterfall {
        alive: Arc<AtomicBool>,
        f: WaterfallAnyFn,
    },
    Serial {
        alive: Arc<AtomicBool>,
        f: ReplyAnyFn,
    },
    Bail {
        alive: Arc<AtomicBool>,
        f: ReplyAnyFn,
    },
}

impl ListenerEntry {
    fn alive(&self) -> bool {
        match self {
            Self::Emit { alive, .. }
            | Self::Waterfall { alive, .. }
            | Self::Serial { alive, .. }
            | Self::Bail { alive, .. } => alive.load(Ordering::SeqCst),
        }
    }

    /// 本条目属发行模式（Emit/Waterfall/Serial/Bail）。
    fn mode(&self) -> Mode {
        match self {
            Self::Emit { .. } => Mode::Emit,
            Self::Waterfall { .. } => Mode::Waterfall,
            Self::Serial { .. } => Mode::Serial,
            Self::Bail { .. } => Mode::Bail,
        }
    }
}

/// modes 表记录：载荷/R 的 TypeId（比较用）+ 类型名（冲突诊断含双方类型名，
/// 草案 §3.2「错误信息含双方类型名」要求）。Emit/Waterfall 的 R 位 = `()`。
struct ModeRecord {
    type_id: TypeId,
    type_name: &'static str,
    /// 本模式是否有 R（Emit/Waterfall 无）。显式标志，免字符串哨兵（Nit-1）。
    has_r: bool,
    r_type_id: TypeId,
    r_name: &'static str,
}

/// 事件总线（草案 §3.2，评审 M-1/M-1'/m-4/Minor-2 修订落地）。
///
/// - `modes`：单一冲突检测表 `(Symbol, Mode) → (载荷 TypeId, R TypeId)`
///   （Emit/Waterfall 的 R 位写 `()`）；含**跨模式载荷一致性**检查；
/// - `listeners`：名字 → 注册序监听器表（`Arc<dyn Fn …>` 存储 + `alive`）；
/// - `RwLock` 仅作 `Sync` 包装（ADR-0002 单线程下无真实竞争，临界区短）；
/// - **release-then-invoke**：派发先 `read()` 锁内 collect 快照（闭包 Arc
///   克隆 + alive 过滤），**释放锁后再调用闭包**——闭包内重入订阅/退订
///   不死锁、天然满足 E-1 快照纪律。
pub struct EventBus {
    modes: RwLock<HashMap<(Symbol, Mode), ModeRecord>>,
    listeners: RwLock<HashMap<Symbol, Vec<ListenerEntry>>>,
    _private: (),
}

impl EventBus {
    /// 新建空总线（草案 §3.1 `EventsProvider` 绑定入口）。
    pub fn new() -> Self {
        Self {
            modes: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
            _private: (),
        }
    }

    /// 订阅（emit 专用，草案 §2.1）：返回**幂等 disposer**（重复 dispose
    /// 无害，自研 armed 同款语义）；派发序 = 注册序。
    ///
    /// 冲突检测：同名不同载荷类型 / 同名同模式异 R / **同名跨模式异载荷**
    /// → panic（符号冲突 = bug，含双方类型名诊断）。
    pub fn on<P: Event>(&self, listener: impl Fn(&P::Payload) + Send + Sync + 'static) -> Disposer {
        let fence = Arc::new(AtomicBool::new(true));
        let f: EmitAnyFn = Arc::new(move |any: &dyn Any| {
            let payload = any
                .downcast_ref::<P::Payload>()
                .expect("emit 载荷类型已核验");
            (listener)(payload);
        });
        self.register(
            Symbol::intern(P::SYMBOL),
            Mode::Emit,
            ModeSpec {
                type_id: TypeId::of::<P>(),
                type_name: std::any::type_name::<P>(),
                r_type_id: None,
                r_name: None,
            },
            ListenerEntry::Emit {
                alive: Arc::clone(&fence),
                f,
            },
        );
        disposer(fence)
    }

    /// 订阅（waterfall 专用，草案 §2.1）：listener 收到 `(payload, next)`；
    /// 调 `next()` 委托下游，返回后载荷已含下游处理结果（around 语义）；
    /// 不调 `next()` = 短路（下游与 terminal 不执行）。
    pub fn on_waterfall<P: Event>(
        &self,
        listener: impl Fn(&mut P::Payload, &dyn Fn(&mut P::Payload)) + Send + Sync + 'static,
    ) -> Disposer {
        let fence = Arc::new(AtomicBool::new(true));
        let f: WaterfallAnyFn = Arc::new(move |any: &mut dyn Any, next: &dyn Fn(&mut dyn Any)| {
            let payload = any
                .downcast_mut::<P::Payload>()
                .expect("waterfall 载荷类型已核验");
            let next_typed: &dyn Fn(&mut P::Payload) = &|p: &mut P::Payload| {
                next(p as &mut dyn Any);
            };
            (listener)(payload, next_typed);
        });
        self.register(
            Symbol::intern(P::SYMBOL),
            Mode::Waterfall,
            ModeSpec {
                type_id: TypeId::of::<P>(),
                type_name: std::any::type_name::<P>(),
                r_type_id: None,
                r_name: None,
            },
            ListenerEntry::Waterfall {
                alive: Arc::clone(&fence),
                f,
            },
        );
        disposer(fence)
    }

    /// 订阅（serial 专用，草案 §2.1 方案 2）：listener 返回 `R`，serial
    /// 派发收集全部返回值（TS serial 的 sync 版）。
    pub fn on_serial<P: Event, R>(
        &self,
        listener: impl Fn(&P::Payload) -> R + Send + Sync + 'static,
    ) -> Disposer
    where
        R: Send + Sync + 'static,
    {
        let fence = Arc::new(AtomicBool::new(true));
        let f: ReplyAnyFn = Arc::new(move |any: &dyn Any| -> Box<dyn Any + Send + Sync> {
            let payload = any
                .downcast_ref::<P::Payload>()
                .expect("serial 载荷类型已核验");
            Box::new((listener)(payload))
        });
        self.register(
            Symbol::intern(P::SYMBOL),
            Mode::Serial,
            ModeSpec {
                type_id: TypeId::of::<P>(),
                type_name: std::any::type_name::<P>(),
                r_type_id: Some(TypeId::of::<R>()),
                r_name: Some(std::any::type_name::<R>()),
            },
            ListenerEntry::Serial {
                alive: Arc::clone(&fence),
                f,
            },
        );
        disposer(fence)
    }

    /// 订阅（bail 专用，草案 §2.1 方案 2）：listener 返回 `Option<R>`，
    /// `Some(r)` = 作答并停止派发；`None` = 不答、继续下一听众。
    pub fn on_bail<P: Event, R>(
        &self,
        listener: impl Fn(&P::Payload) -> Option<R> + Send + Sync + 'static,
    ) -> Disposer
    where
        R: Send + Sync + 'static,
    {
        let fence = Arc::new(AtomicBool::new(true));
        let f: ReplyAnyFn = Arc::new(move |any: &dyn Any| -> Box<dyn Any + Send + Sync> {
            let payload = any
                .downcast_ref::<P::Payload>()
                .expect("bail 载荷类型已核验");
            Box::new((listener)(payload))
        });
        self.register(
            Symbol::intern(P::SYMBOL),
            Mode::Bail,
            ModeSpec {
                type_id: TypeId::of::<P>(),
                type_name: std::any::type_name::<P>(),
                r_type_id: Some(TypeId::of::<R>()),
                r_name: Some(std::any::type_name::<R>()),
            },
            ListenerEntry::Bail {
                alive: Arc::clone(&fence),
                f,
            },
        );
        disposer(fence)
    }

    /// 统一注册：冲突检测（四规则）→ tombstone 复用 or push → 记录 modes。
    fn register(&self, name: Symbol, mode: Mode, spec: ModeSpec, entry: ListenerEntry) {
        // 冲突检测（modes 写锁内）。
        let mut modes = self.modes.write().unwrap();
        match modes.get(&(name, mode)) {
            Some(rec) => {
                if rec.type_id != spec.type_id {
                    panic!(
                        "符号冲突：事件 `{name}` 模式 {mode:?} 已有载荷类型 `{}`，正以载荷类型 `{}` 订阅（同名不同载荷 = bug）",
                        rec.type_name, spec.type_name
                    );
                }
                if spec.r_type_id.is_some() && rec.r_type_id != spec.r_type_id.unwrap() {
                    panic!(
                        "R 单一性：事件 `{name}` 模式 {mode:?} 已有返回值类型 `{}`，正以 `{}` 订阅（同事件同模式 R 必须唯一）",
                        rec.r_name,
                        spec.r_name.unwrap_or("()")
                    );
                }
            }
            None => {
                // 跨模式载荷一致性（m-4 回归补强，Minor-2）：同 Symbol 任意
                // 既有模式载荷 TypeId 必须一致（一个 SYMBOL = 一个载荷类型）。
                for (&n, rec) in modes.iter() {
                    if n.0 == name && rec.type_id != spec.type_id {
                        panic!(
                            "跨模式载荷冲突：事件 `{name}` 其他模式已用载荷类型 `{}`，正以 `{}` 订阅（一个 SYMBOL = 一个载荷类型，Minor-2）",
                            rec.type_name, spec.type_name
                        );
                    }
                }
                modes.insert(
                    (name, mode),
                    ModeRecord {
                        type_id: spec.type_id,
                        type_name: spec.type_name,
                        has_r: spec.r_type_id.is_some(),
                        r_type_id: spec.r_type_id.unwrap_or_else(TypeId::of::<()>),
                        r_name: spec.r_name.unwrap_or("()"),
                    },
                );
            }
        }

        // 监听器表（写锁）：tombstone 复用 or push（表 ≤ 峰值活跃订阅）。
        let mut listeners = self.listeners.write().unwrap();
        let vec = listeners.entry(name).or_default();
        if let Some(slot) = vec.iter_mut().find(|e| !e.alive()) {
            *slot = entry;
        } else {
            vec.push(entry);
        }
    }

    /// emit 派发（草案 §2.2）：注册序逐个观察；panic 传播（panic=bug）。
    /// 空听众集 = no-op（E-2）。release-then-invoke：快照后释放锁再调用。
    pub fn emit<P: Event>(&self, payload: &P::Payload) {
        let name = Symbol::intern(P::SYMBOL);
        let any = payload as &dyn Any;
        let snap: Vec<EmitAnyFn> = {
            let listeners = self.listeners.read().unwrap();
            match listeners.get(&name) {
                None => Vec::new(),
                Some(vec) => vec
                    .iter()
                    .filter(|e| e.alive())
                    .filter_map(|e| match e {
                        ListenerEntry::Emit { f, .. } => Some(Arc::clone(f)),
                        _ => None,
                    })
                    .collect(),
            }
        };
        for f in snap {
            f(any);
        }
    }

    /// waterfall 派发（草案 §2.2）：A→B→terminal 序；around（next 返回后
    /// 处理）；短路 = 不调 next；最内层 next = 分发方提供的 terminal。
    /// 空听众集 = 仅 terminal（E-2）。
    pub fn waterfall<P: Event>(
        &self,
        payload: &mut P::Payload,
        terminal: impl Fn(&mut P::Payload),
    ) {
        let name = Symbol::intern(P::SYMBOL);
        let snap: Vec<(Arc<AtomicBool>, WaterfallAnyFn)> = {
            let listeners = self.listeners.read().unwrap();
            match listeners.get(&name) {
                None => Vec::new(),
                Some(vec) => vec
                    .iter()
                    .filter(|e| e.alive())
                    .filter_map(|e| match e {
                        ListenerEntry::Waterfall { alive, f, .. } => {
                            Some((Arc::clone(alive), Arc::clone(f)))
                        }
                        _ => None,
                    })
                    .collect(),
            }
        };
        let terminal: &dyn Fn(&mut P::Payload) = &terminal;
        let t: &dyn Fn(&mut dyn Any) = &|any: &mut dyn Any| {
            (terminal)(
                any.downcast_mut::<P::Payload>()
                    .expect("terminal 载荷类型已核验"),
            )
        };
        let any: &mut dyn Any = payload;
        waterfall_link(&snap, 0, any, t);
    }

    /// serial 派发（草案 §2.2）：注册序收集全部 `R`；空听众 = 空 vec（E-2）。
    /// 派发前 R 一致性校验（m-3'）。
    pub fn serial<P: Event, R>(&self, payload: &P::Payload) -> Vec<R>
    where
        R: Send + Sync + 'static,
    {
        let name = Symbol::intern(P::SYMBOL);
        self.check_dispatch_r::<R>(name, Mode::Serial);
        let snap = self.snapshot_reply(name, Mode::Serial);
        let any = payload as &dyn Any;
        snap.into_iter()
            .filter(|(alive, _)| alive.load(Ordering::SeqCst))
            .map(|(_, f)| *f(any).downcast::<R>().expect("派发 R 与订阅写定的 R 一致"))
            .collect()
    }

    /// bail 派发（草案 §2.2）：逐个询问，首个 `Some(r)` 即停得 `Some(r)`；
    /// 全 `None` → `None`；空听众 = `None`（E-2）。派发前 R 一致性校验。
    pub fn bail<P: Event, R>(&self, payload: &P::Payload) -> Option<R>
    where
        R: Send + Sync + 'static,
    {
        let name = Symbol::intern(P::SYMBOL);
        self.check_dispatch_r::<R>(name, Mode::Bail);
        let snap = self.snapshot_reply(name, Mode::Bail);
        let any = payload as &dyn Any;
        for (alive, f) in snap {
            if !alive.load(Ordering::SeqCst) {
                continue;
            }
            let option: Option<R> = *f(any)
                .downcast::<Option<R>>()
                .expect("派发 R 与订阅写定的 R 一致");
            if let Some(r) = option {
                return Some(r);
            }
        }
        None
    }

    /// 派发侧 R 一致性校验（m-3'）：`serial<P,R>`/`bail<P,R>` 对照 modes 表
    /// 订阅写定的 R；无订阅（None）→ 视为空集（E-2），不 panic。
    fn check_dispatch_r<R: Send + Sync + 'static>(&self, name: Symbol, mode: Mode) {
        let modes = self.modes.read().unwrap();
        if let Some(rec) = modes.get(&(name, mode))
            && rec.has_r
            && rec.r_type_id != TypeId::of::<R>()
        {
            panic!(
                "派发 R 与订阅写定的 R 不符（事件 `{name}` 模式 {mode:?}：订阅 R = `{}`，派发 R = `{}`）",
                rec.r_name,
                std::any::type_name::<R>()
            );
        }
    }

    /// reply 监听器快照（Serial/Bail 按 mode 过滤；release-then-invoke）。
    fn snapshot_reply(&self, name: Symbol, mode: Mode) -> Vec<(Arc<AtomicBool>, ReplyAnyFn)> {
        let listeners = self.listeners.read().unwrap();
        match listeners.get(&name) {
            None => Vec::new(),
            Some(vec) => vec
                .iter()
                .filter(|e| e.alive() && e.mode() == mode)
                .filter_map(|e| match e {
                    ListenerEntry::Serial { alive, f, .. }
                    | ListenerEntry::Bail { alive, f, .. } => {
                        Some((Arc::clone(alive), Arc::clone(f)))
                    }
                    _ => None,
                })
                .collect(),
        }
    }
}

/// waterfall next 链（递归展开下游），最内层 = terminal。
fn waterfall_link(
    fs: &[(Arc<AtomicBool>, WaterfallAnyFn)],
    i: usize,
    p: &mut dyn Any,
    terminal: &dyn Fn(&mut dyn Any),
) {
    if i >= fs.len() {
        (terminal)(p);
        return;
    }
    // E-1（Minor-1 落地）：派发中途被退订的 waterfall 监听器跳过（等效
    // 未订阅），但**不短路**——下游与 terminal 照常（退订 ≠ 拒绝）。
    if !fs[i].0.load(Ordering::SeqCst) {
        waterfall_link(fs, i + 1, p, terminal);
        return;
    }
    let f = &fs[i].1;
    let next: &dyn Fn(&mut dyn Any) = &|x: &mut dyn Any| waterfall_link(fs, i + 1, x, terminal);
    (f)(p, next);
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// 幂等 disposer：置 alive 失活（自研 armed 同款语义，重复 dispose 无害）。
fn disposer(alive: Arc<AtomicBool>) -> Disposer {
    Box::new(move || {
        alive.store(false, Ordering::SeqCst);
    })
}

/// 总线服务键（草案 §3.1，realm 键控：store 内全局服务，C-1 Arc 惯例）。
pub struct EventsKey;

impl Key for EventsKey {
    type Value = Arc<EventBus>;

    const SYMBOL: &'static str = "events";
}

/// 根组件：绑定总线（App 在根 ctx 挂载，或 bundle 层 insert；草案 §3.1）。
///
/// 只用 core 原生 `once`（不引 cordis-native——评审 m-2：保住「只依赖
/// cordis-core」的零依赖定位）。
pub struct EventsProvider;

impl Component for EventsProvider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }

    fn provide(&self) -> KeySet {
        [Symbol::intern(EventsKey::SYMBOL)].into_iter().collect()
    }

    fn apply(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        // 绑定步：核心原生 once；绑定逆（unbind）在 ctx 累加器中登记。
        Box::new(cordis_core::once(Box::new(move || {
            ctx.set::<EventsKey>(Arc::new(EventBus::new()))
                .expect("绑定 events")
        }))) as Box<dyn EffectIter>
    }
}
