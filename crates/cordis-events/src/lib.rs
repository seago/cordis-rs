//! cordis-events 层（草案 v0.3.1 冻结，Phase 1 第一交付物）。
//!
//! 定位：cordis-rs 架构决策 C 的事件系统层。纯 sync、零依赖（仅依赖
//! `cordis-core`）——类型化事件（`Key` 的镜像）、四种 sync 派发
//! （emit / waterfall / serial / bail；`parallel` 推迟到 async 层，
//! 见草案 §0/§5）、订阅即效应（订阅经 `ctx.effect` 注册，fiber 卸载
//! 自动退订）。
//!
//! **核心义务（草案 §0）**：事件总线监听器闭包须带 `Send + Sync + 'static`
//! 上界——总线是 store 内全局服务，`EventsKey::Value = Arc<EventBus>` 必须
//! 满足 core `Key::Value: Send+Sync`。代价：监听器**不得捕获 `Rc`**
//! （Context/Fiber 等）——需要服务时经 `Arc` 捕获（C-1 惯例）。捕获 `Rc`
//! 的线程私有总线不属本层（另设非 Send+Sync 变体，草案 O-6'）。
//!
//! 依据：`docs/cordis-events-protocol-draft.md` v0.3.1（冻结）；
//! 执行计划 `docs/cordis-events-PHASE1-PLAN.md`（含里程碑间独立审查硬门禁）。

#![deny(missing_docs)]

use cordis_core::{Component, Context, EffectIter, Key, KeySet, Symbol};
use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

/// 类型化事件（草案 §1：与 core `Key` 完全同构——身份 = [SYMBOL]，载荷 =
/// 关联类型；命名对齐 `Key::SYMBOL`，内部经 [`Symbol::intern`] 与 Key 同
/// 驻留）。
///
/// 义务：两个事件类型**不得**声明相同的 [`Event::SYMBOL`]（与 Key 的符号
/// 纪律同款；违反在订阅点 panic = bug）。
pub trait Event: 'static {
    /// 载荷类型（值传递/可变借用，见 §2 各派发模式）。
    type Payload: 'static;

    /// 事件名（驻留为 core `Symbol`）。
    const SYMBOL: &'static str;
}

// ── 订阅/派发核心（草案 §2.1，M1.2 起落地行为；M1.1 先固化类型）──────

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

/// 事件总线（草案 §3.2）。M1.1 骨架：空结构占位，M1.2 填充内部同步化
/// 结构（`RwLock` 单表 + 监听器表，闭包带 `Send+Sync` 上界）。
pub struct EventBus {
    // M1.2：modes 单表 + listeners 表（草案 §3.2）
    _private: (),
}

impl EventBus {
    /// 新建空总线（草案 §3.1 `EventsProvider` 绑定入口）。
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 示例事件（草案 §1 典型声明对照）。
    struct ToolPreExecute;
    impl Event for ToolPreExecute {
        type Payload = String;
        const SYMBOL: &'static str = "tools/pre-execute";
    }

    #[test]
    fn skeleton_constructs_bus_and_provider() {
        // 总线可构造（Send+Sync 编译断言在验收 #9，M1.5 落地）。
        let bus = EventBus::new();
        let _ = bus;

        // EventsKey 义务与符号驻留。
        assert_eq!(EventsKey::SYMBOL, "events");
        assert_eq!(ToolPreExecute::SYMBOL, "tools/pre-execute");

        // EventsProvider d/p 声明正确（apply 不在此调用）。
        let provider = EventsProvider;
        assert!(provider.inject().is_empty());
        assert!(
            provider
                .provide()
                .contains(Symbol::intern(EventsKey::SYMBOL))
        );
    }
}
