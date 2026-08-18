//! M0.2（草案 §1）：`drive` 引擎 + 不变量 I-1（LIFO 逆）/ I-2（步界 guard）。
//! M0.3（草案 §3）：两阶段卸载——I-3（依赖者 async 逆先 settle）+ drain 重入。
//!
//! - I-1：drive 折叠的复合逆以**应用逆序** await 各步逆（Thm 16 的 async 版）。
//! - I-2：guard 仅在步界检查；await 挂起期间 guard 翻假不中断在途步——
//!   在途步完成后其逆照常入账并参与恢复。
//! - I-3：async 尾巴按依赖序收尾（core 级联入队序 + settle FIFO）。
//! - drain 重入：收尾逆可注册新 async 效应（下一代队列排空）；自再生死
//!   循环触发 64 轮守卫 panic。

use cordis_async::{AsyncDisposer, AsyncEffectIter, AsyncFiberError, AsyncStep, drive};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// 共享顺序记录（断言 LIFO 序）。
type Log = Rc<RefCell<Vec<String>>>;

/// 第 `label` 步的逆：执行时记录 `"rev:{label}"`。
fn step_disposer(log: &Log, label: &str) -> AsyncDisposer {
    let log = Rc::clone(log);
    let label = label.to_string();
    Box::new(move || {
        let log = Rc::clone(&log);
        let label = label.clone();
        Box::pin(async move {
            log.borrow_mut().push(format!("rev:{label}"));
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>>
    })
}

/// 静态序列迭代器：给定标签与模式（全部 Yielded 后 Finished；或含 Failed）。
struct SeqIter {
    labels: Vec<String>,
    #[allow(dead_code)] // M0.4 失败通道测试使用
    failed_at: Option<usize>,
    cursor: usize,
    log: Log,
}

impl SeqIter {
    /// 全部 Yielded、末尾 Finished 的 N 步序列。
    fn finished(labels: &[&str], log: &Log) -> Self {
        Self {
            labels: labels.iter().map(|s| s.to_string()).collect(),
            failed_at: None,
            cursor: 0,
            log: Rc::clone(log),
        }
    }
}

impl AsyncEffectIter for SeqIter {
    fn next(&mut self) -> cordis_async::LocalBoxFuture<AsyncStep> {
        let label = self.labels[self.cursor].clone();
        self.cursor += 1;
        let log = Rc::clone(&self.log);
        let is_last = self.cursor >= self.labels.len();
        let fails_here = self.failed_at == Some(self.cursor - 1);
        Box::pin(async move {
            // 模拟"第 N+1 步"的异步性（yield 一拍）。
            tokio::task::yield_now().await;
            if fails_here {
                return AsyncStep::Failed(AsyncFiberError::new(format!("fail:{label}")));
            }
            let d = step_disposer(&log, &label);
            if is_last {
                AsyncStep::Finished(d)
            } else {
                AsyncStep::Yielded(d)
            }
        })
    }
}

/// I-1：三步效应 A→B→C，复合逆按应用逆序（C→B→A）await。
#[tokio::test]
async fn i1_composite_disposer_runs_lifo() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let iter = SeqIter::finished(&["a", "b", "c"], &log);
    let disposer = drive(Box::new(iter), || true).await.expect("正常完成");
    assert!(log.borrow().is_empty(), "drive 期不执行逆");

    disposer().await;
    assert_eq!(
        *log.borrow(),
        vec!["rev:c", "rev:b", "rev:a"],
        "I-1：应用逆序撤销"
    );
}

/// I-2：长 await 中途 guard 翻假 → 在途步完成、逆入账；复合逆含全部
/// 已完成步（含在途完成的那步）。
#[tokio::test]
async fn i2_guard_false_at_step_boundary_keeps_inflight_step() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let iter = SeqIter::finished(&["a", "b", "c"], &log);
    // guard：第 3 次检查（c 步界前）翻假——drive 在 c 前退场，
    // c 不应被触及、b 的逆照常入账。
    let checks = Rc::new(Cell::new(0));
    let check_count = Rc::clone(&checks);
    let guarded = drive(Box::new(iter), move || {
        let c = check_count.get() + 1;
        check_count.set(c);
        c < 3
    })
    .await
    .expect("guard 退场 = 正常完成（非失败）");
    assert_eq!(
        checks.get(),
        3,
        "guard 检查 3 次（a 前、b 前、c 前翻假退场）"
    );

    guarded().await;
    assert_eq!(
        *log.borrow(),
        vec!["rev:b", "rev:a"],
        "I-2：在途步 b 完成入账，后续 c 未触及"
    );
}

