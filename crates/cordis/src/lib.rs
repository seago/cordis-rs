//! Cordis 门面 crate（PLAN §4.1）：统一 re-export 全部公开 API。
//!
//! `#[component]` 宏生成的代码引用 `::cordis::` 路径——使用宏的 crate
//! 依赖本门面即可。

#![deny(missing_docs)]

pub use cordis_core::{
    Classification, Component, Context, Disposer, EffectIter, Fiber, FiberError, FiberId,
    FiberState, InterceptMeta, Key, KeySet, Reactor, RegistryError, Runtime, Step, Store,
    StoreError, Symbol, View, classify, once,
};
pub use cordis_macro::component;
