//! PR #15（M0 处置清单③）：**Thm 59/61 直接测试**。
//!
//! - **Thm 59（Preservation）**：registry 良构（Def 58 四条款）在任意规则步
//!   后保持——测试在编排动作序列的**每个动作后**断言四条款：
//!   (1) `π_n ∈ dom(Fγ) ∪ {root}`；(2) `m ≠ n ⇒ p_m ∩ p_n = ∅`；
//!   (3) installed 的 `ω_n` 在 `d_n` 上全函数、取值在 `dom(Fγ)`；
//!   (4) `installed_n ∧ k ∈ d_n ∧ ω_n(k) = m ⇒ installed_m`。
//! - **Thm 61（Recovery exactness）**：成对独立步序列中，fiber 的累加器
//!   "撤回自己的贡献、不动他人"（式 (56)：`g^u_n(γ^u) ≈ (Ψ_tl ∘ ⋯ ∘ Ψ_t1)(γ^b)`）
//!   ——测试以多 fiber 交错动作 + 中间退役，断言最终状态 == 其余 fiber
//!   单独推进的状态（观测等价）。
//!
//! 间接覆盖（property.rs：Thm 66/73 + oracle×引擎）之上的**直接**回归测试。

use cordis_core::effect::EffectIter;
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{
    Component, Context, Disposer, Fiber, FiberId, FiberState, Key, RegistryError, Runtime, Step,
};
use std::rc::Rc;

// ── 固定键宇宙（u8 值）──────────────────────────────────────────────────

macro_rules! key {
    ($name:ident, $sym:literal) => {
        struct $name;
        impl Key for $name {
            type Value = u8;
            const SYMBOL: &'static str = $sym;
        }
    };
}

key!(K0, "k0");
key!(K1, "k1");
key!(K2, "k2");
key!(K3, "k3");
key!(K4, "k4");

/// 在 `ctx` 绑定一个键（测试固定值 1）。键 → 类型分派。
fn bind(ctx: &Rc<Context>, sym: Symbol) -> Disposer {
    match sym.as_str() {
        "k0" => ctx.set::<K0>(1).expect("测试绑定"),
        "k1" => ctx.set::<K1>(1).expect("测试绑定"),
        "k2" => ctx.set::<K2>(1).expect("测试绑定"),
        "k3" => ctx.set::<K3>(1).expect("测试绑定"),
        "k4" => ctx.set::<K4>(1).expect("测试绑定"),
        other => panic!("键宇宙外的符号：{other}"),
    }
}

/// 多键提供者：激活时逐步绑定每个 provide 键（每步一个可逆效应）。
struct MultiProvider {
    provide: Vec<Symbol>,
}

impl Component for MultiProvider {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        self.provide.iter().copied().collect()
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(MultiIter {
            ctx,
            provide: self.provide.clone(),
            i: 0,
        })
    }
}

struct MultiIter {
    ctx: Rc<Context>,
    provide: Vec<Symbol>,
    i: usize,
}

impl EffectIter for MultiIter {
    fn next(&mut self) -> Step {
        if self.i < self.provide.len() {
            let sym = self.provide[self.i];
            self.i += 1;
            Step::Yielded(bind(&self.ctx, sym))
        } else {
            Step::Finished(Box::new(|| {}) as Disposer)
        }
    }
}

/// 消费者：注入 `inject`（激活后自身无效应），可选提供 `provide`。
struct Consumer {
    inject: Vec<Symbol>,
    provide: Vec<Symbol>,
}

impl Component for Consumer {
    fn inject(&self) -> KeySet {
        self.inject.iter().copied().collect()
    }
    fn provide(&self) -> KeySet {
        self.provide.iter().copied().collect()
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        if self.provide.is_empty() {
            Box::new(once_finished())
        } else {
            // 组件须在激活时实际绑定声明的供给（Def 43 纪律：声明 + 效应）。
            Box::new(MultiIter {
                ctx,
                provide: self.provide.clone(),
                i: 0,
            })
        }
    }
}

fn once_finished() -> impl EffectIter {
    struct Done(bool);
    impl EffectIter for Done {
        fn next(&mut self) -> Step {
            if self.0 {
                self.0 = false;
                Step::Finished(Box::new(|| {}) as Disposer)
            } else {
                unreachable!("迭代器已完成")
            }
        }
    }
    Done(true)
}

fn s(sym: &str) -> Symbol {
    Symbol::intern(sym)
}

fn spec(names: &[&str]) -> Vec<Symbol> {
    names.iter().map(|n| s(n)).collect()
}

/// 测试追踪的注册表成员（含已退役未移除者——O-Insert 供给占用语义）。
type Registry = Vec<(FiberId, Rc<Fiber>)>;