/// I-2 变体：guard 从开始就为假 → 空复合逆（零步）。
#[tokio::test]
async fn i2_guard_false_immediately_yields_empty_composite() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let iter = SeqIter::finished(&["a", "b"], &log);
    let disposer = drive(Box::new(iter), || false).await.expect("立即退场");
    disposer().await;
    assert!(log.borrow().is_empty(), "零步：无逆执行");
}

/// I-2 真实在途用例（REVIEW-91254a9 nit-2）：第 2 步 `next()` 返回一个
/// **挂起 future**（await 外部信号）；drive 停在步挂起期间，guard 翻假——
/// 在途步**不受中断**，待信号完成后退场、其逆照常入账（复合逆含在途步）。
#[tokio::test(flavor = "current_thread")]
async fn i2_guard_flips_while_inflight_step_pending() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let guard = Rc::new(Cell::new(true));

    // 挂起迭代器：a 步立即 Yielded；b 步 next() 返回 await oneshot 的 future。
    struct PendingIter {
        sent_a: bool,
        rx: Option<tokio::sync::oneshot::Receiver<()>>,
        log: Log,
    }
    impl AsyncEffectIter for PendingIter {
        fn next(&mut self) -> cordis_async::LocalBoxFuture<AsyncStep> {
            if !self.sent_a {
                self.sent_a = true;
                let log = Rc::clone(&self.log);
                return Box::pin(async move {
                    // 触一下调度器让 drive 外层循环可推进（模拟第 1 步异步性）。
                    tokio::task::yield_now().await;
                    AsyncStep::Yielded(step_disposer(&log, "a"))
                });
            }
            let rx = self.rx.take().expect("b 的 oneshot 接收端");
            let log = Rc::clone(&self.log);
            Box::pin(async move {
                rx.await.expect("信号到达");
                AsyncStep::Finished(step_disposer(&log, "b"))
            })
        }
    }

    let (tx, rx) = tokio::sync::oneshot::channel();
    let iter = PendingIter {
        sent_a: false,
        rx: Some(rx),
        log: Rc::clone(&log),
    };
    let guard_cell = Rc::clone(&guard);
    let local = tokio::task::LocalSet::new();
    let handle =
        local.spawn_local(async move { drive(Box::new(iter), move || guard_cell.get()).await });

    // 推进 drive 到 b 挂起（多轮 yield 让 a 完成、b 进入 rx.await）。
    local
        .run_until(async move {
            for _ in 0..50 {
                tokio::task::yield_now().await;
                if tx.is_closed() {
                    break;
                }
            }
            // b 仍在挂起：此刻 guard 翻假——在途步不应被中断。
            guard.set(false);
            tx.send(()).expect("完成 b");
            let disposer = handle
                .await
                .expect("任务本身未 panic")
                .expect("drive 正常退场（guard 步界）");
            disposer().await;
            assert_eq!(
                *log.borrow(),
                vec!["rev:b", "rev:a"],
                "I-2 在途：b 挂起期间 guard 翻假不中断其完成；逆照常入账"
            );
        })
        .await;
}

// ── M0.3（草案 §3）：两阶段卸载——I-3 + drain 重入 ─────────────────────

mod m03 {
    use super::Log;
    use cordis_async::{
        AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncRuntime, AsyncStep,
        LocalBoxFuture,
    };
    use cordis_core::{
        Component, Context, Disposer, EffectIter, FiberState, Key, KeySet, Symbol, once,
    };
    use std::any::Any;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 依赖键：I-3 用例中 provider 提供、consumer 注入。
    struct KeyDep;
    impl Key for KeyDep {
        type Value = String;
        const SYMBOL: &'static str = "m0.3.dep";
    }

    /// sync 壳组件：只贡献 d/p（AsyncRegistrar 不调用内层 apply）。
    pub(super) struct Shell {
        inject: Vec<&'static str>,
        provide: Vec<&'static str>,
    }

