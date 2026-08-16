//! 元理论 property suites（PLAN §6 / §9 PR #6）：
//! 以参考解释器（`cordis_core::interp`，PR #2）为 **oracle**，验证真实引擎
//! （`Runtime`，PR #5）在随机编排下的一致性。
//!
//! 覆盖的定理：
//!
//! - **Thm 73 / Cor 62（Confluence、离场无残留）**：同一随机编排序列下，
//!   引擎的静止态（活跃集、`σγ`、绑定总数）与 oracle 逐步一致——最终状态
//!   只由配置决定（引擎的同步级联交错与 oracle 的规范驱动交错不同，结果
//!   必须相同），离场 fiber 的绑定不得泄漏；
//! - **Thm 66（Progress）**：同步核心的转换在调用栈上跑完——每个编排动作
//!   之后引擎必须立即静止（`is_quiet` 断言）；
//! - **Thm 63 的停用结果**：活跃集一致性蕴含「提供者停用时依赖者先行停用」
//!   的最终结果；teardown 期间依赖可读的**顺序性**由 PR #5 集成测试
//!   `withdrawal_cascade_disposes_dependents_first` 直接验证（本套件不重复）。
//!
//! 动作错误一致性亦被断言：插入供给冲突、未知父、未知 fiber、未退役移除
//! 等前提违反必须**同侧**报错（oracle 拒绝 ⟺ 引擎拒绝）。
//!
//! 动作空间含 **parent 维度**（审查 m2）：`Insert` 可引用已插入的 fiber 为父
//! ——引擎在父 fiber 的 ctx 上实例化（Def 47 注册进父累加器），oracle 登记
//! 注册子代（父卸载时子代被 O-Retire）——`HasChildren` 移除前提与父级联
//! 退役由此进入随机覆盖。
//!
//! 组件采用 **Def 69 规范模型**：激活恰好安装 `provide` 全部键
//! （与 oracle 的规范化效应建模一致）。
//!
//! **验证强度（审查 m1）**：`proptest_config` 固定 2000 用例（默认 256 的
//! 8 倍）——随仓库固化，不受 CI 环境影响。

use cordis_core::interp::{Component as InterpComponent, InterpState};
use cordis_core::{
    Component, Context, Disposer, EffectIter, FiberId, FiberState, Key, KeySet, RegistryError,
    Runtime, Step, Symbol,
};
use proptest::prelude::*;
use std::collections::BTreeSet;
use std::rc::Rc;

/// 键宇宙（3 键足够覆盖注入/供给组合，含空集）。
const KEY_UNIVERSE: [&str; 3] = ["k0", "k1", "k2"];

// ── 固定键宇宙的具体键类型（Key::SYMBOL 是静态常量）─────────────────────

struct Key0;
impl Key for Key0 {
    type Value = u8;
    const SYMBOL: &'static str = "k0";
}

struct Key1;
impl Key for Key1 {
    type Value = u8;
    const SYMBOL: &'static str = "k1";
}

struct Key2;
impl Key for Key2 {
    type Value = u8;
    const SYMBOL: &'static str = "k2";
}

fn bind_symbol(ctx: &Rc<Context>, sym: Symbol, value: u8) -> Disposer {
    match sym.as_str() {
        "k0" => ctx.set::<Key0>(value).expect("harness 绑定"),
        "k1" => ctx.set::<Key1>(value).expect("harness 绑定"),
        "k2" => ctx.set::<Key2>(value).expect("harness 绑定"),
        other => panic!("键宇宙外的符号：{other}"),
    }
}

// ── Def 69 规范组件：激活安装全部 provide 键（值 = 常量）────────────────

struct CanonicalComponent {
    inject: KeySet,
    provide: Vec<Symbol>,
}

impl Component for CanonicalComponent {
    fn inject(&self) -> KeySet {
        self.inject.clone()
    }
    fn provide(&self) -> KeySet {
        self.provide.iter().copied().collect()
    }
    fn apply(&self, ctx: Rc<Context>, _config: &dyn std::any::Any) -> Box<dyn EffectIter> {
        Box::new(CanonicalIter {
            ctx,
            provide: self.provide.clone(),
            step: 0,
        })
    }
}

