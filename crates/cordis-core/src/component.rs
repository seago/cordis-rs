//! 组件（论文 Def 43 的 `ℭΓ = (d, p, e)`）。

use std::any::Any;
use std::rc::Rc;

use crate::context::Context;
use crate::effect::EffectIter;
use crate::keyset::KeySet;

/// 组件 `(d, p, e)`（Def 43）。
///
/// - `d`：共效应规格（[`Component::inject`]）——从环境声明的依赖；
/// - `p`：供给（[`Component::provide`]）——可提供的键；效应函数不得写入
///   `p` 之外的键（Def 43/48 纪律，[`Context::set`] 执行期检查）；
/// - `e`：效应函数（[`Component::apply`]）——在 `ctx` 上执行组件效应
///   （经 [`Context::set`]/[`Context::effect`]），返回效应迭代器。
///
/// 单线程宿主（ADR-0002），不要求 `Send + Sync`。
pub trait Component: 'static {
    /// 共效应规格 `d`。
    fn inject(&self) -> KeySet;

    /// 供给 `p`。
    fn provide(&self) -> KeySet;

    /// 效应函数 `e(config)`（Algorithm 4 第 9 行）：在 `ctx` 上执行效应。
    fn apply(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter>;
}