    impl Shell {
        fn inject_only(keys: &[&'static str]) -> Self {
            Self {
                inject: keys.to_vec(),
                provide: Vec::new(),
            }
        }
        fn provide_only(keys: &[&'static str]) -> Self {
            Self {
                inject: Vec::new(),
                provide: keys.to_vec(),
            }
        }
        pub(super) fn empty() -> Self {
            Self {
                inject: Vec::new(),
                provide: Vec::new(),
            }
        }
    }

    impl Component for Shell {
        fn inject(&self) -> KeySet {
            let mut ks = KeySet::new();
            for k in &self.inject {
                ks.insert(Symbol::intern(k));
            }
            ks
        }
        fn provide(&self) -> KeySet {
            let mut ks = KeySet::new();
            for k in &self.provide {
                ks.insert(Symbol::intern(k));
            }
            ks
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
            // AsyncRegistrar 只用 d/p；内层 apply 不应被调用（防御性空效应）。
            Box::new(once(Box::new(|| Box::new(|| {}) as Disposer)))
        }
    }

    /// 异步逆：yield 一拍后记录 `{label}:inverse`（settle 的 async 逆路径）。
    pub(super) fn async_inverse(log: &Log, label: &'static str) -> AsyncDisposer {
        let log = Rc::clone(log);
        Box::new(move || {
            let log = Rc::clone(&log);
            Box::pin(async move {
                tokio::task::yield_now().await;
                log.borrow_mut().push(format!("{label}:inverse"));
            }) as LocalBoxFuture<()>
        }) as AsyncDisposer
    }

    /// 单步行为：`run` 时记 `{label}:run`（可选：顺带绑定 KeyDep），
    /// 逆记 `{label}:inverse`（真实 async 逆）。m04 复用故 pub(super)。
    pub(super) struct OneShotBehavior {
        pub(super) label: &'static str,
        pub(super) log: Log,
        pub(super) provide_dep: bool,
    }

    impl AsyncBehavior for OneShotBehavior {
        fn apply_async(&self, cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
            Box::new(OneShotIter {
                cx,
                label: self.label,
                log: Rc::clone(&self.log),
                provide_dep: self.provide_dep,
                done: false,
            })
        }
    }

    struct OneShotIter {
        cx: AsyncCx,
        label: &'static str,
        log: Log,
        provide_dep: bool,
        done: bool,
    }

    impl AsyncEffectIter for OneShotIter {
        fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
            assert!(!self.done, "单步迭代器至多一步");
            self.done = true;
            let cx = self.cx.clone();
            let label = self.label;
            let log = Rc::clone(&self.log);
            let provide_dep = self.provide_dep;
            Box::pin(async move {
                if provide_dep {
                    // 绑定逆已入 fiber ctx 累加器（core 卸载时经 dispose_all
                    // 执行）；返回的 disposer 句柄故意丢弃（与 core register
                    // 同款意图）。
                    drop(
                        cx.set::<KeyDep>(String::from("v"))
                            .expect("provider 绑定依赖键"),
                    );
                }
                log.borrow_mut().push(format!("{label}:run"));
                AsyncStep::Finished(async_inverse(&log, label))
            })
        }
    }

    /// I-3（草案 §3.2）：async 提供者 + async 消费者；退役提供者 →
    /// core 级联（依赖者先撤，Thm 63）→ 消费者注册器逆先入队 → settle
    /// FIFO 排空：消费者的 async 逆先 settle、提供者后 settle（日志序直证）。
    #[tokio::test]
    async fn i3_dependent_async_inverse_settles_first() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let provider = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::provide_only(&["m0.3.dep"])),
                        OneShotBehavior {
                            label: "provider",
                            log: Rc::clone(&log),
                            provide_dep: true,
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("provider 挂载");
                let consumer = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::inject_only(&["m0.3.dep"])),
                        OneShotBehavior {
                            label: "consumer",
                            log: Rc::clone(&log),
                            provide_dep: false,
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("consumer 挂载");

                // 等 provider 绑定落地 → consumer 激活 → 两个 drive 完成填账。
                //
                // 决定论约定（REVIEW-83c254a nit-1）：`OneShotIter::next()`
                // 内日志与步完成同步、**无中途 await**——drive 一次 poll 即
                // 自启动到完成，故固定轮数 yield 头寸充裕、无 flaky 风险。
                // 若日后在 `next()` 内引入 await/多步，须同步改造成可 await
                // 的就绪条件（如 Notify），勿依赖本自旋基数。
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if matches!(*consumer.state(), FiberState::Active { .. }) {
                        break;
                    }
                }
                assert!(
                    matches!(*consumer.state(), FiberState::Active { .. }),
                    "依赖满足后 consumer 激活"
                );
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                assert_eq!(
                    *log.borrow(),
                    vec!["provider:run".to_string(), "consumer:run".to_string()],
                    "两个 drive 均已完成（slot 填账）"
                );

                // 退役提供者：core 级联 consumer 先卸载（逆先入队），provider 后卸载。
                provider.retire();
                rt.settle().await;