struct CanonicalIter {
    ctx: Rc<Context>,
    provide: Vec<Symbol>,
    step: usize,
}

impl EffectIter for CanonicalIter {
    fn next(&mut self) -> Step {
        if self.step < self.provide.len() {
            let sym = self.provide[self.step];
            self.step += 1;
            Step::Yielded(bind_symbol(&self.ctx, sym, 1))
        } else {
            Step::Finished(Box::new(|| {}) as Disposer)
        }
    }
}

// ── 随机编排动作（Retire/Remove/父引用以「第 k 次成功插入」为索引）──────

#[derive(Debug, Clone)]
enum RawAction {
    Insert {
        /// 父引用（第 k 次成功插入的 fiber；None = root）。
        parent: Option<usize>,
        inject: Vec<usize>,
        provide: Vec<usize>,
    },
    Retire {
        insert_idx: usize,
    },
    Remove {
        insert_idx: usize,
    },
}

fn spec_from(indices: &[usize]) -> KeySet {
    indices
        .iter()
        .map(|&i| Symbol::intern(KEY_UNIVERSE[i % KEY_UNIVERSE.len()]))
        .collect()
}

fn arb_spec() -> impl Strategy<Value = Vec<usize>> {
    prop::collection::vec(0usize..KEY_UNIVERSE.len(), 0..=3)
}

fn arb_action() -> impl Strategy<Value = RawAction> {
    prop_oneof![
        3 => (prop::option::weighted(0.3, 0usize..16), arb_spec(), arb_spec())
            .prop_map(|(parent, inject, provide)| RawAction::Insert { parent, inject, provide }),
        1 => (0usize..16).prop_map(|insert_idx| RawAction::Retire { insert_idx }),
        1 => (0usize..16).prop_map(|insert_idx| RawAction::Remove { insert_idx }),
    ]
}

// ── Harness：同一动作序列驱动 oracle 与引擎并逐步对比 ──────────────────

struct Harness {
    oracle: InterpState,
    runtime: Rc<Runtime>,
    root: Rc<Context>,
    /// 成功插入的 fiber id（两端按同名顺序分配，必须一致）。
    inserted: Vec<FiberId>,
}

impl Harness {
    fn new() -> Self {
        let runtime = Rc::new(Runtime::new());
        let root = runtime.context();
        Self {
            oracle: InterpState::new(),
            runtime,
            root,
            inserted: Vec::new(),
        }
    }

