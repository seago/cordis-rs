//! M0.2（草案 §1）：`drive` 引擎 + 不变量 I-1（LIFO 逆）/ I-2（步界 guard）。
//!
//! - I-1：drive 折叠的复合逆以**应用逆序** await 各步逆（Thm 16 的 async 版）。
//! - I-2：guard 仅在步界检查；await 挂起期间 guard 翻假不中断在途步——
//!   在途步完成后其逆照常入账并参与恢复。

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