                assert_eq!(
                    *log.borrow(),
                    vec![
                        "provider:run".to_string(),
                        "consumer:run".to_string(),
                        "consumer:inverse".to_string(),
                        "provider:inverse".to_string(),
                    ],
                    "I-3：依赖者的 async 逆先 settle，提供者的后 settle"
                );
            })
            .await;
    }

    /// drain 重入（§3.4）：settle 期间某尾巴的 async 逆注册**新的** async
    /// 效应（合法收尾逻辑）→ 入下一代队列 → settle 循环排空至队列空。
    #[tokio::test]
    async fn drain_reentry_next_generation_is_drained() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                struct ReentryBehavior {
                    rt: Rc<AsyncRuntime>,
                    root: Rc<Context>,
                    log: Log,
                }
                impl AsyncBehavior for ReentryBehavior {
                    fn apply_async(
                        &self,
                        _cx: AsyncCx,
                        _config: &dyn Any,
                    ) -> Box<dyn AsyncEffectIter> {
                        Box::new(ReentryIter {
                            rt: Rc::clone(&self.rt),
                            root: Rc::clone(&self.root),
                            log: Rc::clone(&self.log),
                            done: false,
                        })
                    }
                }
                struct ReentryIter {
                    rt: Rc<AsyncRuntime>,
                    root: Rc<Context>,
                    log: Log,
                    done: bool,
                }
                impl AsyncEffectIter for ReentryIter {
                    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
                        assert!(!self.done, "单步");
                        self.done = true;
                        let rt = Rc::clone(&self.rt);
                        let root = Rc::clone(&self.root);
                        let log = Rc::clone(&self.log);
                        Box::pin(async move {
                            log.borrow_mut().push("A:run".into());
                            AsyncStep::Finished(Box::new(move || {
                                let rt = Rc::clone(&rt);
                                let root = Rc::clone(&root);
                                let log = Rc::clone(&log);
                                Box::pin(async move {
                                    // 收尾中注册新 async 效应（合法收尾逻辑）。
                                    log.borrow_mut().push("A:inverse".into());
                                    let b = rt
                                        .use_component(
                                            &root,
                                            Rc::new(Shell::empty()),
                                            OneShotBehavior {
                                                label: "B",
                                                log: Rc::clone(&log),
                                                provide_dep: false,
                                            },
                                            Rc::new(()) as Rc<dyn Any>,
                                        )
                                        .expect("收尾中挂载 B");
                                    // 让 B 的 drive 完成（slot 填账）再退役入队。
                                    tokio::task::yield_now().await;
                                    b.retire();
                                }) as LocalBoxFuture<()>
                            }) as AsyncDisposer)
                        })
                    }
                }

                let a = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        ReentryBehavior {
                            rt: Rc::clone(&rt),
                            root: Rc::clone(&ctx),
                            log: Rc::clone(&log),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载 A");
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                a.retire();
                rt.settle().await;

                assert_eq!(
                    *log.borrow(),
                    vec![
                        "A:run".to_string(),
                        "A:inverse".to_string(),
                        "B:run".to_string(),
                        "B:inverse".to_string(),
                    ],
                    "drain 重入：新一代队列被排空（A 逆中注册的 B 完整收尾）"
                );
            })
            .await;
    }

    /// drain 重入死锁守卫（§3.4）：收尾逆持续注册"又一个自身"且不收敛 →
    /// settle 超过 64 轮 → panic（宿主 bug 诊断）。
    #[tokio::test]
    #[should_panic(expected = "drain 自再生死循环守卫")]
    async fn drain_self_regeneration_triggers_guard_panic() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                struct SelfRegenBehavior {
                    rt: Rc<AsyncRuntime>,
                    root: Rc<Context>,
                    log: Log,
                }
                impl AsyncBehavior for SelfRegenBehavior {
                    fn apply_async(
                        &self,
                        _cx: AsyncCx,
                        _config: &dyn Any,
                    ) -> Box<dyn AsyncEffectIter> {
                        Box::new(SelfRegenIter {
                            rt: Rc::clone(&self.rt),
                            root: Rc::clone(&self.root),
                            log: Rc::clone(&self.log),
                            done: false,
                        })
                    }
                }
                struct SelfRegenIter {
                    rt: Rc<AsyncRuntime>,
                    root: Rc<Context>,
                    log: Log,
                    done: bool,
                }
                impl AsyncEffectIter for SelfRegenIter {
                    fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
                        assert!(!self.done, "单步");
                        self.done = true;
                        let rt = Rc::clone(&self.rt);
                        let root = Rc::clone(&self.root);
                        let log = Rc::clone(&self.log);
                        Box::pin(async move {
                            log.borrow_mut().push("self:run".into());
                            AsyncStep::Finished(Box::new(move || {
                                let rt = Rc::clone(&rt);
                                let root = Rc::clone(&root);
                                let log = Rc::clone(&log);
                                Box::pin(async move {
                                    // 收尾逆再挂一个自身 → 无限链 → 守卫 panic。
                                    let next = rt
                                        .use_component(
                                            &root,
                                            Rc::new(Shell::empty()),
                                            SelfRegenBehavior {
                                                rt: Rc::clone(&rt),
                                                root: Rc::clone(&root),
                                                log: Rc::clone(&log),
                                            },
                                            Rc::new(()) as Rc<dyn Any>,
                                        )
                                        .expect("收尾中再挂载");
                                    tokio::task::yield_now().await;
                                    next.retire();
                                }) as LocalBoxFuture<()>
                            }) as AsyncDisposer)
                        })
                    }
                }

                let first = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        SelfRegenBehavior {
                            rt: Rc::clone(&rt),
                            root: Rc::clone(&ctx),
                            log: Rc::clone(&log),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载首个");
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                first.retire();
                rt.settle().await; // 每轮逆再挂一个 → 第 65 轮守卫 panic
            })
            .await;
    }
}