    fn apply(&mut self, action: &RawAction) {
        match action {
            RawAction::Insert {
                parent,
                inject,
                provide,
            } => {
                let inject = spec_from(inject);
                let provide = spec_from(provide);
                // 父引用解析：引用不存在（越界/已移除）→ 两侧一致报错
                // （oracle UnknownParent / 引擎无父 ctx 可实例化）。
                let parent_id = parent.and_then(|idx| self.inserted.get(idx).copied());
                let o_res = self.oracle.insert(
                    parent_id,
                    &InterpComponent {
                        inject: inject.clone(),
                        provide: provide.clone(),
                    },
                );
                let component: Rc<dyn Component> = Rc::new(CanonicalComponent {
                    inject: inject.clone(),
                    provide: provide.iter().collect(),
                });
                let e_res = match parent_id {
                    None => self.root.use_component(component, Rc::new(())),
                    // Def 47：在父 fiber 的 ctx 上实例化（注册进父的累加器）。
                    Some(pid) => match self.runtime.fiber(pid) {
                        Some(parent_fiber) => {
                            parent_fiber.ctx().use_component(component, Rc::new(()))
                        }
                        None => Err(RegistryError::UnknownParent),
                    },
                };
                match (o_res, e_res) {
                    (Ok(oid), Ok(fiber)) => {
                        assert_eq!(oid, fiber.id(), "两端 fiber id 必须一致");
                        assert_eq!(parent_id, fiber.parent(), "两端父引用必须一致");
                        self.inserted.push(oid);
                    }
                    (Err(_), Err(_)) => {} // 前提违反（供给冲突/未知父）两侧一致拒绝
                    (Ok(_), Err(e)) => panic!("引擎拒绝而 oracle 接受：{e:?}"),
                    (Err(e), Ok(_)) => panic!("oracle 拒绝而引擎接受：{e:?}"),
                }
            }
            RawAction::Retire { insert_idx } => {
                let Some(&id) = self.inserted.get(*insert_idx) else {
                    return; // 引用不存在：两端均无操作
                };
                match (self.oracle.retire(id), self.runtime.fiber(id)) {
                    (Ok(()), Some(fiber)) => fiber.retire(),
                    (Err(_), None) => {} // 已移除：两端一致（UnknownFiber）
                    (Ok(()), None) => panic!("oracle 可退役而引擎查无此 fiber"),
                    (Err(_), Some(_)) => panic!("oracle 拒绝退役而引擎存在此 fiber"),
                }
            }
            RawAction::Remove { insert_idx } => {
                let Some(&id) = self.inserted.get(*insert_idx) else {
                    return; // 引用不存在：两端均无操作
                };
                match (self.oracle.remove(id), self.runtime.remove_fiber(id)) {
                    (Ok(()), Ok(())) => {}
                    (Err(_), Err(_)) => {} // 前提违反（未退役/活跃/未知）两侧一致拒绝
                    (Ok(()), Err(e)) => panic!("引擎拒绝移除而 oracle 接受：{e:?}"),
                    (Err(e), Ok(())) => panic!("oracle 拒绝移除而引擎接受：{e:?}"),
                }
            }
        }
        // Thm 66（引擎形态）：同步核心下每个动作后必须已静止。
        assert!(
            self.runtime.is_quiet(),
            "引擎在编排动作后必须静止（Thm 66）"
        );
        self.oracle.drive_to_quiescence();
    }

    fn compare(&self, context: &str) {
        // 活跃集（Thm 73 的安装集；引擎交错 vs oracle 规范驱动必须一致）。
        let o_active = self.oracle.active_set();
        let e_active: BTreeSet<FiberId> = self.runtime.active_fibers();
        assert_eq!(o_active, e_active, "活跃集不一致：{context}");
        // σγ。
        assert_eq!(
            self.oracle.provided(),
            self.runtime.provided(),
            "σγ 不一致：{context}"
        );
        // Cor 62 离场无残留：绑定总数 == σγ 大小——离场 fiber 的绑定
        // 不得在 store 中泄漏（引擎的 store 是唯一事实来源）。
        let binding_count = self.runtime.store().symbols().count();
        assert_eq!(
            binding_count,
            self.oracle.provided().len(),
            "离场 fiber 有残留绑定：{context}"
        );
    }
}

proptest! {
    // 审查 m1：固定验证强度（2000 用例，默认 256 的 8 倍），随仓库固化。
    #![proptest_config(ProptestConfig::with_cases(2000))]
    #[test]
    fn engine_matches_oracle(actions in prop::collection::vec(arb_action(), 1..=12)) {
        let mut h = Harness::new();
        for (i, action) in actions.iter().enumerate() {
            h.apply(action);
            h.compare(&format!("step {i}: {action:?}"));
        }
    }
}

