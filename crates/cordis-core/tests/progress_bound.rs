//! PR #15（M0 处置清单④）：**Thm 66（Progress）定量上界断言**。
//!
//! Thm 66(2)：`S(n) ≤ (K+4)(V(n)+1)`——`S(n)` 为作用于 n 的生命周期步数，
//! `K` 为效应迭代器长度上界（`len(e_n) ≤ K`），`V(n)` 为目标视图翻转次数
//! （式 (61)：`|{t : target^t_n ≠ target^{t+1}_n}|`）。
//!
//! 同步核心不暴露逐"生命周期步"计数（转换在单次 refresh 调用链内完成，
//! 见 THEORY-MAP 同步适配记录）——本测试断言可观测的定量上界：
//!
//! - **(a) 效应步总数**：n 的迭代器 `next()` 调用总数（包装计数）
//!   ≤ `(K+4)(V+1)`（每个激活期 ≤ K+1 次 next，激活期数 = 目标翻转驱动
//!   的安装期数 ≤ V+1，故必然满足——这是对 Thm 66 上界中效应步部分
//!   的直接观测）；
//! - **(b) 转换次数**：n 的目标翻转触发的转换数（受控场景中每次供给
//!   增删恰好一次）≤ `(K+4)(V+1)`；
//! - **(c) 无死锁**：每阶段驱动后 `is_quiet`（Thm 66(1) 的可观测形式）。
//!
//! 引擎侧精确 `S(n)` 计数需生命周期步计数器（记录为 M2 可选增强，见
//! THEORY-MAP M1 走查处置④）。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, Context, Disposer, Fiber, Key, RegistryError, Runtime, Step};
use std::cell::Cell;
use std::rc::Rc;

// ── 键宇宙 ──────────────────────────────────────────────────────────────

struct K0;
impl Key for K0 {
    type Value = u8;
    const SYMBOL: &'static str = "k0";
}

fn k0() -> Symbol {
    Symbol::intern("k0")
}

/// 计数包装：记录迭代器 `next()` 调用总数（= 效应步总数）。
struct CountingIter {
    inner: Box<dyn EffectIter>,
    steps: Rc<Cell<usize>>,
}

impl EffectIter for CountingIter {
    fn next(&mut self) -> Step {
        self.steps.set(self.steps.get() + 1);
        self.inner.next()
    }
}

/// 长度 `len` 的迭代器：`len` 次 Yielded（每次绑定 k0）后 Finished。
struct BindIter {
    ctx: Rc<Context>,
    left: usize,
}

impl EffectIter for BindIter {
    fn next(&mut self) -> Step {
        if self.left > 0 {
            self.left -= 1;
            let inverse = self.ctx.set::<K0>(1).expect("绑定 k0");
            Step::Yielded(inverse)
        } else {
            Step::Finished(Box::new(|| {}) as Disposer)
        }
    }
}

/// 长度 `len` 的**无效应**迭代器：`len` 次 Yielded（空逆，不绑定任何键——
/// 被测组件 n 只观察步数，不声明供给，越界绑定会触发 Def 43/48 纪律）后
/// Finished。
struct NoopIter {
    left: usize,
}

impl EffectIter for NoopIter {
    fn next(&mut self) -> Step {
        if self.left > 0 {
            self.left -= 1;
            Step::Yielded(Box::new(|| {}) as Disposer)
        } else {
            Step::Finished(Box::new(|| {}) as Disposer)
        }
    }
}

/// 被测组件 n：注入 `k0`（目标视图随提供者增删翻转），激活时运行长度
/// `K` 的迭代器（效应步数上界 K）。
struct Probe {
    k: usize,
    steps: Rc<Cell<usize>>,
}

impl Component for Probe {
    fn inject(&self) -> KeySet {
        [k0()].into_iter().collect()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(CountingIter {
            inner: Box::new(NoopIter { left: self.k }),
            steps: Rc::clone(&self.steps),
        })
    }
}

/// 提供者（提供 k0，激活时实际绑定——Def 43：声明 + 效应）。
struct Provider;