// ── M0.4（草案 §3.3）：失败通道——I-4 + 关停（C-7 / 测试 11）───────────

mod m04 {
    use super::Log;
    use super::m03::{OneShotBehavior, Shell, async_inverse};
    use cordis_async::{
        AsyncBehavior, AsyncCx, AsyncEffectIter, AsyncFiberError, AsyncFiberState, AsyncRuntime,
        AsyncStep, LocalBoxFuture,
    };
    use cordis_core::Context;
    use cordis_loader::{Entry, Loader};
    use std::any::Any;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// 首次激活失败、复活后成功的单步行为（attempts 跨激活共享）。
    /// m05 复用故 pub(super)。
    pub(super) struct FailOnceBehavior {
        pub(super) log: Log,
        pub(super) attempts: Rc<Cell<u32>>,
    }

    impl AsyncBehavior for FailOnceBehavior {
        fn apply_async(&self, _cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
            Box::new(FailOnceIter {
                log: Rc::clone(&self.log),
                attempts: Rc::clone(&self.attempts),
                done: false,
            })
        }
    }

    struct FailOnceIter {
        log: Log,
        attempts: Rc<Cell<u32>>,
        done: bool,
    }

    impl AsyncEffectIter for FailOnceIter {
        fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
            assert!(!self.done, "单步迭代器至多一步");
            self.done = true;
            let n = self.attempts.get() + 1;
            self.attempts.set(n);
            let log = Rc::clone(&self.log);
            Box::pin(async move {
                if n == 1 {
                    // 首次：组件运行时失败（值通道，非 panic）。
                    log.borrow_mut().push("fail:run".into());
                    AsyncStep::Failed(AsyncFiberError::new("boom"))
                } else {
                    log.borrow_mut().push("revive:run".into());
                    AsyncStep::Finished(async_inverse(&log, "revive"))
                }
            })
        }
    }

    /// I-4（草案 §3.3）：`Failed` → 静止终态 + 自退役（loader G1 通道
    /// 写回 disabled）+ settle 恒可完成 + is_quiet 真 + 重启用复活（重建
    /// → 新代 drive spawn）。
    #[tokio::test]
    async fn i4_failed_settles_quiet_writeback_and_revive() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let loader = Rc::new(Loader::new(Rc::clone(ctx.runtime())));
                loader.register_retire_hook();
                let comp = rt.wrap_component(
                    Rc::new(Shell::empty()),
                    FailOnceBehavior {
                        log: Rc::clone(&log),
                        attempts: Rc::new(Cell::new(0)),
                    },
                );
                loader.register_component("failing", comp);
                loader.apply(&[Entry::new("failing", "failing", Rc::new(()), 0, false)]);
                // 激活即 Active（无依赖）；drive 失败是异步的——先取 fiber id。
                let fid = loader.fiber("failing").expect("已激活").id();

                // 等失败落地：drive → Failed → on_failed 自退役 → loader 写回。
                //
                // 决定论约定（REVIEW-596125d nit-1）：`FailOnceIter::next()`
                // 内日志与步完成同步、**无中途 await**——失败/复活日志在
                // spawn 后下一次 poll 即落盘，固定轮数 yield 头寸充裕、无
                // flaky。若日后在 `next()` 内引入 await/多步，须改造成可
                // await 的就绪条件，勿依赖本自旋基数。
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if loader.entry_disabled("failing") == Some(true) {
                        break;
                    }
                }
                assert_eq!(
                    loader.entry_disabled("failing"),
                    Some(true),
                    "G1 通道：自退役经 retire hook 写回条目 disabled"
                );
                assert!(
                    matches!(
                        rt.entry(fid).expect("条目存在").state(),
                        AsyncFiberState::Failed(_)
                    ),
                    "I-4：失败 = 静止终态"
                );

                // settle 恒可完成（失败路径 slot 留空、tail 正常收账）。
                rt.settle().await;
                assert!(rt.is_quiet(), "I-4：settle 后静止（Failed 视为静止）");

                // 复活：编排方重启用（disabled=false 重载）→ loader 重建 →
                // 新代 begin_activation + 新 drive spawn → 成功。
                loader.apply(&[Entry::new("failing", "failing", Rc::new(()), 0, false)]);
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if log.borrow().iter().any(|l| l == "revive:run") {
                        break;
                    }
                }
                assert!(
                    log.borrow().iter().any(|l| l == "revive:run"),
                    "复活：新代 drive 成功运行"
                );
                // entry/registrar 经 `wrap_component` 单实例恒共享（REVIEW-596125d
                // nit-3）：loader 重建产生新 fiber（可能新 fid），但同一注册器
                // 复用同一 entry——原 fid 的弱引用仍 upgrade 到它，换代递推
                // 由 begin_activation 保证，故按原 fid 查询成立。
                assert!(
                    matches!(
                        rt.entry(fid).expect("条目存在").state(),
                        AsyncFiberState::Running { generation: 2 }
                    ),
                    "复活：同一注册器条目换代（gen=2）并运行"
                );
            })
            .await;
    }

    /// 测试 11（契约 C-7）：编排方退役（loader teardown）→ shutdown 双真
    /// 断言通过；退役零配置污染（loader 驱动退役不写回 disabled）。
    #[tokio::test]
    async fn shutdown_after_orchestrator_retire_is_double_quiet() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let loader = Rc::new(Loader::new(Rc::clone(ctx.runtime())));
                loader.register_retire_hook();
                let comp = rt.wrap_component(
                    Rc::new(Shell::empty()),
                    OneShotBehavior {
                        label: "svc",
                        log: Rc::clone(&log),
                        provide_dep: false,
                    },
                );
                loader.register_component("svc", comp);
                loader.apply(&[Entry::new("svc", "svc", Rc::new(()), 0, false)]);
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                assert!(
                    log.borrow().iter().any(|l| l == "svc:run"),
                    "drive 完成（slot 填账）"
                );

                // 编排方退役：loader teardown（条目移除 → 退役 + 卸载）。
                loader.apply(&[]);
                assert_eq!(
                    loader.entry_disabled("svc"),
                    None,
                    "退役零配置污染：loader 驱动退役不写回 disabled（条目已移除，hook 过滤）"
                );

                // shutdown：兜底无事可做 → settle → 双真断言通过。
                rt.shutdown().await;
                assert!(rt.is_quiet(), "shutdown 后 async 视图静止");
            })
            .await;
    }

    /// 测试 11（契约 C-7 违约捕获）：编排方**未退役**即 shutdown → async
    /// 侧兜底收账（cancel + enqueue + settle 完成），core 侧仍有 Active
    /// fiber → 双真断言失败 panic（正式 assert，开放项 §4 决议）。
    #[tokio::test]
    #[should_panic(expected = "shutdown 双真断言")]
    async fn shutdown_without_orchestrator_retire_panics() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let fiber = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        OneShotBehavior {
                            label: "svc",
                            log: Rc::clone(&log),
                            provide_dep: false,
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载");
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                assert!(log.borrow().iter().any(|l| l == "svc:run"));

                // 未退役：shutdown 兜底 cancel+enqueue → settle 收账 → core
                // 仍有 Active fiber → 双真断言失败（调用方违约）。
                let _ = fiber;
                rt.shutdown().await;
            })
            .await;
    }
}

