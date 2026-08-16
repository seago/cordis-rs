//! 参考解释器：论文 §4.2 基础演算的逐规则直译（PLAN §6，oracle）。
//!
//! 用途：真实引擎（PR #3 起）的 property test 以本解释器的静止状态为基准。
//! 规则即代码、不优化。调度选择固定（见 [`InterpState::step_lifecycle`]），
//! 论文本身不规定调度。
//!
//! 对抽象效应函数 `e` 的**规范化建模**（Def 69 假设）：激活恰好安装
//! `provide` 中的全部键，停用撤销之；其余上下文不变。另建模 **Def 47
//! 注册**：在父 fiber 下实例化即登记为父的注册子代，父卸载（累加器运行）
//! 时子代被 O-Retire（随父退役）。见 THEORY-MAP 已知偏差。
//!
//! 规则前提：O-Insert / O-Retire / O-Remove（编排）、L-Reload / L-Unload（生命周期）。

use std::collections::{BTreeMap, BTreeSet};

use crate::fiber::FiberId;
use crate::keyset::KeySet;
use crate::symbol::Symbol;

/// 目标视图 `ω: d → 𝔑`（Def 46）：每个声明键 → 其提供者。
pub type View = BTreeMap<Symbol, FiberId>;

/// 组件 `(d, p, e) ∈ ℭΓ`（Def 43）；效应函数 `e` 由本解释器规范化建模。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Component {
    /// 共效应规格 `d`：从环境声明的依赖。
    pub inject: KeySet,
    /// 供给 `p`：可提供的键（激活时恰好安装这些键）。
    pub provide: KeySet,
}

/// 生命周期状态 `θ`（两状态模型，图 1 / Def 44 式 (38)）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Lifecycle {
    /// `Inactive`。
    Inactive,
    /// `Active(g, ω)`：`view` 即激活时承诺的 `ω`；逆 `g` 由规范化建模隐式给出。
    Active {
        /// 激活时承诺的目标视图 `ω`。
        view: View,
    },
}

/// 编排动作（外部输入；O-Insert / O-Retire / O-Remove 的请求面）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// 插入新组件（O-Insert）。`parent = None` 表示挂到 root。
    Insert {
        /// 待插入的组件。
        component: Component,
        /// 父 fiber（`π`）。
        parent: Option<FiberId>,
    },
    /// 请求退役（O-Retire）。
    Retire {
        /// 目标 fiber。
        fiber: FiberId,
    },
    /// 移除已退役且非活跃、无子代的 fiber（O-Remove）。
    Remove {
        /// 目标 fiber。
        fiber: FiberId,
    },
}

/// 规则前提违反。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    /// O-Insert：名字已存在（本解释器名字恒新鲜，不会发生）。
    NameExists,
    /// O-Insert：`π ∉ dom(Fγ) ∪ {root}`。
    UnknownParent,
    /// O-Insert：`∃m. p ∩ p_m ≠ ∅`（单一来源纪律）。
    ProvisionClash,
    /// O-Retire / O-Remove / L-*：`n ∉ dom(Fγ)`。
    UnknownFiber,
    /// O-Remove：`τ_n ≠ ⊤`。
    NotRetired,
    /// O-Remove：`θ_n ≠ Inactive`。
    StillActive,
    /// O-Remove：`∃m. π_m = n`（须先移除子代）。
    HasChildren,
    /// L-Reload：`θ_n ≠ Inactive`。
    NotInactive,
    /// L-Reload：`target_n(γ) = ⊥`（依赖未满足或已退役）。
    NoTarget,
    /// L-Unload：`θ_n ≠ Active`。
    NotActive,
    /// L-Unload：`target_n(γ) = ω_n`（无变化）。
    ViewUnchanged,
}