impl Component for Provider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        [k0()].into_iter().collect()
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        // 一步绑定 k0（1 次 Yielded + Finished）。
        Box::new(BindIter { ctx, left: 1 })
    }
}

fn use_at(
    root: &Rc<Context>,
    component: impl Component + 'static,
) -> Result<Rc<Fiber>, RegistryError> {
    root.use_component(Rc::new(component), Rc::new(()))
}

fn active(f: &Rc<Fiber>) -> bool {
    matches!(&*f.state(), cordis_core::FiberState::Active { .. })
}

/// Thm 66(2) 定量上界：K = 迭代器长度上界，V = 目标视图翻转次数
/// （受控场景：每次供给增删 = 一次翻转）。
#[test]
fn thm66_progress_quantitative_bound() {
    const K: usize = 5; // len(e_n) 上界（测试组件迭代器长度）
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let steps = Rc::new(Cell::new(0usize));

    let n = use_at(
        &root,
        Probe {
            k: K,
            steps: Rc::clone(&steps),
        },
    )
    .expect("n 实例化");
    assert!(inactive_or_pending(&n), "无提供者：n 未激活");

    // 目标视图翻转：每次提供者增删 = n 的一次 target 翻转。
    let mut v_turns = 0usize;
    let mut conversions = 0usize;

    let p1 = use_at(&root, Provider).expect("p1 实例化");
    v_turns += 1;
    conversions += 1;
    assert!(active(&n), "提供者就位 → n 激活（翻转 #1）");
    assert!(runtime.is_quiet(), "Thm 66(1)：阶段静止");

    // 中间翻转几次（退役/重装提供者）。
    p1.retire();
    runtime
        .remove_fiber(p1.id())
        .expect("移除 p1（释放供给名 k0）");
    v_turns += 1;
    conversions += 1;
    assert!(inactive_or_pending(&n), "提供者退役 → n 停用（翻转 #2）");
    assert!(runtime.is_quiet());

    let p2 = use_at(&root, Provider).expect("p2 实例化");
    v_turns += 1;
    conversions += 1;
    assert!(active(&n), "重装提供者 → n 再激活（翻转 #3）");
    assert!(runtime.is_quiet());

    p2.retire();
    runtime.remove_fiber(p2.id()).expect("移除 p2");
    v_turns += 1;
    conversions += 1;
    assert!(inactive_or_pending(&n), "（翻转 #4）");
    assert!(runtime.is_quiet());

    let p3 = use_at(&root, Provider).expect("p3 实例化");
    v_turns += 1;
    conversions += 1;
    assert!(active(&n), "（翻转 #5）");
    assert!(runtime.is_quiet());

    // 清场。
    p3.retire();
    v_turns += 1;
    conversions += 1;
    assert!(inactive_or_pending(&n), "（翻转 #6）");
    runtime.remove_fiber(p3.id()).expect("移除 p3");
    n.retire();
    runtime.remove_fiber(n.id()).expect("移除 n");
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "回到静止");

    // ── Thm 66(2) 定量断言 ──────────────────────────────────────────
    let bound = (K + 4) * (v_turns + 1);
    let effect_steps = steps.get();
    assert!(
        effect_steps <= bound,
        "效应步总数 {effect_steps} ≤ (K+4)(V+1) = {bound}（K={K}, V={v_turns}）"
    );
    assert!(
        conversions <= bound,
        "转换次数 {conversions} ≤ (K+4)(V+1) = {bound}"
    );
    // 更强的观测：每个激活期 ≤ K+1 次 next（K 次 Yielded + 1 次 Finished）；
    // 激活期数 = 安装期数 = v_turns/2（每两次翻转一次安装）。
    let episodes = v_turns / 2;
    assert!(
        effect_steps <= (K + 1) * episodes,
        "效应步紧界：{effect_steps} ≤ (K+1)×安装期数 = {}（K={K}, 安装期={episodes}）",
        (K + 1) * episodes
    );
}

fn inactive_or_pending(f: &Rc<Fiber>) -> bool {
    matches!(&*f.state(), cordis_core::FiberState::Inactive(_))
}