// ── M0.5（草案 §5）：门面完善——测试 8（代次更新）/ 9（无环关停）/ 10（H 竞态）──

mod m05 {
    use super::Log;
    use super::m03::{OneShotBehavior, Shell};
    use cordis_async::{
        AsyncBehavior, AsyncCx, AsyncDisposer, AsyncEffectIter, AsyncFiberState, AsyncRuntime,
        AsyncStep, LocalBoxFuture,
    };
    use cordis_core::Context;
    use std::any::Any;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// 带 String 标签的异步逆（yield 一拍后记录 `rev:{label}`）。
    fn async_inverse_owned(log: &Log, label: String) -> AsyncDisposer {
        let log = Rc::clone(log);
        Box::new(move || {
            let log = Rc::clone(&log);
            let label = label.clone();
            Box::pin(async move {
                tokio::task::yield_now().await;
                log.borrow_mut().push(format!("rev:{label}"));
            }) as LocalBoxFuture<()>
        }) as AsyncDisposer
    }

    /// 激活计数行为：每次激活记 `run:{n}`，逆记 `rev:{n}`（n = 激活序）。
    struct UpdateBehavior {
        log: Log,
        activations: Rc<Cell<u32>>,
    }

    impl AsyncBehavior for UpdateBehavior {
        fn apply_async(&self, _cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
            let n = self.activations.get() + 1;
            self.activations.set(n);
            Box::new(UpdateIter {
                n,
                log: Rc::clone(&self.log),
                done: false,
            })
        }
    }

    struct UpdateIter {
        n: u32,
        log: Log,
        done: bool,
    }