/// 解释器中的 fiber（Def 44 的 `⟨d, p, e, π, σ, τ, θ⟩`；`e` 隐式）。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Fiber {
    /// `n`：fiber 名。
    pub id: FiberId,
    /// `π`：父 fiber（None = root）。
    pub parent: Option<FiberId>,
    /// `d`：共效应规格。
    pub inject: KeySet,
    /// `p`：供给。
    pub provide: KeySet,
    /// `σ`：已安装的键（激活时 = provide，停用时 = ∅）。
    pub table: KeySet,
    /// `τ`：退役标志。
    pub retired: bool,
    /// `θ`：生命周期状态。
    pub state: Lifecycle,
    /// 本 fiber 注册的子代（Def 47：注册的逆 = O-Retire，随本 fiber 的
    /// 累加器执行——本 fiber 卸载时子代被退役）。
    pub registered: Vec<FiberId>,
}

/// 系统状态 `γ`：registry `Fγ` + 名字计数器（Def 45）。
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct InterpState {
    fibers: BTreeMap<FiberId, Fiber>,
    next: u64,
}

impl InterpState {
    /// 空系统状态。
    pub fn new() -> Self {
        Self::default()
    }

    // ── 派生量（Def 45 式 (40) / Def 46）───────────────────────────────

    /// `σγ`：活跃 fiber 的表之并（Def 45 式 (40)）。
    pub fn provided(&self) -> KeySet {
        let mut out = KeySet::new();
        for f in self.fibers.values() {
            if matches!(f.state, Lifecycle::Active { .. }) {
                out.extend(f.table.iter());
            }
        }
        out
    }

    /// `provider_k(γ)`：提供 `key` 的活跃 fiber（供给不相交 ⟹ 唯一）。
    pub fn provider_of(&self, key: Symbol) -> Option<FiberId> {
        self.fibers
            .values()
            .find(|f| matches!(f.state, Lifecycle::Active { .. }) && f.table.contains(key))
            .map(|f| f.id)
    }

    /// 满足谓词 `γ ⊧ d`（Def 24，读自派生表）。
    pub fn satisfied(&self, spec: &KeySet) -> bool {
        spec.iter().all(|k| self.provider_of(k).is_some())
    }

    /// `target_n(γ)`（Def 46 式 (41)）：`⊥` 用 `None` 表示。
    pub fn target(&self, n: FiberId) -> Option<View> {
        let fiber = self.fibers.get(&n)?;
        if fiber.retired || !self.satisfied(&fiber.inject) {
            return None;
        }
        Some(
            fiber
                .inject
                .iter()
                .map(|k| (k, self.provider_of(k).expect("satisfied implies provider")))
                .collect(),
        )
    }

    /// 静止判定 `quiet(γ)`（Def 46 式 (42)）。
    pub fn is_quiet(&self) -> bool {
        self.fibers.values().all(|f| match &f.state {
            Lifecycle::Inactive => self.target(f.id).is_none(),
            Lifecycle::Active { view } => self.target(f.id).as_ref() == Some(view),
        })
    }

    // ── 编排规则（前缀 O-）──────────────────────────────────────────────

    /// O-Insert：`γ ⇒ γ[n ↦ ⟨d, p, e, π, ∅, ⊥, Inactive⟩]`。
    ///
    /// 前提：`n ∉ dom(Fγ)`（恒真，名字新鲜）、`π ∈ dom(Fγ) ∪ {root}`、
    /// `∀m ∈ dom(Fγ). p ∩ p_m = ∅`。
    pub fn insert(
        &mut self,
        parent: Option<FiberId>,
        component: &Component,
    ) -> Result<FiberId, Violation> {
        if let Some(p) = parent
            && !self.fibers.contains_key(&p)
        {
            return Err(Violation::UnknownParent);
        }
        // 供给不相交检查覆盖 dom(Fγ) 中**全部** fiber（含已退役未移除者）：
        // 与论文 O-Insert 前提 `∀m ∈ dom(Fγ). p ∩ p_m = ∅` 一致——
        // 退役组件的供给名在其被 remove 前保持占用（THEORY-MAP 已知偏差 m4）。
        if self
            .fibers
            .values()
            .any(|f| f.provide.intersects(&component.provide))
        {
            return Err(Violation::ProvisionClash);
        }
        let id = FiberId::fresh(&mut self.next);
        self.fibers.insert(
            id,
            Fiber {
                id,
                parent,
                inject: component.inject.clone(),
                provide: component.provide.clone(),
                table: KeySet::new(),
                retired: false,
                state: Lifecycle::Inactive,
                registered: Vec::new(),
            },
        );
        // Def 47：在父 fiber 下实例化 = 父的效应注册了本 fiber——
        // 登记进父的 registered（父卸载时本 fiber 被 O-Retire）。
        if let Some(p) = parent {
            self.fibers
                .get_mut(&p)
                .expect("parent existence checked")
                .registered
                .push(id);
        }
        Ok(id)
    }

