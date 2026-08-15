//! Cordis 声明式组件加载器（论文 §5.2.1，M0 后半 / M2 交付）。
//!
//! 配置树（Entry：id/url/isolate/intercept/config/disabled，Def 74）、
//! 按字段最小扰动协调、托管 realm（local/global + delimiter 标签）。
//!
//! 进度：PR #1 骨架。

#![deny(missing_docs)]
