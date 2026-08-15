//! Cordis 热模块替换引擎（论文 §5.2.2，M2 交付）。
//!
//! Algorithm 8/9/10 直译：模块分类不动点 → 过期条目检测 → 事务性重载
//! （wasm：换实例；native：dlopen 换库；失败回滚）。
//!
//! 进度：PR #1 骨架。

#![deny(missing_docs)]
