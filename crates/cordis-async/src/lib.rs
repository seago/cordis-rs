//! cordis-async 层（草案 v1.4，Phase 0 Mind0.1 骨架）。
//!
//! 定位：sync `cordis-core` 零语义改动之上的一等 async 层——异步效应
//! 协议（AsyncEffectIter）、取消/错误通道、可 await 卸载编排（两阶段
//! 卸载 + settle + 代次）、Remote 桥。本模块为协议类型占位；驱动引擎
//!（`drive`/I-1/I-2）于 M0.2 实现。
//!
//! 依据：`docs/cordis-async-protocol-draft.md` v1.4（冻结）；
//! 执行计划 `docs/cordis-async-PHASE0-PLAN.md`（含里程碑间独立审查硬门禁）。

#![deny(missing_docs)]

use std::future::Future;
use std::pin::Pin;

/// 组合线程本地 future（非 Send；仅在本线程 LocalSet 内 await）。
pub type LocalBoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static>>;

/// 异步逆：撤销一步 async 效应（可 await；对应 core
/// [`cordis_core::effect::Disposer`] 的
/// `FnOnce()` 形态）。
pub type AsyncDisposer = Box<dyn FnOnce() -> LocalBoxFuture<()> + 'static>;

/// 异步效应迭代器：宿主驱动，每步 await 后产出逆或失败。
///
/// 与 core 同款纪律：迭代必须**有限步终止**（订阅型长驻行为用注册器
/// 模式，见草案 §6）。
pub trait AsyncEffectIter: 'static {
    /// 产下一步（可 await）；调用方保证步界 guard 检查。
    fn next(&mut self) -> LocalBoxFuture<AsyncStep>;
}

/// 单步结果（core [`cordis_core::effect::Step`] 的 async 等价物）。
pub enum AsyncStep {
    /// 产出逆并继续（core `Step::Yielded` 的 async 版）。
    Yielded(AsyncDisposer),
    /// 产出逆并终止（core `Step::Finished` 的 async 版）。
    Finished(AsyncDisposer),
    /// 组件运行时失败（core L-Raise 的 async 等价物；**不是** panic 通道）。
    Failed(AsyncFiberError),
}

/// 组件失败载荷（对应 core [`cordis_core::fiber::FiberError`]；async 世界以值传播，不经 panic）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AsyncFiberError(String);

impl AsyncFiberError {
    /// 构造失败载荷。
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// 失败消息。
    pub fn message(&self) -> &str {
        &self.0
    }
}

/// 驱动引擎（草案 §1；Algorithm 1 的 async 转写）。
///
/// 逐步 await 迭代器：guard 在每个**步界**检查（§4.3.2 步界中断同语义）；
/// `Failed` → 先 LIFO await 恢复已完成步骤再报失败；正常终止 → 折叠
/// 复合逆（以应用逆序 await 各步逆，I-1）。
///
/// guard 为 `false` 时的退场：在途步完成、其逆照常入账并参与复合逆
///（I-2）——不中断在途的 `next()` 挂起。
pub async fn drive(
    mut iter: Box<dyn AsyncEffectIter>,
    guard: impl Fn() -> bool,
) -> Result<AsyncDisposer, AsyncFiberError> {
    let mut acc: Vec<AsyncDisposer> = Vec::new();
    loop {
        if !guard() {
            break;
        }
        match iter.next().await {
            AsyncStep::Yielded(d) => acc.push(d),
            AsyncStep::Finished(d) => {
                acc.push(d);
                break;
            }
            AsyncStep::Failed(e) => {
                // LIFO 恢复已完成步骤，再上报失败。
                for d in acc.into_iter().rev() {
                    d().await;
                }
                return Err(e);
            }
        }
    }
    Ok(Box::new(move || {
        Box::pin(async move {
            for d in acc.into_iter().rev() {
                d().await;
            }
        }) as LocalBoxFuture<()>
    }) as AsyncDisposer)
}