    /// O-Retire：`γ ⇒ γ[τ_n ↦ ⊤]`。唯一前提 `n ∈ dom(Fγ)`；对状态无条件。
    pub fn retire(&mut self, n: FiberId) -> Result<(), Violation> {
        let f = self.fibers.get_mut(&n).ok_or(Violation::UnknownFiber)?;
        f.retired = true;
        Ok(())
    }

    /// O-Remove：`τ_n = ⊤ ∧ θ_n = Inactive ∧ ∀m. π_m ≠ n ⇒ γ ⇒ γ∖n`。
    pub fn remove(&mut self, n: FiberId) -> Result<(), Violation> {
        let f = self.fibers.get(&n).ok_or(Violation::UnknownFiber)?;
        if !f.retired {
            return Err(Violation::NotRetired);
        }
        if !matches!(f.state, Lifecycle::Inactive) {
            return Err(Violation::StillActive);
        }
        if self.fibers.values().any(|m| m.parent == Some(n)) {
            return Err(Violation::HasChildren);
        }
        self.fibers.remove(&n);
        Ok(())
    }

    // ── 生命周期规则（前缀 L-）──────────────────────────────────────────

    /// L-Reload：`θ_n = Inactive ∧ ω = target_n(γ) ≠ ⊥`。
    ///
    /// 效应建模（Def 69 假设）：`σ_n := provide`，`θ := Active(ω)`。
    pub fn reload(&mut self, n: FiberId) -> Result<(), Violation> {
        // 先校验存在性（与 unload 的错误分类一致：未知 fiber 报 UnknownFiber）。
        if !self.fibers.contains_key(&n) {
            return Err(Violation::UnknownFiber);
        }
        let Some(view) = self.target(n) else {
            return Err(Violation::NoTarget);
        };
        let f = self.fibers.get_mut(&n).expect("existence checked above");
        if !matches!(f.state, Lifecycle::Inactive) {
            return Err(Violation::NotInactive);
        }
        f.table = f.provide.clone();
        f.state = Lifecycle::Active { view };
        Ok(())
    }

    /// L-Unload：`θ_n = Active(g, ω) ∧ target_n(γ) ≠ ω`。
    ///
    /// 逆建模：`σ_n := ∅`，`θ := Inactive`。
    ///
    /// **Def 47 注册逆**：本 fiber 的累加器还包含其注册子代的 O-Retire——
    /// 卸载时全部注册子代被退役（τ := ⊤），随后由生命周期规则驱动停用。
    pub fn unload(&mut self, n: FiberId) -> Result<(), Violation> {
        let target = self.target(n);
        let f = self.fibers.get_mut(&n).ok_or(Violation::UnknownFiber)?;
        let Lifecycle::Active { view } = &f.state else {
            return Err(Violation::NotActive);
        };
        if target.as_ref() == Some(view) {
            return Err(Violation::ViewUnchanged);
        }
        f.table = KeySet::new();
        f.state = Lifecycle::Inactive;
        let registered = f.registered.clone();
        let _ = f; // 结束可变借用，再遍历注册子代
        for child in registered {
            if let Some(c) = self.fibers.get_mut(&child) {
                c.retired = true;
            }
        }
        Ok(())
    }

    // ── 调度与 oracle 接口（解释器选择，论文不规定）────────────────────