    impl AsyncEffectIter for UpdateIter {
        fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
            assert!(!self.done, "单步迭代器至多一步");
            self.done = true;
            let n = self.n;
            let log = Rc::clone(&self.log);
            Box::pin(async move {
                log.borrow_mut().push(format!("run:{n}"));
                AsyncStep::Finished(async_inverse_owned(&log, format!("{n}")))
            })
        }
    }

    /// 挂起单步行为：`next()` 返回 await oneshot 的 future（drive 在途）；
    /// 信号到达后记 `{label}:done` 并 Finished（逆记 `rev:{label}`）。
    struct PendingOnceBehavior {
        label: &'static str,
        log: Log,
        rx: Rc<RefCell<Option<tokio::sync::oneshot::Receiver<()>>>>,
    }

    impl AsyncBehavior for PendingOnceBehavior {
        fn apply_async(&self, _cx: AsyncCx, _config: &dyn Any) -> Box<dyn AsyncEffectIter> {
            Box::new(PendingOnceIter {
                label: self.label,
                log: Rc::clone(&self.log),
                rx: self.rx.borrow_mut().take(),
                done: false,
            })
        }
    }

    struct PendingOnceIter {
        label: &'static str,
        log: Log,
        rx: Option<tokio::sync::oneshot::Receiver<()>>,
        done: bool,
    }

    impl AsyncEffectIter for PendingOnceIter {
        fn next(&mut self) -> LocalBoxFuture<AsyncStep> {
            assert!(!self.done, "单步迭代器至多一步");
            self.done = true;
            let label = self.label;
            let log = Rc::clone(&self.log);
            let rx = self.rx.take().expect("一次挂起");
            Box::pin(async move {
                log.borrow_mut().push(format!("{label}:pending"));
                rx.await.expect("信号到达");
                log.borrow_mut().push(format!("{label}:done"));
                AsyncStep::Finished(async_inverse_owned(&log, label.to_string()))
            })
        }
    }

    /// 测试 8（评审点 E / 草案 §5）：`update(config)` → 旧代 cancel + 新代
    /// drive spawn（fiber 身份保留、代次递增）；旧尾巴由 settle 排空——
    /// 日志序直证「旧代尾巴在新代激活后收尾，且新代未收账」。
    #[tokio::test]
    async fn update_bumps_generation_and_settles_old_tail() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let fiber = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        UpdateBehavior {
                            log: Rc::clone(&log),
                            activations: Rc::new(Cell::new(0)),
                        },
                        Rc::new(1u8) as Rc<dyn Any>,
                    )
                    .expect("挂载");
                // 决定论约定（REVIEW-23383f3 nit-3）：`UpdateIter::next()` 无
                // 中途 await、单 poll 落盘——条件轮询即就绪即停，不依赖固定
                // spin 基数。若日后在 next() 内引入 await，须改造成可 await
                // 的就绪条件。
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if log.borrow().iter().any(|l| l == "run:1") {
                        break;
                    }
                }
                assert_eq!(*log.borrow(), vec!["run:1".to_string()], "首代 drive 完成");

                // 更新：旧代 unload（逆 cancel + 旧尾巴入队）→ 链式 reload
                // （新代 spawn）；fiber 身份保留。
                rt.update(&fiber, Rc::new(2u8) as Rc<dyn Any>);
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if log.borrow().iter().any(|l| l == "run:2") {
                        break;
                    }
                }
                assert_eq!(
                    *log.borrow(),
                    vec!["run:1".to_string(), "run:2".to_string()],
                    "新代 drive 已 spawn 并完成（旧逆尚未执行）"
                );
                assert!(
                    matches!(
                        rt.entry(fiber.id()).expect("条目").state(),
                        AsyncFiberState::Running { generation: 2 }
                    ),
                    "代次递增（gen=2），fiber 身份保留"
                );

                // settle：FIFO 排空旧代尾巴（rev:1）——新代未卸载、无新尾巴。
                rt.settle().await;
                assert_eq!(
                    *log.borrow(),
                    vec![
                        "run:1".to_string(),
                        "run:2".to_string(),
                        "rev:1".to_string()
                    ],
                    "旧代尾巴先 settle（rev:1）；新代（gen=2）未收账"
                );
                assert!(
                    matches!(
                        rt.entry(fiber.id()).expect("条目").state(),
                        AsyncFiberState::Running { generation: 2 }
                    ),
                    "settle 后新代仍运行"
                );
            })
            .await;
    }

    /// 测试 9（草案 §5 无环关停）：AsyncRuntime 可完整 drop（Weak 计数
    /// 归零——条目/fiber 无回边，评审点 B 成立）；core 侧安静。
    ///
    /// 走 retire+settle 而非 shutdown()（REVIEW-23383f3 nit-2 命名对齐）：
    /// 释放性验证（强计数归零）两路径等价；shutdown 双真路径已由 m04
    /// 测试 11 覆盖，此处不重复。
    #[tokio::test]
    async fn retired_settled_runtime_releases_no_cycle() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let fiber = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        OneShotBehavior {
                            label: "svc",
                            log: Rc::clone(&log),
                            provide_dep: false,
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载");
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                rt.retire(&fiber);
                rt.settle().await;

                let weak = Rc::downgrade(&rt);
                drop(rt);
                assert!(
                    weak.upgrade().is_none(),
                    "无环关停：AsyncRuntime 无残留强引用（条目/逆/兜底均不持回边）"
                );
                assert!(ctx.runtime().is_quiet(), "core 侧安静（退役 + 收账完成）");
            })
            .await;
    }

    /// 测试 10（评审点 H）：drive 恰在 cancel 后、settle 排空前完成 Ok →
    /// disposer 经共享槽被 settle **恰一次** await（A）；Failed 路径 slot
    /// 留空、无 disposer 残留（B）。
    #[tokio::test]
    async fn h_race_slot_taken_exactly_once_and_failed_slot_empty() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                // A：挂起在途的 drive（oneshot 门）。
                let (tx, rx) = tokio::sync::oneshot::channel();
                let a = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        PendingOnceBehavior {
                            label: "a",
                            log: Rc::clone(&log),
                            rx: Rc::new(RefCell::new(Some(rx))),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载 A");
                // B：首激活即失败（自退役；slot 恒空）。
                let b = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        super::m04::FailOnceBehavior {
                            log: Rc::clone(&log),
                            attempts: Rc::new(Cell::new(0)),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载 B");

                // 等 A 挂起（a:pending 落盘）与 B 失败落地（自退役写回）。
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if log.borrow().iter().any(|l| l == "a:pending") {
                        break;
                    }
                }
                assert!(log.borrow().iter().any(|l| l == "a:pending"));
                assert!(
                    matches!(
                        rt.entry(b.id()).expect("B 条目").state(),
                        AsyncFiberState::Failed(_)
                    ),
                    "B：失败静止终态（Failed slot 留空前件）"
                );

                // 竞态窗口：A 在途（drive 未完成）→ 退役（逆 cancel + enqueue
                // 在途 tail）→ 信号让在途步完成 → drive 步界退场（Ok，逆入账）
                // → settle 恰一次 take。
                rt.retire(&a);
                tx.send(()).expect("放行在途步");
                rt.settle().await;

                let rev_a = log
                    .borrow()
                    .iter()
                    .filter(|l| l.as_str() == "rev:a")
                    .count();
                assert_eq!(
                    rev_a, 1,
                    "H 竞态：共享槽被 settle 恰一次 take（rev:a 恰一次）"
                );
                assert!(
                    !log.borrow().iter().any(|l| l == "rev:b"),
                    "Failed 路径 slot 留空：无 B 的 disposer 被 await"
                );
                assert!(rt.is_quiet(), "A/B 均已退役且尾巴排空");
            })
            .await;
    }

    /// 测试 10 第三子句（草案 §9）：shutdown 对在途尾巴补收账——drive 在途
    /// 时退役 → 逆 enqueue 在途 tail → shutdown 的 settle 排空 → 逆恰一次；
    /// 双真断言通过（编排方已退役）。
    #[tokio::test]
    async fn shutdown_settles_inflight_tail() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let ctx = Context::new();
                let rt = AsyncRuntime::new(&ctx);
                let log: Log = Rc::new(RefCell::new(Vec::new()));

                let (tx, rx) = tokio::sync::oneshot::channel();
                let c = rt
                    .use_component(
                        &ctx,
                        Rc::new(Shell::empty()),
                        PendingOnceBehavior {
                            label: "c",
                            log: Rc::clone(&log),
                            rx: Rc::new(RefCell::new(Some(rx))),
                        },
                        Rc::new(()) as Rc<dyn Any>,
                    )
                    .expect("挂载 C");
                for _ in 0..64 {
                    tokio::task::yield_now().await;
                    if log.borrow().iter().any(|l| l == "c:pending") {
                        break;
                    }
                }
                assert!(log.borrow().iter().any(|l| l == "c:pending"));

                // 在途退役 → shutdown：settle 排空在途尾巴（补收账）→ 双真。
                rt.retire(&c);
                tx.send(()).expect("放行在途步");
                rt.shutdown().await;

                let rev_c = log
                    .borrow()
                    .iter()
                    .filter(|l| l.as_str() == "rev:c")
                    .count();
                assert_eq!(rev_c, 1, "shutdown 收账：在途尾巴的逆恰一次 await");
                assert!(rt.is_quiet(), "shutdown 后静止");
            })
            .await;
    }
}