/// Def 58 四条款断言（Thm 59）。仅对 `installed`（`Active`）fiber 检查
/// 条款 (3)/(4)（`Unloading`/`Reloading` 为转换中间态，同步引擎动作后
/// 已静止，不出现）。
fn assert_well_formed(runtime: &Runtime, registry: &Registry) {
    // 注册表成员 = 测试追踪且仍在 registry（未 remove）。
    let members: Vec<&Rc<Fiber>> = registry
        .iter()
        .filter(|(id, _)| runtime.fiber(*id).is_some())
        .map(|(_, f)| f)
        .collect();

    // 条款 (1)：π_n ∈ dom(Fγ) ∪ {root}。
    for f in &members {
        if let Some(parent) = f.parent() {
            assert!(
                registry.iter().any(|(id, _)| *id == parent),
                "条款 (1)：fiber {:?} 的父 {:?} 在注册表中",
                f.id(),
                parent
            );
        }
    }

    // 条款 (2)：m ≠ n ⇒ p_m ∩ p_n = ∅。
    for (i, a) in members.iter().enumerate() {
        for b in &members[i + 1..] {
            assert!(
                !a.provide().intersects(b.provide()),
                "条款 (2)：fiber {:?} 与 {:?} 供给相交",
                a.id(),
                b.id()
            );
        }
    }

    // 条款 (3)/(4)：installed（Active）的 ω。
    for f in &members {
        let FiberState::Active { view } = &*f.state() else {
            continue;
        };
        // (3) ω 在 d 上全函数：dom(ω) == d。
        let d: KeySet = f.inject().iter().collect();
        let dom_omega: KeySet = view.keys().copied().collect();
        assert_eq!(
            dom_omega,
            d,
            "条款 (3)：fiber {:?} 的 ω 在 d 上全函数",
            f.id()
        );
        // (3) 取值在 dom(Fγ)。
        for provider in view.values() {
            assert!(
                runtime.fiber(*provider).is_some(),
                "条款 (3)：fiber {:?} 的 ω 取值 {:?} 在注册表中",
                f.id(),
                provider
            );
        }
        // (4) ω(k) = m ⇒ installed_m。
        for provider in view.values() {
            let p = runtime.fiber(*provider).expect("条款 (3) 已保证在注册表");
            assert!(
                matches!(&*p.state(), FiberState::Active { .. }),
                "条款 (4)：fiber {:?} 的提供者 {:?} 应 installed",
                f.id(),
                provider
            );
        }
    }
}

fn use_at(
    root: &Rc<Context>,
    component: impl Component + 'static,
) -> Result<Rc<Fiber>, RegistryError> {
    root.use_component(Rc::new(component), Rc::new(()))
}

fn active(f: &Rc<Fiber>) -> bool {
    matches!(&*f.state(), FiberState::Active { .. })
}

fn inactive(f: &Rc<Fiber>) -> bool {
    matches!(&*f.state(), FiberState::Inactive(_))
}

// ── Thm 59：良构四条款在编排全过程中保持 ────────────────────────────────

#[test]
fn thm59_wellformedness_preserved_throughout_orchestration() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let mut registry: Registry = Vec::new();

    // 1. 提供者 a（k0）激活。
    let a = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k0"]),
        },
    )
    .expect("a 实例化");
    registry.push((a.id(), a.clone()));
    assert!(active(&a), "a 激活");
    assert_well_formed(&runtime, &registry);

    // 2. 消费者 b（注入 k0，提供 k1）→ 激活；消费者 c（注入 k1，提供 k2）→ 激活。
    let b = use_at(
        &root,
        Consumer {
            inject: spec(&["k0"]),
            provide: spec(&["k1"]),
        },
    )
    .expect("b 实例化");
    registry.push((b.id(), b.clone()));
    let c = use_at(
        &root,
        Consumer {
            inject: spec(&["k1"]),
            provide: spec(&["k2"]),
        },
    )
    .expect("c 实例化");
    registry.push((c.id(), c.clone()));
    assert!(active(&b) && active(&c), "b、c 激活（依赖链 a→b→c）");
    assert_well_formed(&runtime, &registry);

    // 3. 独立提供者 d（k3）。
    let d = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k3"]),
        },
    )
    .expect("d 实例化");
    registry.push((d.id(), d.clone()));
    assert_well_formed(&runtime, &registry);

    // 4. 退役 b → c 级联停用（k1 无提供者）。四条款仍成立（Inactive 不参与
    //    条款 3/4；条款 2 覆盖退役未移除者：b 的 k1 供给名仍占用）。
    b.retire();
    assert!(inactive(&b) && inactive(&c), "退役 b → c 级联停用");
    assert!(active(&a) && active(&d), "a、d 不受影响");
    assert_well_formed(&runtime, &registry);

    // 5. 移除 c、b（释放 k1 供给名；O-Remove 前提：先退役），重连 b2、c2。
    c.retire();
    runtime.remove_fiber(c.id()).expect("移除 c");
    runtime.remove_fiber(b.id()).expect("移除 b");
    let b2 = use_at(
        &root,
        Consumer {
            inject: spec(&["k0"]),
            provide: spec(&["k1"]),
        },
    )
    .expect("b2 实例化（重连）");
    registry.push((b2.id(), b2.clone()));
    let c2 = use_at(
        &root,
        Consumer {
            inject: spec(&["k1"]),
            provide: spec(&["k2"]),
        },
    )
    .expect("c2 实例化");
    registry.push((c2.id(), c2.clone()));
    assert!(active(&b2) && active(&c2), "重连后 b2、c2 激活");
    assert_well_formed(&runtime, &registry);

    // 6. 退役 a → b2、c2 级联停用；随后清场。
    a.retire();
    assert!(inactive(&b2) && inactive(&c2), "退役 a → b2、c2 级联停用");
    assert_well_formed(&runtime, &registry);
    c2.retire();
    b2.retire();
    d.retire();
    runtime.remove_fiber(c2.id()).expect("移除 c2");
    runtime.remove_fiber(b2.id()).expect("移除 b2");
    runtime.remove_fiber(d.id()).expect("移除 d");
    runtime.remove_fiber(a.id()).expect("移除 a");
    assert_well_formed(&runtime, &registry);
    assert!(runtime.is_quiet(), "回到静止");
}