    /// 当前可启用的 L-Reload 目标（fiber id 升序）。
    pub fn enabled_reloads(&self) -> Vec<FiberId> {
        self.fibers
            .keys()
            .copied()
            .filter(|n| {
                self.target(*n).is_some()
                    && matches!(
                        self.fibers.get(n).map(|f| &f.state),
                        Some(Lifecycle::Inactive)
                    )
            })
            .collect()
    }

    /// 当前可启用的 L-Unload 目标（fiber id 升序）。
    pub fn enabled_unloads(&self) -> Vec<FiberId> {
        self.fibers
            .keys()
            .copied()
            .filter(|n| {
                matches!(&self.fibers.get(n).expect("n ∈ keys").state, Lifecycle::Active { view }
                    if self.target(*n).as_ref() != Some(view))
            })
            .collect()
    }

    /// 按 fiber id 升序执行第一个可启用的生命周期规则；无可启用则返回 `None`。
    pub fn step_lifecycle(&mut self) -> Option<FiberId> {
        for n in self.fibers.keys().copied().collect::<Vec<_>>() {
            if matches!(
                self.fibers.get(&n).map(|f| &f.state),
                Some(Lifecycle::Inactive)
            ) && self.target(n).is_some()
            {
                self.reload(n).expect("premise checked above");
                return Some(n);
            }
            if let Some(Lifecycle::Active { view }) = self.fibers.get(&n).map(|f| &f.state)
                && self.target(n).as_ref() != Some(view)
            {
                self.unload(n).expect("premise checked above");
                return Some(n);
            }
        }
        None
    }

    /// 反复执行可启用的生命周期规则直至静止（Thm 66：必然到达）。
    ///
    /// 步数上界取 `8·|Fγ| + 8`（经验值）：单个 fiber 的正常轨迹 ≤ 3 步
    /// （reload→unload→reload 一次轮换），8 倍留足裕量，超出即 panic
    /// （oracle 自身出错，测试即失败）。
    pub fn drive_to_quiescence(&mut self) {
        let limit = self.fibers.len() * 8 + 8;
        let mut steps = 0;
        while self.step_lifecycle().is_some() {
            steps += 1;
            assert!(
                steps < limit,
                "oracle failed to quiesce within {limit} steps"
            );
        }
    }

    /// 应用一个编排动作（O-规则）。
    pub fn apply(&mut self, action: &Action) -> Result<(), Violation> {
        match action {
            Action::Insert { component, parent } => self.insert(*parent, component).map(|_| ()),
            Action::Retire { fiber } => self.retire(*fiber),
            Action::Remove { fiber } => self.remove(*fiber),
        }
    }

    /// 支持集（Def 67 的活性部分）：`¬τ_n ∧ γ ⊧ d_n` 的 fiber 集。
    ///
    /// Lemma 70：静止态中支持集恰为活跃 fiber 集（在 Def 69 假设下）。
    pub fn support_set(&self) -> BTreeSet<FiberId> {
        self.fibers
            .iter()
            .filter(|(_, f)| !f.retired && self.satisfied(&f.inject))
            .map(|(id, _)| *id)
            .collect()
    }

