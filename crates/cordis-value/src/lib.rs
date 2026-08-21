//! Cordis 统一跨边界值类型（产品验证线 P-2）。
//!
//! 论文/草案的分层要求：原生组件与 wasm 组件互通的值类型**不隶属任一后端**
//! ——独立 crate（零第三方、零 `cordis-core` 依赖），`cordis-wasm` 与原生
//! 组件都依赖本 crate（消除 THEORY-MAP PR#13 的"原生→wasm 依赖方向"）。
//!
//! 形态与 wit `value` 变体同构（`flag/count/offset/text/blob`）——值语义
//! 与既有 wasm 桥一致（P-2 保证零变化）。

#![deny(missing_docs)]

/// 统一跨边界值（wit `value` 的 Rust 形态）。
///
/// `Send + Sync`：可跨线程（worker/镜像同步）传递；`Clone + PartialEq`：
/// 镜像比较/断言用。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// 布尔（`flag`）。
    Flag(bool),
    /// 无符号计数（`count`）。
    Count(u64),
    /// 有符号偏移（`offset`）。
    Offset(i64),
    /// 文本（`text`）。
    Text(String),
    /// 字节串（`blob`）。
    Blob(Vec<u8>),
}