// ── Thm 61：交错动作下，逆只撤回自己的贡献 ──────────────────────────────

#[test]
fn thm61_recovery_exactness_with_interleaved_fibers() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    // 三个 fiber 交错：a（k0）→ c（注入 k0、提供 k1）→ b（k2）→ d（注入 k1）。
    // 动作序列使各 fiber 的效应在时间上交错（a 早于 c，c 早于 d，b 插在中间）。
    let a = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k0"]),
        },
    )
    .expect("a");
    let c = use_at(
        &root,
        Consumer {
            inject: spec(&["k0"]),
            provide: spec(&["k1"]),
        },
    )
    .expect("c");
    let b = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k2"]),
        },
    )
    .expect("b");
    let d = use_at(
        &root,
        Consumer {
            inject: spec(&["k1"]),
            provide: vec![],
        },
    )
    .expect("d");
    assert!(
        active(&a) && active(&c) && active(&b) && active(&d),
        "全部激活"
    );
    let store = runtime.store();
    assert!(
        ["k0", "k1", "k2"].iter().all(|k| store.contains(s(k))),
        "静止态：k0/k1/k2 全部绑定"
    );
    drop(store);

    // 中间退役 b：仅 k2 撤销；k0（a）、k1（c）保留——式 (56)：
    // b 的累加器在 γ^u 撤回的恰是其贡献，其余 fiber 的状态不受扰动。
    b.retire();
    assert!(inactive(&b), "b 停用");
    assert!(
        active(&a) && active(&c) && active(&d),
        "其余 fiber 不受影响"
    );
    let store = runtime.store();
    assert!(store.contains(s("k0")), "k0 保留（a 的贡献）");
    assert!(store.contains(s("k1")), "k1 保留（c 的贡献）");
    assert!(!store.contains(s("k2")), "k2 已撤回（b 的贡献）");
    drop(store);

    // 再退役 c：k1 撤销、d 级联停用，k0 仍在。
    c.retire();
    assert!(inactive(&c) && inactive(&d), "退役 c → d 级联停用");
    let store = runtime.store();
    assert!(store.contains(s("k0")), "k0 保留");
    assert!(!store.contains(s("k1")), "k1 已撤回");
    drop(store);

    // 最后退役 a：全清。
    a.retire();
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "回到静止");
}

/// 交错 + 中间退役的反向顺序：先退役最老提供者，观察其余贡献保持。
#[test]
fn thm61_recovery_exactness_retiring_oldest_first() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();

    let a = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k0", "k1"]),
        },
    )
    .expect("a");
    let b = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k2", "k3"]),
        },
    )
    .expect("b");
    let c = use_at(
        &root,
        MultiProvider {
            provide: spec(&["k4"]),
        },
    )
    .expect("c");
    assert!(active(&a) && active(&b) && active(&c));

    // 先退役 a（最早动作的 fiber）：k0/k1 撤回，b、c 的贡献保持。
    a.retire();
    assert!(inactive(&a));
    assert!(active(&b) && active(&c), "b、c 不受影响");
    let store = runtime.store();
    assert!(
        !store.contains(s("k0")) && !store.contains(s("k1")),
        "a 的贡献已撤"
    );
    assert!(
        store.contains(s("k2")) && store.contains(s("k3")),
        "b 的贡献保持"
    );
    assert!(store.contains(s("k4")), "c 的贡献保持");
    drop(store);

    // 交错退役 c：仅 k4 撤销。
    c.retire();
    let store = runtime.store();
    assert!(!store.contains(s("k4")), "c 的贡献已撤");
    assert!(
        store.contains(s("k2")) && store.contains(s("k3")),
        "b 的贡献保持"
    );
    drop(store);

    b.retire();
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    assert!(runtime.is_quiet(), "回到静止");
}
