//! 类型化共效应键（论文 Def 24 的 `𝒱 k` 类型族）。

/// 类型化共效应键。
///
/// `TypeId` 提供进程内的键身份，[`Key::SYMBOL`] 提供跨边界（wasm）与
/// 调试用的符号身份（ADR-0001）。值类型为 [`Key::Value`]。
///
/// 义务：两个键类型**不得**声明相同的 [`Key::SYMBOL`]（与 TS 版 Cordis
/// 相同的作者义务；违反时在访问点报 [`crate::store::StoreError::TypeMismatch`]）。
pub trait Key: 'static + Send + Sync {
    /// 该键对应的值类型（论文 `𝒱 k`）。
    type Value: Send + Sync + 'static;

    /// 跨边界符号名（驻留为 [`crate::symbol::Symbol`]）。
    const SYMBOL: &'static str;
}