/// Thm 73(1)（canonical form）：动态历史的静止态 == 静态装配的静止态。
///
/// 组件集合 `{A: ∅→k0, B: k0→k1, C: k0,k1→∅}`（⊲ 序 A→B→C）。
/// - **动态**：乱序注册（C、B 先 Inactive，A 出现后级联激活）→ 退役 A
///   （B、C 级联停用）→ 重装 A′（B、C 重新激活）——交错历史后静止；
/// - **canonical**：同一编排步骤，A 中每个 fiber 一个 episode、按 ⊲ 序
///   （A→B→C）一次性装入，无卸载。
///
/// 断言两者活跃集一致——"动态历史无痕迹 = 静态装配"（§4.4.5 招牌承诺）。
#[test]
fn thm73_canonical_form_static_assembly() {
    let comp = |inject: &[&str], provide: &[&str]| -> Rc<dyn Component> {
        Rc::new(CanonicalComponent {
            inject: inject.iter().map(|s| Symbol::intern(s)).collect(),
            provide: provide.iter().map(|s| Symbol::intern(s)).collect(),
        })
    };

    // ── 动态历史（乱序注册 + 退役 + 重装）────────────────────────────
    let rt = Rc::new(Runtime::new());
    let root = rt.context();
    let c = root
        .use_component(comp(&["k0", "k1"], &[]), Rc::new(()))
        .unwrap();
    let b = root
        .use_component(comp(&["k0"], &["k1"]), Rc::new(()))
        .unwrap();
    assert!(matches!(&*c.state(), FiberState::Inactive(_)), "依赖未满足");
    assert!(matches!(&*b.state(), FiberState::Inactive(_)), "依赖未满足");
    let a = root.use_component(comp(&[], &["k0"]), Rc::new(())).unwrap();
    assert!(matches!(&*a.state(), FiberState::Active { .. }));
    assert!(
        matches!(&*b.state(), FiberState::Active { .. }),
        "A 激活 → B 级联"
    );
    assert!(
        matches!(&*c.state(), FiberState::Active { .. }),
        "B 激活 → C 级联"
    );

    // 退役 A：B、C 级联停用；移除 A（退役 fiber 仍占供给名，O-Insert 语义）
    // 后重装 A′：B、C 重新激活（交错历史）。
    a.retire();
    assert!(
        matches!(&*b.state(), FiberState::Inactive(_)),
        "A 退役 → B 停用"
    );
    rt.remove_fiber(a.id()).expect("退役且 Inactive 后可移除");
    let a2 = root.use_component(comp(&[], &["k0"]), Rc::new(())).unwrap();
    assert!(matches!(&*a2.state(), FiberState::Active { .. }));
    assert!(
        matches!(&*b.state(), FiberState::Active { .. }),
        "A′ 激活 → B 重连"
    );
    assert!(
        matches!(&*c.state(), FiberState::Active { .. }),
        "B 激活 → C 重连"
    );
    // ── canonical：按 ⊲ 序（A→B→C）一次性装入，无卸载 ────────────────
    let rt2 = Rc::new(Runtime::new());
    let root2 = rt2.context();
    let _a = root2
        .use_component(comp(&[], &["k0"]), Rc::new(()))
        .unwrap();
    let _b = root2
        .use_component(comp(&["k0"], &["k1"]), Rc::new(()))
        .unwrap();
    let _c = root2
        .use_component(comp(&["k0", "k1"], &[]), Rc::new(()))
        .unwrap();

    // Thm 73(1) 的 "up to the names"：fiber id 是绝对名字（绝不复用，
    // 动态历史的 A′ ≠ canonical 的 A），按每个活跃 fiber 的
    // (inject, provide) 签名比较安装集（§4.4.5、Lemma 56 renaming）。
    let signature = |rt: &Runtime| -> BTreeSet<(Vec<Symbol>, Vec<Symbol>)> {
        rt.active_fibers()
            .into_iter()
            .map(|id| {
                let f = rt.fiber(id).expect("活跃集内的 fiber 必在 registry");
                let mut inject: Vec<Symbol> = f.inject().iter().collect();
                let mut provide: Vec<Symbol> = f.provide().iter().collect();
                inject.sort();
                provide.sort();
                (inject, provide)
            })
            .collect()
    };
    let dyn_sig = signature(&rt);
    let canon_sig = signature(&rt2);

    assert_eq!(dyn_sig.len(), 3, "动态历史静止态含 A′/B/C");
    assert_eq!(
        dyn_sig, canon_sig,
        "Thm 73(1)：动态历史无痕迹 = 静态装配（安装集一致，up to names）"
    );
}