    /// 活跃 fiber 集。
    pub fn active_set(&self) -> BTreeSet<FiberId> {
        self.fibers
            .iter()
            .filter(|(_, f)| matches!(f.state, Lifecycle::Active { .. }))
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(name: &str) -> Symbol {
        Symbol::intern(name)
    }

    fn spec(names: &[&str]) -> KeySet {
        names.iter().map(|s| k(s)).collect()
    }

    fn comp(inject: &[&str], provide: &[&str]) -> Component {
        Component {
            inject: spec(inject),
            provide: spec(provide),
        }
    }

    /// 造一个不属于任何 registry 的"幽灵"fiber 名（绕过名字分配纪律的测试专用）。
    fn ghost() -> FiberId {
        let mut counter = 0xFF00;
        FiberId::fresh(&mut counter)
    }

    /// 从状态出发按任意可启用规则分支，收集所有可达静止态（深度受限）。
    fn quiescent_states(s: &InterpState, depth: usize) -> Vec<InterpState> {
        if s.is_quiet() {
            return vec![s.clone()];
        }
        if depth == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for n in s.enabled_reloads() {
            let mut t = s.clone();
            t.reload(n).unwrap();
            out.extend(quiescent_states(&t, depth - 1));
        }
        for n in s.enabled_unloads() {
            let mut t = s.clone();
            t.unload(n).unwrap();
            out.extend(quiescent_states(&t, depth - 1));
        }
        out
    }

    #[test]
    fn dependency_activation_ordering() {
        // app 声明 db；db 尚未激活时 app 的 target = ⊥（§3.2.2 前半）。
        let mut s = InterpState::new();
        let db = s.insert(None, &comp(&[], &["db"])).unwrap();
        let app = s.insert(None, &comp(&["db"], &[])).unwrap();
        assert_eq!(s.target(app), None, "依赖未提供时 target = ⊥");
        assert!(!s.is_quiet(), "db 无依赖，其 L-Reload 已启用，系统不静止");

        // 推进：db 先激活，app 的目标随之出现并激活（L-Reload 自动触发）。
        s.drive_to_quiescence();
        assert!(s.is_quiet());
        assert_eq!(s.target(app), Some([(k("db"), db)].into_iter().collect()));
        assert_eq!(s.active_set(), BTreeSet::from([db, app]));
    }

    #[test]
    fn withdrawal_cascade() {
        // 退役 db → db 停用 → app 的依赖消失 → app 停用（§4.3.1 的顺序）。
        let mut s = InterpState::new();
        let db = s.insert(None, &comp(&[], &["db"])).unwrap();
        let app = s.insert(None, &comp(&["db"], &[])).unwrap();
        s.drive_to_quiescence();
        assert_eq!(s.active_set(), BTreeSet::from([db, app]));
        assert_eq!(s.active_set(), s.support_set(), "Lemma 70");

        s.retire(db).unwrap();
        s.drive_to_quiescence();
        assert!(s.active_set().is_empty());
        assert!(s.is_quiet());
        assert_eq!(s.provided(), KeySet::new());

        s.retire(app).unwrap();
        s.remove(db).unwrap();
        s.remove(app).unwrap();
        assert!(s.is_quiet());
        assert!(s.fibers.is_empty());
    }

    #[test]
    fn provision_clash_rejected() {
        let mut s = InterpState::new();
        s.insert(None, &comp(&[], &["db"])).unwrap();
        assert_eq!(
            s.insert(None, &comp(&[], &["db"])),
            Err(Violation::ProvisionClash)
        );
    }

    #[test]
    fn unknown_parent_rejected() {
        let mut s = InterpState::new();
        assert_eq!(
            s.insert(Some(ghost()), &comp(&[], &["x"])),
            Err(Violation::UnknownParent)
        );
    }

    #[test]
    fn lifecycle_rules_on_unknown_fiber_classify_consistently() {
        // 审查 m1：L- 规则对未知 fiber 必须统一报 UnknownFiber（而非 NoTarget）。
        let mut s = InterpState::new();
        let ghost = ghost();
        assert_eq!(s.reload(ghost), Err(Violation::UnknownFiber));
        assert_eq!(s.unload(ghost), Err(Violation::UnknownFiber));
        assert_eq!(s.retire(ghost), Err(Violation::UnknownFiber));
        assert_eq!(s.remove(ghost), Err(Violation::UnknownFiber));
    }

    #[test]
    fn remove_preconditions() {
        let mut s = InterpState::new();
        let parent = s.insert(None, &comp(&[], &["k"])).unwrap();
        let child = s.insert(Some(parent), &comp(&[], &[])).unwrap();
        s.drive_to_quiescence(); // 父（无依赖）与子（无依赖）均激活

        // 未退役不可移除
        assert_eq!(s.remove(parent), Err(Violation::NotRetired));
        // 退役但未驱动（仍 Active）→ StillActive 优先
        s.retire(parent).unwrap();
        assert_eq!(s.remove(parent), Err(Violation::StillActive));
        // 驱动后父卸载（Def 47 级联：注册子代随父退役并停用），仍有 π-子代
        s.drive_to_quiescence();
        assert_eq!(s.remove(parent), Err(Violation::HasChildren));
        assert!(
            s.fibers.get(&child).unwrap().retired,
            "子代被父的注册逆退役"
        );
        assert!(matches!(
            s.fibers.get(&child).unwrap().state,
            Lifecycle::Inactive
        ));
        // 级联后子已退役停用：先移除子（解除 π-子代关系），再移除父。
        s.remove(child).unwrap();
        s.remove(parent).unwrap();
        assert!(s.is_quiet());
    }

    #[test]
    fn remove_rejects_active_fiber() {
        // 退役但未驱动（仍 Active）不可移除（O-Remove 前提 θ = Inactive）。
        let mut s = InterpState::new();
        let a = s.insert(None, &comp(&[], &["k"])).unwrap();
        s.drive_to_quiescence(); // a Active
        s.retire(a).unwrap(); // τ 置位但尚未驱动
        assert_eq!(s.remove(a), Err(Violation::StillActive));
        s.drive_to_quiescence();
        s.remove(a).unwrap();
        assert!(s.is_quiet());
    }

    #[test]
    fn drive_reaches_quiet_and_lemma70() {
        // 随机拓扑：A 提供 k1；B 注入 k1 并提供 k2；C 注入 k2。
        let mut s = InterpState::new();
        let a = s.insert(None, &comp(&[], &["k1"])).unwrap();
        let b = s.insert(None, &comp(&["k1"], &["k2"])).unwrap();
        let c = s.insert(None, &comp(&["k2"], &[])).unwrap();
        s.drive_to_quiescence();
        assert!(s.is_quiet());
        assert_eq!(s.active_set(), s.support_set(), "Lemma 70");
        assert_eq!(s.active_set(), BTreeSet::from([a, b, c]));
        assert_eq!(s.provided(), spec(&["k1", "k2"]));

        // 退役 B：B 停用 → k2 消失 → C 停用；k1 仍由 A 提供。
        s.retire(b).unwrap();
        s.drive_to_quiescence();
        assert!(s.is_quiet());
        assert_eq!(s.active_set(), BTreeSet::from([a]));
        assert_eq!(s.active_set(), s.support_set(), "Lemma 70");
        assert_eq!(s.provided(), spec(&["k1"]));
    }

    /// 参考解释器自身的 confluence 检查（Thm 73 的小规模机器验证）：
    /// 固定动作序列下，任意生命周期交错都到达同一静止态。
    #[test]
    fn confluence_all_interleavings() {
        // 阶段 1：构建三层依赖拓扑并激活（A 供 k1，B 注入 k1 供 k2，C 注入 k2）。
        let mut s = InterpState::new();
        let a = s.insert(None, &comp(&[], &["k1"])).unwrap();
        let b = s.insert(None, &comp(&["k1"], &["k2"])).unwrap();
        let c = s.insert(None, &comp(&["k2"], &[])).unwrap();
        s.drive_to_quiescence();
        assert_eq!(s.active_set(), BTreeSet::from([a, b, c]));

        // 阶段 2：全部退役，随后任意交错（3 个 unload 任意顺序）须到达同一静止态。
        for n in [b, a, c] {
            s.retire(n).unwrap();
        }
        let states = quiescent_states(&s, 32);
        assert!(!states.is_empty(), "oracle must reach quiescence");
        let first = &states[0];
        assert!(states.iter().all(|t| t == first), "confluence violated");
        assert!(first.is_quiet());
        assert!(first.active_set().is_empty(), "全部退役后无人活跃");
        assert_eq!(first.active_set(), first.support_set(), "Lemma 70");
        assert!(
            first.fibers.contains_key(&a)
                && first.fibers.contains_key(&b)
                && first.fibers.contains_key(&c)
        );

        // 阶段 3：静止后三者均可移除，系统回到空。
        let mut s1 = first.clone();
        for n in [b, a, c] {
            s1.remove(n).unwrap();
        }
        assert!(s1.fibers.is_empty());
        assert!(s1.is_quiet());
    }
}
