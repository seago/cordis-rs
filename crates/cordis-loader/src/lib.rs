//! Cordis 声明式组件加载器（论文 §5.2.1）。
//!
//! 编排器把期望的系统组成描述为持久化配置（条目列表），加载器把配置的
//! 变化翻译为对应的 fiber 操作（§5.2.1："the loader translates changes to
//! this specification into the corresponding imperative fiber operations"）。
//!
//! **Def 74 全字段（M2-PR3，PR #19）**：`id`（协调键）/ `component`（`url`
//! 的原生版，变更即重建）/ `isolate`（local/global 托管 realm 注解，实例化
//! 期应用；**变更 = Algorithm 7 realm 重指派**（M2-PR4，不重建）/
//! `intercept`（键 → 元数据注解，**就地更新、不触发 reload**——§5.2.1 的
//! "intercept — updated in place"）/ `config`（经 `revision` 检测变更、
//! 重建）/ `disabled`（卸载/重载）。
//!
//! **配置树（M2-PR3）**：`Entry::group` / `Entry::include` 分支条目（`children`）
//! ——组持有者 fiber 上实例化子条目（`π` = 组 fiber，Def 47），子列表按
//! `id` 做 **keyed diff**（§5.2.1："updating a surviving child re-enters this
//! same per-field dispatch, so group reconciliation and entry update recurse
//! together down the tree"）；组退役/移除 → 级联拆除整棵子树（自底向上）。
//! `include` 与 `group` 结构相同（外部配置嫁接；文件解析由编排方承担）。
//!
//! **两阶段协调（审查 m1）**：`apply` 每层先做卸载侧（释放供给名）再做
//! 实例化侧——同供给键的替换可在单次 `apply` 完成。
//!
//! **已知边界（M2-PR3 记录，⑩⑪ M3-PR3 评估 + PR #29 G1 落地）**：
//! ① 双向绑定（§5.2.1 "the binding runs in both directions"）：**组件侧
//! 已落地（G1，PR #29）**——`Fiber::update` 就地重跑（fiber 身份保留、
//! 依赖者级联、失败态可复活）+ `Runtime::set_update_hook` + loader
//! `update_entry`/`register_update_hook`（条目书签写回、fiber→条目反查）。
//! **已补齐（G1 剩余，PR #30）**：组件自退役（`Fiber::retire`）→ 退役观察者
//! 写回条目 `disabled = true`（TS `internal/plugin` 半段；过滤：条目仍在且
//! 未 disabled；apply 期间 teardown 延迟排空；`loader.fiber(id)` 对退役
//! fiber 返回 None）。退役**粘滞**（无观察者时跨未变 apply 保持，见
//! `retired_component_persists_across_unchanged_apply` 测试）；desired
//! 显式 `disabled=false` 重新启用（disabled 为协调字段）；同 revision
//! apply 不清除 fiber 层写回、书签回映 desired（协调记录非权威源）；

//! ② isolate 变更经 Algorithm 7 realm 重指派（M2-PR4，就地不重建）；
//! ③ 组条目 isolate 注解——**已收口（G3，PR #31）**：per-key isolate
//!（[`Entry::isolate`]）在组条目上经派生链**拷贝继承**给子条目（组 ctx
//! 重定向被 derive 继承）、子条目自己的注解**覆盖**（最近注解优先）——
//! 无需 effective-isolate 穿透（拷贝继承免去 patch 复杂度）；组 isolate
//! 变更仍整棵重建（保守路径，`group_isolate_change_rebuilds_subtree` 直证）；
//! ④ **失败 fiber 静默加载**（审查 nit7，REVIEW-32a913d）：L-Raise 后
//! `use_component` 对组件失败返回 `Ok(fiber)`（`Inactive(Some(ζ))`）——
//! loader 不检查失败态，静默记为"已加载"；调用方需自行 `fiber.state()`
//! 判失败（loader 上报失败态 / HMR 回滚区分"加载失败"与"组件运行失败"
//! 为 M2 后续任务）；
//! ⑤ **移除条目拦截注解不回退父（组）继承拦截值**（REVIEW-24bfab5
//! major1）：`Context` 派生族对 `ι` 为扁平拷贝（无父链），子条目覆写
//! 组拦截后移除自身注解 → 该键回到无元数据状态（仅剩组件声明），而非
//! 组继承值——条目注解为权威的既定语义。

mod config;
mod patch;

pub use config::{Config, interpolate};
use config::{configs_same, validate_config};
pub use patch::{Patch, apply_patches};

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::rc::Rc;

use cordis_core::context::InterceptMeta;
use cordis_core::effect::{EffectIter, once};
use cordis_core::keyset::KeySet;
use cordis_core::symbol::Symbol;
use cordis_core::{Component, Context, Disposer, Fiber, FiberId, FiberState, Runtime};

/// 隔离注解（Def 74 的 `isolate`；§5.2.1 托管 realm 的两种作用域）。
///
/// - [`IsolateAnnotation::Local`]（`true`）：local realm——按条目 `id` 打标
///   的私有 realm，条目随迁携带；
/// - [`IsolateAnnotation::Global`]（字符串）：global realm——所有命名相同
///   字符串的条目共享该 realm（移动条目改变的是共享关系而非所属 realm）。
///
/// **per-key 应用（G3）**：只解析映射中的键（无注解键 = 裸键 realm）；
/// 组条目上的注解经派生链继承给子条目（子条目覆盖 = 最近注解优先）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IsolateAnnotation {
    /// `true`：local realm（按条目 id 打标）。
    Local,
    /// 字符串：global realm（命名共享）。
    Global(String),
}

/// 拦截注解集（Def 74 的 `intercept`：键 → 元数据，读时咨询）。
///
/// 类型擦除存储（`Box<dyn InterceptMeta>`）；[`InterceptMeta::clone_box`]
/// 支持深拷贝（条目克隆/重建复用）。
#[derive(Default)]
pub struct Intercepts(HashMap<Symbol, Box<dyn InterceptMeta>>);

impl Clone for Intercepts {
    fn clone(&self) -> Self {
        Intercepts(self.0.iter().map(|(k, v)| (*k, v.clone_box())).collect())
    }
}

impl Intercepts {
    /// 空注解集。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `key` 处的注解元数据。
    pub fn insert<M: InterceptMeta>(&mut self, key: Symbol, meta: M) {
        self.0.insert(key, Box::new(meta));
    }

    /// 迭代全部 `(键, 元数据)`。
    pub fn iter(&self) -> impl Iterator<Item = (Symbol, &dyn InterceptMeta)> {
        self.0
            .iter()
            .map(|(k, v)| (*k, v.as_ref() as &dyn InterceptMeta))
    }

    /// 已注解的键。
    pub fn keys(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.0.keys().copied()
    }

    /// 是否无注解。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `key` 是否有注解。
    pub fn contains_key(&self, key: Symbol) -> bool {
        self.0.contains_key(&key)
    }
}

impl fmt::Debug for Intercepts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.0.keys()).finish()
    }
}

/// 配置条目（Def 74 全字段，M2-PR3）：声明一个 fiber（叶子）或一组
/// 子条目（分支 = group/include）。
#[derive(Clone)]
pub struct Entry {
    /// 稳定标识——协调键（组内子列表变化时据此 diff）。
    pub id: String,
    /// 组件名（`url` 的原生版）：经 [`Loader::register_component`] 注册；
    /// 变更即重建条目（论文的 `id, url → rebuild`）。分支条目为空串。
    pub component: String,
    /// 隔离注解（Def 74 的 `isolate`；G3 per-key 粒度，TS
    /// `EntryOptions.isolate: Dict<true | string>` 参照）：**键 → 注解**，
    /// 只隔离映射中的键（混合粒度：`{val: Local, sum: Global("x")}`）；
    /// 组条目上应用后经派生链拷贝继承给子条目（子条目自己的注解覆盖 =
    /// 最近注解优先，⑪ 收口）。
    pub isolate: BTreeMap<Symbol, IsolateAnnotation>,
    /// 拦截注解（Def 74 的 `intercept`：键 → 元数据，就地更新不重建）。
    pub intercept: Intercepts,
    /// **注入携带配置**（G2，TS `EntryOptions.inject` 参照）：键 → 拦截
    /// 元数据，实例化时应用（遮蔽同键 `intercept`，TS fiber 层 inject
    /// 遮蔽 entry 层 intercept 的对应）；读取方经 [`Context::get_meta`]
    /// 右偏合并消费（Def 30/31 的 `ι(k)` 实用化）。组条目上应用后由
    /// 派生链拷贝继承给子条目。
    pub inject: Intercepts,
    /// 配置（绑定进 `apply` 形成效应函数，Algorithm 4 第 9 行）。
    /// 以 [`Rc`] 持有以便重建时复用；**变更检测依赖 [`Entry::revision`]**
    /// （配置值本身不可比较）。
    pub config: Rc<dyn Any>,
    /// 配置修订号：调用方在 `config` 变更时递增。
    pub revision: u64,
    /// 是否被管理性关闭（`disabled`）。
    pub disabled: bool,
    /// 子条目（分支：group/include；非空即分支）。
    pub children: Vec<Entry>,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("id", &self.id)
            .field("component", &self.component)
            .field("isolate", &self.isolate)
            .field("intercept", &self.intercept)
            .field("inject", &self.inject)
            .field("revision", &self.revision)
            .field("disabled", &self.disabled)
            .field("children", &self.children)
            .finish()
    }
}

impl Entry {
    /// 构造叶子条目。
    pub fn new(
        id: impl Into<String>,
        component: impl Into<String>,
        config: Rc<dyn Any>,
        revision: u64,
        disabled: bool,
    ) -> Self {
        Self {
            id: id.into(),
            component: component.into(),
            isolate: BTreeMap::new(),
            intercept: Intercepts::new(),
            inject: Intercepts::new(),
            config,
            revision,
            disabled,
            children: Vec::new(),
        }
    }

    /// 分支条目：子条目组（@cordisjs/group 等价——子列表即组的配置，
    /// 按 `id` keyed diff）。
    pub fn group(id: impl Into<String>, children: Vec<Entry>) -> Self {
        Self {
            id: id.into(),
            component: String::new(),
            isolate: BTreeMap::new(),
            intercept: Intercepts::new(),
            inject: Intercepts::new(),
            config: Rc::new(()),
            revision: 0,
            disabled: false,
            children,
        }
    }

    /// 分支条目：外部配置嫁接（@cordisjs/include 等价——与 `group` 结构
    /// 相同；外部配置文件的解析由编排方承担，此处直接嫁接条目）。
    pub fn include(id: impl Into<String>, children: Vec<Entry>) -> Self {
        Self::group(id, children)
    }

    /// 设置键的隔离注解（G3 per-key）。
    ///
    /// **变更纪律（同 config）**：已加载条目的 isolate 变更走 Algorithm 7
    /// realm 重指派（叶子）或整棵重建（组，保守路径）——不依赖 revision。
    pub fn with_isolate(mut self, key: Symbol, isolate: IsolateAnnotation) -> Self {
        self.isolate.insert(key, isolate);
        self
    }

    /// 设置拦截注解。
    pub fn with_intercept<M: InterceptMeta>(mut self, key: Symbol, meta: M) -> Self {
        self.intercept.insert(key, meta);
        self
    }

    /// 设置注入携带配置（G2：TS `EntryOptions.inject` 参照）——实例化时
    /// 应用并**遮蔽同键**拦截注解（TS fiber 层 inject 遮蔽 entry 层
    /// intercept）；读取方经 [`Context::get_meta`] 右偏合并消费。
    ///
    /// **变更纪律（同 config）**：值不可比较，已加载条目的 inject 变更须
    /// 随 `revision` 递增触发重建（reconcile 不感知本字段）。
    pub fn with_inject<M: InterceptMeta>(mut self, key: Symbol, meta: M) -> Self {
        self.inject.insert(key, meta);
        self
    }

    /// 是否为分支（group/include）。
    pub fn is_group(&self) -> bool {
        !self.children.is_empty()
    }
}

/// 已加载条目的运行时状态（含子树）。
#[derive(Clone)]
struct LoadedEntry {
    component: String,
    config: Rc<dyn Any>,
    revision: u64,
    disabled: bool,
    isolate: BTreeMap<Symbol, IsolateAnnotation>,
    intercept: Intercepts,
    /// 条目上下文（注解派生；Algorithm 7 重指派时就地 patch ρ，
    /// M2-PR4）。
    ctx: Rc<Context>,
    /// 实例化的 fiber（组 = 持有者 fiber；`disabled` 或尚未满足依赖时为
    /// `None`）。
    fiber: Option<Rc<Fiber>>,
    /// 子条目（组持有者 fiber 之下）。
    children: HashMap<String, LoadedEntry>,
}

/// 声明式加载器：维护 `id → fiber` 的映射，把配置变化增量协调到运行时。
pub struct Loader {
    runtime: Rc<Runtime>,
    root: Rc<Context>,
    components: RefCell<HashMap<String, Rc<dyn Component>>>,
    entries: RefCell<HashMap<String, LoadedEntry>>,
    /// G7 配置协议注册表（类型 → cast；`register_config` 注册）。
    config_casts: RefCell<HashMap<std::any::TypeId, config::ConfigCast>>,
    /// 退役写回 pending 队列（apply 期间 teardown 触发的 retire 延迟处理——
    /// hook 不能重借 entries；apply 末尾排空）。
    retire_pending: RefCell<Vec<FiberId>>,
    /// 是否正处于 apply 协调中（hook 据此选择 pending 或直写）。
    in_apply: Cell<bool>,
}

impl Loader {
    /// 在共享运行时上创建加载器（条目实例化于 root 上下文）。
    pub fn new(runtime: Rc<Runtime>) -> Self {
        let root = runtime.context();
        Self {
            runtime,
            root,
            components: RefCell::new(HashMap::new()),
            entries: RefCell::new(HashMap::new()),
            config_casts: RefCell::new(HashMap::new()),
            retire_pending: RefCell::new(Vec::new()),
            in_apply: Cell::new(false),
        }
    }

    /// 注册组件（`Entry::component` 名的解析表，`url` 的原生版）。
    pub fn register_component(&self, name: impl Into<String>, component: Rc<dyn Component>) {
        self.components.borrow_mut().insert(name.into(), component);
    }

    /// 协调：以 `desired` 为权威记录，增量对齐（幂等，递归整棵配置树）。
    ///
    /// - 新增条目 → 实例化（除非 `disabled`）；
    /// - 消失条目 → 拆除（退役、卸载并移除 fiber，释放供给名）；
    /// - `disabled` 切换 → 卸载 / 重载；
    /// - `component` / `revision` 变更 → 重建；`isolate` 变更 →
    ///   Algorithm 7 realm 重指派（M2-PR4，就地不重建）；
    /// - `intercept` 变更 → **就地**更新（`intercept_set_boxed`/
    ///   `intercept_clear`，不触发 reload——§5.2.1 "intercept — updated
    ///   in place"）；
    /// - 分支（group/include）子列表 → 按 `id` keyed diff 递归。
    ///
    /// **两阶段执行**（审查 m1，每层递归同样适用）：先做卸载侧（移除消失
    /// 条目、卸载 `disabled` 置位与需重建条目的旧 fiber，释放供给名），
    /// 再做实例化侧——保证**同供给键的替换**（desired 用条目 Y 替换提供
    /// 同一键的 X）可在单次 `apply` 完成，否则 Y 实例化时命中 X 的供给
    /// 检查而 `ProvisionClash`。
    ///
    /// 组件名未注册或供给冲突（两个**同时存在**的条目提供同一键）→ panic
    /// （配置错误，panic = bug）。
    ///
    /// `desired` 内重复 `id` 未定义（按 last-wins 处理，可能浪费一次
    /// 实例化）；调用方应保证 `id` 在整棵树内唯一。
    pub fn apply(&self, desired: &[Entry]) {
        self.in_apply.set(true);
        self.apply_into(
            desired,
            Rc::clone(&self.root),
            &mut self.entries.borrow_mut(),
        );
        self.in_apply.set(false);
        // 注（REVIEW-460d8d0 nit）：apply_into 内 panic（配置错误）时不复位
        // in_apply——panic = 宿主 bug（进程 unwind），退役观察者不再依赖，
        // 延迟写回随之丢弃，可接受。
        // 排空退役写回（apply 期间 teardown 触发的 retire 延迟到协调结束后，
        // 避免 hook 重借 entries）。
        let pending = std::mem::take(&mut *self.retire_pending.borrow_mut());
        for fid in pending {
            self.writeback_retire(fid);
        }
    }

    /// 注册名对应的组件实例（HMR 备份/恢复用，M2-PR5）。
    pub fn component_of(&self, name: &str) -> Option<Rc<dyn Component>> {
        self.components.borrow().get(name).cloned()
    }

    /// **双向绑定条目侧写回**（§5.2.1 "the binding runs in both
    /// directions"；TS loader `internal/update` 钩子参照，loader/index.ts:74）：
    /// 就地更新已加载条目的 config 并对条目 fiber 做**就地重跑**（[`Fiber::update`]，
    /// 身份保留、依赖者级联），不递增 revision——协调键不变 ⟹ 后续同
    /// revision 的 apply 不重建（写回不被清除）。调用方作为 desired 树的
    /// 所有者自行决定持久化。
    pub fn update_entry(&self, id: &str, config: Rc<dyn Any>) {
        // 1. 条目书签（供后续协调可见；整棵树递归查找）。
        if let Some(l) = find_loaded_mut(&mut self.entries.borrow_mut(), id) {
            l.config = Rc::clone(&config);
        }
        // 2. 条目 fiber 就地重跑（Active 时；revision 未变，fiber 保留）。
        if let Some(fiber) = self.fiber(id)
            && !fiber.retired()
            && matches!(&*fiber.state(), FiberState::Active { .. })
        {
            fiber.update(config);
        }
    }

    /// 已加载条目当前 config（双向绑定写回后可查询；None = 条目不存在）。
    /// 整棵树递归查找。
    pub fn entry_config(&self, id: &str) -> Option<Rc<dyn Any>> {
        find_loaded(&self.entries.borrow(), id).map(|l| Rc::clone(&l.config))
    }

    /// 已加载条目当前 disabled 书签（自退役写回后可查询；None = 条目不存在）。
    /// 整棵树递归查找。
    pub fn entry_disabled(&self, id: &str) -> Option<bool> {
        find_loaded(&self.entries.borrow(), id).map(|l| l.disabled)
    }

    /// 注册 config 类型 `C` 的 [`Config`] 协议（G7）：启用该校验与值级
    /// diff（`Config::validate` 失败 → apply panic；`Config::same` 为真 →
    /// revision 递增免重建）。
    ///
    /// **HMR 兼容纪律**：见 [`Config`] 文档——实现 `same` 须承诺同值 =
    /// 无重载需求（cordis-hmr 以 revision 递增 + 复用旧 config 触发重建）。
    pub fn register_config<C: Config + 'static>(&self) {
        config::register_config_cast::<C>(&mut self.config_casts.borrow_mut());
    }

    /// 注册更新观察者（loader 侧写回通道）：把 [`Runtime::set_update_hook`]
    /// 接到本 loader——组件侧 [`Fiber::update`] 触发时，自动把新 config 写入
    /// 该 fiber 所属条目的书签（TS `internal/update` 的 loader 半段）。
    ///
    /// 需要 `Rc<Loader>`（观察者闭包持有 loader 引用）。
    pub fn register_update_hook(self: &Rc<Self>) {
        let loader = Rc::clone(self);
        self.runtime
            .set_update_hook(Some(Rc::new(move |fiber: &Fiber, config: Rc<dyn Any>| {
                if let Some(id) = loader.entry_of(fiber.id())
                    && let Some(l) = find_loaded_mut(&mut loader.entries.borrow_mut(), &id)
                {
                    l.config = Rc::clone(&config);
                }
            })));
    }

    /// 注册退役观察者（§5.2.1 双向绑定条目侧；TS `internal/plugin` 半段
    /// 参照，loader/index.ts:88-124）：组件自退役（[`Fiber::retire`]）→
    /// 自动写回所属条目书签 `disabled = true`。
    ///
    /// **过滤语义**：任何 retire 均触发（含 loader 驱动 teardown）——仅当
    /// 条目**仍在且未 disabled**（= 组件自退役；loader 驱动的退役发生时
    /// 条目已被移除或已置 disabled）才写回。
    ///
    /// **协调语义**：`disabled` 是协调字段——desired 显式 `disabled=false`
    /// 的 apply 会重新启用（与 update 写回不同：config 非协调字段、同
    /// revision apply 不清除）。编排方作为树所有者决定持久化。
    pub fn register_retire_hook(self: &Rc<Self>) {
        let loader = Rc::clone(self);
        self.runtime
            .set_retire_hook(Some(Rc::new(move |fiber: &Fiber| {
                if loader.in_apply.get() {
                    // apply 协调期间（teardown 路径）：entries 已被 apply_into
                    // 可变借用——延迟到 apply 末尾排空。
                    loader.retire_pending.borrow_mut().push(fiber.id());
                } else {
                    // 组件自退役（apply 之外）：立即写回。
                    loader.writeback_retire(fiber.id());
                }
            })));
    }

    /// 退役写回（TS `internal/plugin` 半段）：fiber 所属条目**仍在且未
    /// disabled**（= 组件自退役）→ 书签 `disabled = true`。loader 驱动的
    /// 退役发生时条目已被移除或已置位，`entry_of`/`!l.disabled` 自然过滤。
    fn writeback_retire(&self, fid: FiberId) {
        if let Some(id) = self.entry_of(fid)
            && let Some(l) = find_loaded_mut(&mut self.entries.borrow_mut(), &id)
            && !l.disabled
        {
            l.disabled = true;
        }
    }

    /// 条目当前 fiber（组 = 持有者 fiber；未加载 / 已卸载 / 未满足依赖时为
    /// `None`）。整棵树递归查找。
    pub fn fiber(&self, id: &str) -> Option<Rc<Fiber>> {
        self.find_fiber(id, &self.entries.borrow())
    }

    fn find_fiber(&self, id: &str, map: &HashMap<String, LoadedEntry>) -> Option<Rc<Fiber>> {
        if let Some(loaded) = map.get(id) {
            // 退役 = 已卸载（自退役写回后 LoadedEntry 仍持引用供 disabled
            // 清除路径清理 registry，但查询语义为 None）。
            return loaded.fiber.clone().filter(|f| !f.retired());
        }
        map.values().find_map(|l| self.find_fiber(id, &l.children))
    }

    /// 反查 fiber → 条目 id（写回映射；整棵树递归）。
    fn entry_of(&self, fid: FiberId) -> Option<String> {
        self.entry_of_in(&fid, &self.entries.borrow())
    }

    fn entry_of_in(&self, fid: &FiberId, map: &HashMap<String, LoadedEntry>) -> Option<String> {
        for (id, l) in map {
            if l.fiber.as_ref().is_some_and(|f| f.id() == *fid) {
                return Some(id.clone());
            }
            if let Some(found) = self.entry_of_in(fid, &l.children) {
                return Some(found);
            }
        }
        None
    }

    /// 已加载条目数（整棵树）。
    pub fn len(&self) -> usize {
        self.count(&self.entries.borrow())
    }

    fn count(&self, map: &HashMap<String, LoadedEntry>) -> usize {
        map.values()
            .fold(0, |acc, l| acc + 1 + self.count(&l.children))
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    // ── 内部：递归协调 ────────────────────────────────────────────────

    /// 在 `parent_ctx` 下协调一层条目（顶层 = root；组内 = 组持有者 ctx）。
    fn apply_into(
        &self,
        desired: &[Entry],
        parent_ctx: Rc<Context>,
        loaded: &mut HashMap<String, LoadedEntry>,
    ) {
        // 阶段 1（卸载侧）：先释放供给名，为阶段 2 的同供给替换腾位。
        let current: Vec<String> = loaded.keys().cloned().collect();
        for id in current {
            let Some(entry) = desired.iter().rev().find(|e| e.id == id) else {
                self.unload_from(&id, loaded);
                continue;
            };
            let Some(l) = loaded.get(&id).cloned() else {
                continue;
            };
            let disabling = !l.disabled && entry.disabled;
            // G7：revision 递增但 config 值级相等（Config::same opt-in）→
            // 免重建（TS deepEqual 同型；未实现 same 的类型保守走 revision）。
            let rebuilding = !entry.disabled
                && (l.component != entry.component
                    || (l.revision != entry.revision
                        && !configs_same(
                            &self.config_casts.borrow(),
                            l.config.as_ref(),
                            entry.config.as_ref(),
                        )));
            // 注（M2-PR4）：isolate 变更**不走卸载侧**——经 Algorithm 7
            // realm 重指派（patch_isolation，reconcile_into 内处理）。
            if disabling || rebuilding {
                self.unload_from(&id, loaded);
            }
        }

        // 阶段 2（实例化侧）：新增 / disabled 清除 / 重建；未变条目零操作。
        for entry in desired {
            match loaded.get(&entry.id).cloned() {
                None => {
                    let fresh = self.make_loaded(entry, &parent_ctx);
                    loaded.insert(entry.id.clone(), fresh);
                }
                Some(l) => self.reconcile_into(entry, &l, &parent_ctx, loaded),
            }
        }
    }

    /// 逐字段最小扰动（§5.2.1 的 per-field dispatch），递归子树。
    fn reconcile_into(
        &self,
        entry: &Entry,
        loaded: &LoadedEntry,
        parent_ctx: &Rc<Context>,
        map: &mut HashMap<String, LoadedEntry>,
    ) {
        if loaded.disabled != entry.disabled {
            if entry.disabled {
                // disabled 置位：拆除整棵子树（组 → 子代级联）。
                self.unload_from(&entry.id, map);
                if let Some(l) = map.get_mut(&entry.id) {
                    l.disabled = true;
                }
            } else {
                // disabled 清除：以 desired 重新实例化（叶子或整棵分支）。
                // 自退役写回（G1 剩余）后旧 fiber 已退役未移除——先拆除
                // 释放供给名（退役 fiber 仍在 registry，直接实例化会
                // ProvisionClash），再重实例化。
                if let Some(fiber) = map.get(&entry.id).and_then(|l| l.fiber.clone())
                    && fiber.retired()
                {
                    self.unload_from(&entry.id, map);
                }
                let fresh = self.make_loaded(entry, parent_ctx);
                map.insert(entry.id.clone(), fresh);
            }
            return;
        }
        if entry.disabled {
            return; // 禁用且状态未变
        }

        if entry.is_group() {
            // 组自身 isolate 变更 → 整棵重建（保守路径，G3 收口后组
            // isolate 已应用——重建使子条目按新 realm 重装；叶子条目的
            // isolate 变更走 Algorithm 7 重指派）。
            if loaded.isolate != entry.isolate {
                self.unload_from(&entry.id, map);
                let fresh = self.make_loaded(entry, parent_ctx);
                map.insert(entry.id.clone(), fresh);
                return;
            }
            let holder = map
                .get(&entry.id)
                .and_then(|l| l.fiber.clone())
                .expect("组已启用则有持有者 fiber");
            if let Some(l) = map.get_mut(&entry.id) {
                if let Some(fiber) = &l.fiber {
                    self.apply_intercept(fiber.ctx(), &l.intercept, &entry.intercept);
                }
                l.intercept = entry.intercept.clone();
                // 防呆记录（REVIEW-24bfab5 nit7）：revision 变更已由阶段一
                // 兜底整棵重建，此处为等价赋值；config 供记录。
                l.config = Rc::clone(&entry.config);
                l.revision = entry.revision;
            }
            // 子列表 keyed diff（两阶段递归；幸存子条目不重建）。
            let holder_ctx = holder.ctx().clone();
            let children = &mut map.get_mut(&entry.id).expect("组条目存在").children;
            self.apply_into(&entry.children, holder_ctx, children);
            return;
        }

        // 叶子：component / revision 变更 → 重建（防御性分支——阶段一
        // 已对重建条件卸载，此处在 reconcile 路径实际不可达，REVIEW-
        // 24bfab5 nit3）；isolate 变更 → **Algorithm 7 realm 重指派**
        //（M2-PR4，替代重建）。
        if loaded.component != entry.component
            || (loaded.revision != entry.revision
                && !configs_same(
                    &self.config_casts.borrow(),
                    loaded.config.as_ref(),
                    entry.config.as_ref(),
                ))
        {
            self.unload_from(&entry.id, map);
            let fresh = self.make_loaded(entry, parent_ctx);
            map.insert(entry.id.clone(), fresh);
            return;
        }
        if loaded.isolate != entry.isolate {
            self.patch_isolation(entry, loaded, map);
            return;
        }
        // 仅拦截注解变化：就地应用（不重建）。
        if let Some(l) = map.get_mut(&entry.id) {
            if let Some(fiber) = &l.fiber {
                self.apply_intercept(fiber.ctx(), &l.intercept, &entry.intercept);
            }
            l.intercept = entry.intercept.clone();
            l.config = Rc::clone(&entry.config);
        }
    }

    /// 构造（并加载）条目：叶子实例化组件；分支实例化持有者 + 递归子条目。
    fn make_loaded(&self, entry: &Entry, parent_ctx: &Rc<Context>) -> LoadedEntry {
        let ctx = self.entry_ctx(entry, parent_ctx);
        if entry.is_group() {
            let mut children = HashMap::new();
            let fiber = if entry.disabled {
                None
            } else {
                let holder = self.instantiate_group(entry, &ctx);
                self.apply_into(&entry.children, holder.ctx().clone(), &mut children);
                Some(holder)
            };
            LoadedEntry {
                component: entry.component.clone(),
                config: Rc::clone(&entry.config),
                revision: entry.revision,
                disabled: entry.disabled,
                isolate: entry.isolate.clone(),
                intercept: entry.intercept.clone(),
                ctx,
                fiber,
                children,
            }
        } else {
            let fiber = if entry.disabled {
                None
            } else {
                Some(self.instantiate_leaf(entry, &ctx))
            };
            LoadedEntry {
                component: entry.component.clone(),
                config: Rc::clone(&entry.config),
                revision: entry.revision,
                disabled: entry.disabled,
                isolate: entry.isolate.clone(),
                intercept: entry.intercept.clone(),
                ctx,
                fiber,
                children: HashMap::new(),
            }
        }
    }

    /// 条目上下文：叶子 = 注解 ctx（isolate 派生 + intercept 派生）；
    /// 分支 = 派生 + 组拦截注解（isolate 无声明键可应用，M2-PR3 边界）。
    fn entry_ctx(&self, entry: &Entry, parent_ctx: &Rc<Context>) -> Rc<Context> {
        if entry.is_group() {
            let mut ctx = parent_ctx.derive();
            for (key, meta) in entry.intercept.iter() {
                ctx.intercept_set_boxed(key, meta.clone_box());
            }
            // G2：组注入携带配置——子条目 ctx 经 derive 拷贝继承。
            for (key, meta) in entry.inject.iter() {
                ctx.intercept_set_boxed(key, meta.clone_box());
            }
            // G3：组 per-key isolate——组 ctx 重定向经 derive 拷贝传给
            // 子条目（子条目自己的注解覆盖 = 最近注解优先，⑪ 收口）。
            for (key, iso) in entry.isolate.iter() {
                let realm = match iso {
                    IsolateAnnotation::Local => {
                        Symbol::intern(&format!("local:{}:{}", entry.id, key.as_str()))
                    }
                    IsolateAnnotation::Global(name) => {
                        Symbol::intern(&format!("global:{name}:{}", key.as_str()))
                    }
                };
                ctx = ctx.isolate(*key, realm);
            }
            ctx
        } else {
            // 查表副作用 = 未注册 panic；绑定不再用于注解（G3 per-key 后
            // isolate 不再需要 keys）。
            let _component = self
                .components
                .borrow()
                .get(&entry.component)
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "未注册的组件 `{}`（先 register_component）",
                        entry.component
                    )
                });
            self.annotated_ctx(parent_ctx, entry)
        }
    }

    /// 叶子实例化：在注解 ctx 上注册组件；若父为 fiber（组内），在父 ctx
    /// 注册级联退役（Def 47 注册逆——父退役 → O-Retire 本 fiber）。
    fn instantiate_leaf(&self, entry: &Entry, ctx: &Rc<Context>) -> Rc<Fiber> {
        validate_config(
            &self.config_casts.borrow(),
            entry.config.as_ref(),
            &entry.id,
        );
        let component = self
            .components
            .borrow()
            .get(&entry.component)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "未注册的组件 `{}`（先 register_component）",
                    entry.component
                )
            });
        let fiber = ctx
            .use_component(component, Rc::clone(&entry.config))
            .unwrap_or_else(|err| panic!("条目 `{}` 实例化失败：{err:?}（配置错误）", entry.id));
        // 级联：父 fiber 退役 → 子代 O-Retire（Def 47；经父 ctx 累加器）。
        // 注（REVIEW-24bfab5 nit5）：`register` 的 retire 逆落在**派生 ctx**
        //（annotated_ctx）的累加器上——孤儿（从不执行）；此处的**显式父
        // ctx 效应**才是组退役级联的真正通道（retire 幂等，双路径安全）。
        if ctx.fiber().is_some() {
            drop(ctx.effect(|| -> Box<dyn EffectIter> {
                let f = Rc::clone(&fiber);
                Box::new(once(Box::new(move || {
                    Box::new(move || f.retire()) as Disposer
                })))
            }));
        }
        fiber
    }

    /// 组持有者实例化：无注入/供给的空组件（组的角色 = 子条目的父 fiber，
    /// Def 47 注册）。
    fn instantiate_group(&self, entry: &Entry, parent_ctx: &Rc<Context>) -> Rc<Fiber> {
        validate_config(
            &self.config_casts.borrow(),
            entry.config.as_ref(),
            &entry.id,
        );
        // 组拦截/注入/isolate 注解经 annotated_ctx 应用（G3：per-key
        // isolate 重定向组 ctx——子条目经 derive 拷贝继承；REVIEW-24bfab5
        // nit4：复用而非手工复刻）。
        let ctx = self.annotated_ctx(parent_ctx, entry);
        let holder = ctx
            .use_component(Rc::new(GroupHolder), Rc::clone(&entry.config))
            .unwrap_or_else(|err| panic!("组条目 `{}` 实例化失败：{err:?}（配置错误）", entry.id));
        // 组持有者也注册级联（组在组内时）。
        if ctx.fiber().is_some() {
            drop(ctx.effect(|| -> Box<dyn EffectIter> {
                let f = Rc::clone(&holder);
                Box::new(once(Box::new(move || {
                    Box::new(move || f.retire()) as Disposer
                })))
            }));
        }
        holder
    }

    /// 注解 ctx：派生（隔离父上下文）→ isolate（派生）→ intercept
    /// （类型擦除替换，条目注解为权威）。
    fn annotated_ctx(&self, parent: &Rc<Context>, entry: &Entry) -> Rc<Context> {
        let mut ctx = parent.derive();
        // G3 per-key isolate：只隔离映射中的键（TS `isolate: Dict` 参照）。
        for (key, iso) in entry.isolate.iter() {
            let realm = match iso {
                IsolateAnnotation::Local => {
                    Symbol::intern(&format!("local:{}:{}", entry.id, key.as_str()))
                }
                IsolateAnnotation::Global(name) => {
                    Symbol::intern(&format!("global:{name}:{}", key.as_str()))
                }
            };
            ctx = ctx.isolate(*key, realm);
        }
        for (key, meta) in entry.intercept.iter() {
            ctx.intercept_set_boxed(key, meta.clone_box());
        }
        // G2 注入携带配置：后应用 ⟹ 同键遮蔽 intercept（TS 分层同序）。
        for (key, meta) in entry.inject.iter() {
            ctx.intercept_set_boxed(key, meta.clone_box());
        }
        ctx
    }

    /// 拦截注解就地差异应用（不重建）：desired 键 → `intercept_set_boxed`
    /// （替换、重放幂等）；消失键 → `intercept_clear`。
    fn apply_intercept(&self, ctx: &Rc<Context>, previous: &Intercepts, desired: &Intercepts) {
        for (key, meta) in desired.iter() {
            ctx.intercept_set_boxed(key, meta.clone_box());
        }
        for key in previous.keys() {
            if !desired.contains_key(key) {
                ctx.intercept_clear(key);
            }
        }
    }

    /// **Algorithm 7：隔离 realm 重指派（M2-PR4，替代 isolate 变更重建）**
    ///
    /// 论文用 delimiter（`δk` 标签）判定"绑定是否属本条目"（own）；本实现
    /// 以 **loader 树的子树成员关系**等价判定（条目 fiber + 递归子代）——
    /// 适应记录见 THEORY-MAP PR #20 行。
    ///
    /// 步骤（对照 Algorithm 7）：Δ = 变化键（旧 realm vs 新 realm）→
    /// patch `ρ`（条目 ctx + 子树各 fiber ctx——ρ 表为拷贝继承，需遍历；
    /// 论文为持久化结构共享）→ refresh 子树 fiber（重算目标）→ 移动
    /// 子树绑定（own ∧ store[s1] ∧ ¬store[s2]）→ 通知 affected 外部依赖者
    /// （resolve ∈ {s1,s2} ∧ own(D) ≠ own(P)）。
    fn patch_isolation(
        &self,
        entry: &Entry,
        loaded: &LoadedEntry,
        map: &mut HashMap<String, LoadedEntry>,
    ) {
        let fiber = loaded.fiber.as_ref().expect("patch 仅在已激活条目");
        let component = fiber.component();
        // Δ 键域：组件声明键 ∪ 新旧 isolate 映射键（G3 per-key）。
        let mut keys: Vec<Symbol> = component
            .inject()
            .iter()
            .chain(component.provide().iter())
            .collect();
        keys.extend(entry.isolate.keys().copied());
        keys.extend(loaded.isolate.keys().copied());
        keys.sort_unstable();
        keys.dedup();

        // Δ：变化键（s1 = 旧 realm，s2 = 新 realm）。
        let mut diff: Vec<(Symbol, Symbol, Symbol)> = Vec::new();
        for key in keys {
            let s1 = self.realm_of(&loaded.isolate, &entry.id, key);
            let s2 = self.realm_of(&entry.isolate, &entry.id, key);
            if s1 != s2 {
                diff.push((key, s1, s2));
            }
        }
        if diff.is_empty() {
            self.update_leaf_fields(entry, loaded, map);
            return;
        }

        // 子树成员（own 判定；论文 delimiter 的树等价）。
        let subtree = self.collect_subtree_ids(loaded);

        // patch ρ：条目 ctx + 子树各 fiber ctx（拷贝继承 → 遍历）。
        for (key, _, s2) in &diff {
            loaded.ctx.isolate_in_place(*key, *s2);
            for fid in &subtree {
                if let Some(f) = self.runtime.fiber(*fid) {
                    f.ctx().isolate_in_place(*key, *s2);
                }
            }
        }

        // refresh 子树 fiber（目标重算：重绑 s2 / 卸载）。
        for fid in &subtree {
            if let Some(f) = self.runtime.fiber(*fid) {
                self.runtime.refresh(&f);
            }
        }

        // 移动子树绑定（own ∧ store[s1] ∧ ¬store[s2]）；own 判定在移动
        // **之前**快照（移动后 s1 无绑定，affected 谓词需要迁移前事实）。
        let prov_owns: Vec<bool> = diff
            .iter()
            .map(|(_, s1, _)| {
                self.runtime
                    .provider_of_realm(*s1)
                    .is_some_and(|p| subtree.contains(&p))
            })
            .collect();
        for ((_, s1, s2), own) in diff.iter().zip(prov_owns.iter()) {
            if *own && !self.runtime.store().contains(*s2) {
                self.runtime
                    .move_binding(*s1, *s2)
                    .unwrap_or_else(|err| panic!("Algorithm 7 绑定迁移失败：{err:?}（bug）"));
            }
        }

        // 通知 affected 外部依赖者（排除子树成员——已 patch + refresh）：
        // resolve ∈ {s1,s2} ∧ own(D) ≠ own(P)（外部 own(D)=false）。
        let diff_clone = diff.clone();
        self.runtime.notify_affected(move |f| {
            if subtree.contains(&f.id()) {
                return false;
            }
            diff_clone
                .iter()
                .zip(prov_owns.iter())
                .any(|((key, s1, s2), own)| {
                    let r = f.ctx().realm_of(*key);
                    (r == *s1 || r == *s2) && *own
                })
        });

        self.update_leaf_fields(entry, loaded, map);
    }

    /// 叶子字段同步（patch_isolation 收尾；拦截注解就地差异应用）。
    fn update_leaf_fields(
        &self,
        entry: &Entry,
        loaded: &LoadedEntry,
        map: &mut HashMap<String, LoadedEntry>,
    ) {
        if let Some(l) = map.get_mut(&entry.id) {
            l.component = entry.component.clone();
            l.config = Rc::clone(&entry.config);
            l.revision = entry.revision;
            l.isolate = entry.isolate.clone();
            l.intercept = entry.intercept.clone();
            if let Some(fiber) = &l.fiber {
                self.apply_intercept(fiber.ctx(), &loaded.intercept, &entry.intercept);
            }
        }
    }

    /// isolate 注解 → 键的 realm 符号。
    fn realm_of(
        &self,
        isolate: &BTreeMap<Symbol, IsolateAnnotation>,
        id: &str,
        key: Symbol,
    ) -> Symbol {
        match isolate.get(&key) {
            None => key,
            Some(IsolateAnnotation::Local) => {
                Symbol::intern(&format!("local:{id}:{}", key.as_str()))
            }
            Some(IsolateAnnotation::Global(name)) => {
                Symbol::intern(&format!("global:{name}:{}", key.as_str()))
            }
        }
    }

    /// 条目子树 fiber id 集合（own 判定）：叶子 = 条目 fiber 自身；
    /// 分支（组）= 持有者 fiber + 递归子代。
    fn collect_subtree_ids(&self, loaded: &LoadedEntry) -> std::collections::HashSet<FiberId> {
        let mut ids = std::collections::HashSet::new();
        self.collect_subtree_ids_into(loaded, &mut ids);
        ids
    }

    fn collect_subtree_ids_into(
        &self,
        loaded: &LoadedEntry,
        ids: &mut std::collections::HashSet<FiberId>,
    ) {
        if let Some(fiber) = &loaded.fiber {
            ids.insert(fiber.id());
        }
        for child in loaded.children.values() {
            self.collect_subtree_ids_into(child, ids);
        }
    }

    /// 拆除条目（退役 + 移除出 registry）——组先退役持有者（级联 O-Retire
    /// 子代），再自底向上移除子代（O-Remove 的 HasChildren 前提），最后
    /// 移除持有者。
    fn unload_from(&self, id: &str, map: &mut HashMap<String, LoadedEntry>) {
        if let Some(mut loaded) = map.remove(id) {
            self.teardown(&mut loaded);
        }
    }

    fn teardown(&self, loaded: &mut LoadedEntry) {
        let fiber = loaded.fiber.take();
        let fid = fiber.as_ref().map(|f| f.id());
        if let Some(fiber) = &fiber {
            fiber.retire(); // 同步卸载（级联依赖者；组 → 子代经 ctx 累加器）
        }
        let children = std::mem::take(&mut loaded.children);
        for (_, mut child) in children {
            self.teardown(&mut child);
        }
        if let Some(fid) = fid {
            self.runtime
                .remove_fiber(fid)
                .unwrap_or_else(|err| panic!("条目移除失败：{err:?}（子树拆除顺序错误 = bug）"));
        }
    }
}

/// 组持有者组件：无注入/供给、无效应——角色 = 子条目的父 fiber（Def 47
/// 注册的承载者）。
struct GroupHolder;

impl Component for GroupHolder {
    fn inject(&self) -> KeySet {
        KeySet::new()
    }
    fn provide(&self) -> KeySet {
        KeySet::new()
    }
    fn apply(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(once(Box::new(|| Box::new(|| {}) as Disposer)))
    }
}

/// 递归查找已加载条目（整棵树；`update_entry`/`entry_config` 用）。
fn find_loaded<'a>(map: &'a HashMap<String, LoadedEntry>, id: &str) -> Option<&'a LoadedEntry> {
    if let Some(l) = map.get(id) {
        return Some(l);
    }
    map.values().find_map(|l| find_loaded(&l.children, id))
}

fn find_loaded_mut<'a>(
    map: &'a mut HashMap<String, LoadedEntry>,
    id: &str,
) -> Option<&'a mut LoadedEntry> {
    if map.contains_key(id) {
        return map.get_mut(id);
    }
    for l in map.values_mut() {
        if let Some(found) = find_loaded_mut(&mut l.children, id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use cordis_core::{EffectIter, FiberState, InterceptMeta, Key, KeySet, Symbol, once};
    use std::collections::BTreeSet;

    struct ValKey;
    impl Key for ValKey {
        type Value = String;
        const SYMBOL: &'static str = "val";
    }

    struct SumKey;
    impl Key for SumKey {
        type Value = String;
        const SYMBOL: &'static str = "sum";
    }

    fn spec(names: &[&str]) -> KeySet {
        names.iter().map(|s| Symbol::intern(s)).collect()
    }

    /// 测试组件：提供 `val`，值取自配置（`config.downcast_ref::<String>()`）。
    fn val_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["val"]),
            effects: Box::new(|ctx, config| {
                let value = config
                    .downcast_ref::<String>()
                    .expect("val_provider 的 config 为 String")
                    .clone();
                Box::new(once(Box::new(move || {
                    ctx.set::<ValKey>(value).expect("绑定 val")
                })))
            }),
        })
    }

    /// 测试组件：提供 `val`，绑定固定值（不读 config——用于区分组件身份）。
    fn fixed_val_provider(value: &str) -> Rc<TestComponent> {
        let value = value.to_string();
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["val"]),
            effects: Box::new(move |ctx, _config| {
                let value = value.clone();
                Box::new(once(Box::new(move || {
                    ctx.set::<ValKey>(value).expect("绑定 val")
                })))
            }),
        })
    }

    /// 测试组件：注入 `val`，读取后绑定 `sum`。
    fn sum_consumer() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&["val"]),
            provide: spec(&["sum"]),
            effects: Box::new(|ctx, _config| {
                Box::new(once(Box::new(move || {
                    let value = {
                        let v = ctx.get::<ValKey>().expect("注入的 val 可用");
                        format!("sum({v})")
                    };
                    ctx.set::<SumKey>(value).expect("绑定 sum")
                })))
            }),
        })
    }

    /// 测试组件效应程序（ctx + config → 迭代器）。
    type Effects = Box<dyn Fn(Rc<Context>, &dyn Any) -> Box<dyn EffectIter>>;

    struct TestComponent {
        inject: KeySet,
        provide: KeySet,
        effects: Effects,
    }

    impl Component for TestComponent {
        fn inject(&self) -> KeySet {
            self.inject.clone()
        }
        fn provide(&self) -> KeySet {
            self.provide.clone()
        }
        fn apply(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
            (self.effects)(ctx, config)
        }
    }

    fn entry(id: &str, component: &str, config: &str, revision: u64, disabled: bool) -> Entry {
        Entry::new(
            id,
            component,
            Rc::new(config.to_string()),
            revision,
            disabled,
        )
    }

    fn loader() -> (Rc<Loader>, Rc<Runtime>) {
        let runtime = Rc::new(Runtime::new());
        let loader = Rc::new(Loader::new(Rc::clone(&runtime)));
        loader.register_component("provider", val_provider());
        loader.register_component("consumer", sum_consumer());
        (loader, runtime)
    }

    #[test]
    fn loads_entries_and_activates_in_dependency_order() {
        let (loader, _runtime) = loader();
        loader.apply(&[
            entry("consumer", "consumer", "ignored", 1, false),
            entry("provider", "provider", "pg", 1, false),
        ]);
        // 消费者先声明：依赖未满足保持 Inactive；provider 出现后激活。
        let provider = loader.fiber("provider").expect("provider 已加载");
        let consumer = loader.fiber("consumer").expect("consumer 已加载");
        assert!(matches!(&*provider.state(), FiberState::Active { .. }));
        assert!(
            matches!(&*consumer.state(), FiberState::Active { .. }),
            "provider 激活后 consumer 自动激活"
        );
        assert_eq!(
            provider.ctx().get::<ValKey>().unwrap().as_str(),
            "pg",
            "provider 绑定配置值"
        );
        assert_eq!(
            consumer.ctx().get::<SumKey>().unwrap().as_str(),
            "sum(pg)",
            "consumer 读取到注入值"
        );
    }

    #[test]
    fn apply_is_idempotent() {
        let (loader, _runtime) = loader();
        let desired = [
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ];
        loader.apply(&desired);
        let id_before = loader.fiber("provider").unwrap().id();
        loader.apply(&desired);
        assert_eq!(
            loader.fiber("provider").unwrap().id(),
            id_before,
            "未变化条目不重建（协调幂等）"
        );
        assert_eq!(loader.len(), 2);
    }

    #[test]
    fn disabled_toggle_unloads_and_reloads() {
        let (loader, runtime) = loader();
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        let first_id = loader.fiber("provider").unwrap().id();

        // disabled 置位：退役 + 移除，绑定恢复。
        loader.apply(&[entry("provider", "provider", "pg", 1, true)]);
        assert!(loader.fiber("provider").is_none(), "禁用条目无 fiber");
        assert!(
            runtime.fiber(first_id).is_none(),
            "旧 fiber 已从 registry 移除"
        );
        assert!(runtime.store().symbols().next().is_none(), "绑定已恢复");

        // disabled 清除：以保留的配置重新实例化。
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        let second = loader.fiber("provider").expect("重载");
        assert!(matches!(&*second.state(), FiberState::Active { .. }));
        assert_ne!(second.id(), first_id, "重载产生新 fiber");
        assert_eq!(second.ctx().get::<ValKey>().unwrap().as_str(), "pg");
    }

    #[test]
    fn config_change_rebuilds_entry() {
        let (loader, _runtime) = loader();
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        let first_id = loader.fiber("provider").unwrap().id();
        assert_eq!(
            loader
                .fiber("provider")
                .unwrap()
                .ctx()
                .get::<ValKey>()
                .unwrap()
                .as_str(),
            "pg"
        );

        // revision 递增 + 新配置 → 重建（退役旧 fiber、新配置实例化）。
        loader.apply(&[entry("provider", "provider", "mysql", 2, false)]);
        let rebuilt = loader.fiber("provider").expect("重建");
        assert_ne!(rebuilt.id(), first_id, "config 变更重建条目");
        assert_eq!(rebuilt.ctx().get::<ValKey>().unwrap().as_str(), "mysql");

        // revision 不变、同配置 → 幂等。
        let id_before = rebuilt.id();
        loader.apply(&[entry("provider", "provider", "mysql", 2, false)]);
        assert_eq!(loader.fiber("provider").unwrap().id(), id_before);
    }

    #[test]
    fn removed_entry_unloads_and_cascades() {
        let (loader, runtime) = loader();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let provider_id = loader.fiber("provider").unwrap().id();
        let consumer = loader.fiber("consumer").unwrap();

        // 从配置中移除 provider → 退役卸载 + 移除；consumer 级联停用。
        loader.apply(&[entry("consumer", "consumer", "ignored", 1, false)]);
        assert!(loader.fiber("provider").is_none());
        assert!(runtime.fiber(provider_id).is_none(), "旧 fiber 已移除");
        assert!(
            matches!(&*consumer.state(), FiberState::Inactive(_)),
            "依赖消失 → consumer 级联停用"
        );
        assert!(runtime.store().symbols().next().is_none(), "绑定全部恢复");
    }

    #[test]
    fn same_supply_replacement_in_single_apply() {
        // m1 回归：desired 用 Y 替换提供同一键的 X——两阶段协调
        // （先释放供给名再实例化），单次 apply 不得 ProvisionClash。
        let (loader, runtime) = loader();
        loader.register_component("provider2", val_provider());
        loader.apply(&[entry("old", "provider", "pg", 1, false)]);
        let old_id = loader.fiber("old").unwrap().id();
        assert_eq!(
            loader
                .fiber("old")
                .unwrap()
                .ctx()
                .get::<ValKey>()
                .unwrap()
                .as_str(),
            "pg"
        );

        // 单次 apply：移除 old（provider）并新增 new（provider2，同供给 val）。
        loader.apply(&[entry("new", "provider2", "mysql", 1, false)]);
        assert!(loader.fiber("old").is_none(), "旧条目已移除");
        assert!(
            runtime.fiber(old_id).is_none(),
            "旧 fiber 已从 registry 移除"
        );
        let new_fiber = loader.fiber("new").expect("新条目已实例化");
        assert!(matches!(&*new_fiber.state(), FiberState::Active { .. }));
        assert_eq!(
            new_fiber.ctx().get::<ValKey>().unwrap().as_str(),
            "mysql",
            "新条目使用新配置"
        );
        assert!(
            runtime.store().symbols().next().is_some(),
            "供给名由新条目持有"
        );
    }

    #[test]
    fn disabled_period_changes_take_effect_on_reenable() {
        // m3 固化：disabled 期间 component/revision 变更不更新记录，
        // enabled 后以新 entry 实例化——最终一致。
        let (loader, _runtime) = loader();
        loader.register_component("provider2", fixed_val_provider("second"));
        loader.apply(&[entry("provider", "provider", "pg", 1, false)]);
        assert!(loader.fiber("provider").is_some());

        // disabled 置位。
        loader.apply(&[entry("provider", "provider", "pg", 1, true)]);
        assert!(loader.fiber("provider").is_none(), "禁用条目无 fiber");

        // disabled 期间变更组件名 + revision（记录保持旧值，不实例化）。
        loader.apply(&[entry("provider", "provider2", "ignored", 2, true)]);
        assert!(loader.fiber("provider").is_none(), "仍 disabled：不实例化");

        // disabled 清除：以新 entry（provider2 / rev 2）实例化。
        loader.apply(&[entry("provider", "provider2", "ignored", 2, false)]);
        let fiber = loader.fiber("provider").expect("重载");
        assert!(matches!(&*fiber.state(), FiberState::Active { .. }));
        assert_eq!(
            fiber.ctx().get::<ValKey>().unwrap().as_str(),
            "second",
            "启用后使用 disabled 期间的新组件"
        );
    }

    #[test]
    #[should_panic(expected = "未注册的组件")]
    fn unknown_component_panics() {
        let (loader, _runtime) = loader();
        loader.apply(&[entry("x", "ghost", "cfg", 1, false)]);
    }
    // ── M2-PR3：intercept / isolate / group / include ─────────────────

    /// 拦截测试元数据（⊕：paths 取并、read_only 右偏）。
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct PathMeta {
        paths: BTreeSet<String>,
        read_only: bool,
    }

    impl InterceptMeta for PathMeta {
        fn merge(existing: &Self, new: &Self) -> Self {
            let mut paths = existing.paths.clone();
            paths.extend(new.paths.iter().cloned());
            PathMeta {
                paths,
                read_only: new.read_only,
            }
        }
        fn clone_box(&self) -> Box<dyn InterceptMeta> {
            Box::new(self.clone())
        }
    }

    fn path_meta(paths: &[&str], read_only: bool) -> PathMeta {
        PathMeta {
            paths: paths.iter().map(|s| s.to_string()).collect(),
            read_only,
        }
    }

    struct FsKey;
    impl Key for FsKey {
        type Value = String;
        const SYMBOL: &'static str = "fs";
    }

    /// 提供 `sum` 的提供者（与 val_provider 供给不相交，可同组共存）。
    fn sum_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["sum"]),
            effects: Box::new(|ctx, config| {
                let value = config
                    .downcast_ref::<String>()
                    .expect("sum_provider 的 config 为 String")
                    .clone();
                Box::new(once(Box::new(move || {
                    ctx.set::<SumKey>(value).expect("绑定 sum")
                })))
            }),
        })
    }

    /// fs 提供者（供注入 fs 的消费者激活）。
    fn fs_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["fs"]),
            effects: Box::new(|ctx, _config| {
                Box::new(once(Box::new(move || {
                    ctx.set::<FsKey>("fs-value".into()).expect("绑定 fs")
                })))
            }),
        })
    }

    /// 注入 fs 且声明 PathMeta 的消费者（Def 30 的 𝔇inter）。
    struct MetaConsumer;

    impl Component for MetaConsumer {
        fn inject(&self) -> KeySet {
            spec(&["fs"])
        }
        fn provide(&self) -> KeySet {
            KeySet::new()
        }
        fn apply(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
            Box::new(once(Box::new(|| Box::new(|| {}) as Disposer)))
        }
        fn declared_metadata(&self, key: Symbol) -> Option<Box<dyn InterceptMeta>> {
            if key == Symbol::intern("fs") {
                Some(Box::new(path_meta(&["/declared"], false)))
            } else {
                None
            }
        }
    }

    #[test]
    fn intercept_annotation_applied_in_place_without_rebuild() {
        let (loader, _runtime) = loader();
        loader.register_component("fs", fs_provider());
        loader.register_component("consumer", Rc::new(MetaConsumer));

        let base = Entry::new("c", "consumer", Rc::new(()), 0, false);
        loader.apply(&[base
            .clone()
            .with_intercept(Symbol::intern("fs"), path_meta(&["/anno"], true))]);
        let fiber = loader.fiber("c").expect("consumer 激活");
        let first_id = fiber.id();
        // 组件声明 ⊕ 条目注解（右偏：ι 优先）。
        assert_eq!(
            fiber.ctx().get_meta::<PathMeta>(Symbol::intern("fs")),
            Some(path_meta(&["/declared", "/anno"], true))
        );

        // 注解更新：就地应用（不重建——fiber id 不变）。
        loader.apply(&[base
            .clone()
            .with_intercept(Symbol::intern("fs"), path_meta(&["/anno2"], false))]);
        assert_eq!(
            loader.fiber("c").expect("仍在").id(),
            first_id,
            "注解更新不重建"
        );
        assert_eq!(
            loader
                .fiber("c")
                .unwrap()
                .ctx()
                .get_meta::<PathMeta>(Symbol::intern("fs")),
            Some(path_meta(&["/declared", "/anno2"], false))
        );

        // 注解移除：intercept_clear → 回退到组件声明。
        loader.apply(std::slice::from_ref(&base));
        assert_eq!(
            loader
                .fiber("c")
                .unwrap()
                .ctx()
                .get_meta::<PathMeta>(Symbol::intern("fs")),
            Some(path_meta(&["/declared"], false))
        );
    }

    // ── G3（TS-REFERENCE-GAP）：per-key isolate 粒度 ──────────────────

    /// 双键提供者（a + b；G3 混合粒度用）。
    fn dual_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["a", "b"]),
            effects: Box::new(|ctx, _config| {
                Box::new(once(Box::new(move || {
                    let _a = ctx
                        .set_dyn(Symbol::intern("a"), Box::new("va".to_string()))
                        .expect("绑定 a");
                    let _b = ctx
                        .set_dyn(Symbol::intern("b"), Box::new("vb".to_string()))
                        .expect("绑定 b");
                    Box::new(|| {}) as Disposer
                })))
            }),
        })
    }

    /// G3 混合粒度（TS `isolate: { a: true, b: 'label' }` 同型）：同一
    /// 条目对 `a` 用 Local、`b` 用 Global——per-key realm 各自独立。
    #[test]
    fn isolate_per_key_mixed_granularity() {
        let (loader, runtime) = loader();
        loader.register_component("dual", dual_provider());
        loader.apply(&[Entry::new("p", "dual", Rc::new(()), 0, false)
            .with_isolate(Symbol::intern("a"), IsolateAnnotation::Local)
            .with_isolate(Symbol::intern("b"), IsolateAnnotation::Global("g".into()))]);
        let store = runtime.store();
        assert!(
            store.contains(Symbol::intern("local:p:a")),
            "a 键 → Local realm（按条目 id）"
        );
        assert!(
            store.contains(Symbol::intern("global:g:b")),
            "b 键 → Global realm（命名共享）"
        );
        assert!(!store.contains(Symbol::intern("a")), "a 不在裸键 realm");
        drop(store);
        assert!(runtime.is_quiet(), "静止");
    }

    /// G3 + ⑪ 收口：组条目的 per-key isolate 经派生链**继承**给子条目
    ///（组 ctx 重定向被 derive 拷贝）；子条目自己的注解**覆盖**（最近
    /// 注解优先）。
    #[test]
    fn group_isolate_inherits_to_children_and_child_overrides() {
        // 继承：组 g 对 val 标 Local → 子条目提供者绑定 local:g:val。
        let (l1, rt1) = loader();
        l1.register_component("db", val_provider());
        l1.apply(&[Entry::group(
            "g",
            vec![Entry::new("p", "db", Rc::new("v".to_string()), 0, false)],
        )
        .with_isolate(Symbol::intern("val"), IsolateAnnotation::Local)]);
        assert!(
            rt1.store().contains(Symbol::intern("local:g:val")),
            "组 isolate 继承：子条目绑定落在组 local realm"
        );
        assert!(l1.fiber("p").is_some(), "子条目激活");
        drop(rt1.store());
        assert!(rt1.is_quiet(), "静止");

        // 覆盖（最近注解优先）：子条目自己的 Global 注解覆盖组的 Local。
        let (l2, rt2) = loader();
        l2.register_component("db", val_provider());
        l2.apply(&[Entry::group(
            "g",
            vec![
                Entry::new("p", "db", Rc::new("v".to_string()), 0, false)
                    .with_isolate(Symbol::intern("val"), IsolateAnnotation::Global("x".into())),
            ],
        )
        .with_isolate(Symbol::intern("val"), IsolateAnnotation::Local)]);
        assert!(
            rt2.store().contains(Symbol::intern("global:x:val")),
            "子条目注解覆盖组注解（最近优先）"
        );
        assert!(
            !rt2.store().contains(Symbol::intern("local:g:val")),
            "组的 Local 不再生效"
        );
        assert!(rt2.is_quiet(), "静止");
    }

    /// G3 组 isolate 变更：reconcile 组分支仍走整棵重建（保守路径）——
    /// 子条目重建、绑定迁到新 realm。
    #[test]
    fn group_isolate_change_rebuilds_subtree() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.apply(&[Entry::group(
            "g",
            vec![Entry::new("p", "db", Rc::new("v".to_string()), 0, false)],
        )
        .with_isolate(Symbol::intern("val"), IsolateAnnotation::Local)]);
        assert!(
            runtime.store().contains(Symbol::intern("local:g:val")),
            "初始：组 Local realm"
        );
        let child_first = loader.fiber("p").unwrap().id();

        loader.apply(&[Entry::group(
            "g",
            vec![Entry::new("p", "db", Rc::new("v".to_string()), 0, false)],
        )
        .with_isolate(Symbol::intern("val"), IsolateAnnotation::Global("y".into()))]);
        assert!(
            runtime.store().contains(Symbol::intern("global:y:val")),
            "组 isolate 变更 → 整棵重建、绑定迁 realm"
        );
        assert!(
            !runtime.store().contains(Symbol::intern("local:g:val")),
            "旧 realm 无绑定"
        );
        assert_ne!(
            loader.fiber("p").unwrap().id(),
            child_first,
            "整棵重建 = 子条目新 fiber"
        );
        assert!(runtime.is_quiet(), "静止");
    }

    // ── G7（TS-REFERENCE-GAP）：config 校验 + 值级 diff ──────────────

    /// G7 测试配置：validate（空串 = Err）+ same（值级比较）。
    #[derive(Clone, Debug)]
    struct ValConfig(String);

    impl Config for ValConfig {
        fn validate(&self) -> Result<(), String> {
            if self.0.is_empty() {
                Err("empty config".into())
            } else {
                Ok(())
            }
        }
        fn same(&self, other: &dyn Any) -> bool {
            other
                .downcast_ref::<ValConfig>()
                .is_some_and(|o| o.0 == self.0)
        }
    }

    /// 读 [`ValConfig`] 的 provider（覆盖 `loader()` 的 String 版）。
    fn val_config_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["val"]),
            effects: Box::new(|ctx, config| {
                let value = config
                    .downcast_ref::<ValConfig>()
                    .expect("ValConfig")
                    .0
                    .clone();
                Box::new(once(Box::new(move || {
                    ctx.set::<ValKey>(value).expect("绑定 val")
                })))
            }),
        })
    }

    fn val_config(v: &str, revision: u64) -> Entry {
        Entry::new(
            "p",
            "provider",
            Rc::new(ValConfig(v.to_string())),
            revision,
            false,
        )
    }

    /// G7 值级 diff：注册 Config 后，revision 递增但同值 → 免重建
    ///（TS deepEqual 同型）；值变 → 重建。
    #[test]
    fn config_same_skips_rebuild_on_identical_value() {
        let (loader, runtime) = loader();
        loader.register_component("provider", val_config_provider());
        loader.register_config::<ValConfig>();
        loader.apply(&[
            val_config("pg", 1),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let first = loader.fiber("p").unwrap().id();

        loader.apply(&[
            val_config("pg", 2),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert_eq!(
            loader.fiber("p").unwrap().id(),
            first,
            "revision 递增但值级相同 → 免重建"
        );
        assert!(runtime.is_quiet(), "静止");

        loader.apply(&[
            val_config("pg2", 3),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert_ne!(loader.fiber("p").unwrap().id(), first, "值变化 → 重建");
        assert!(runtime.is_quiet(), "静止");
    }

    /// G7 未注册类型保守走 revision：String config revision 递增同值仍
    /// 重建（HMR 兼容纪律——cordis-hmr 依赖 revision 递增触发重载）。
    #[test]
    fn unregistered_config_keeps_revision_semantics() {
        let (loader, _runtime) = loader();
        loader.apply(&[
            entry("p", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let first = loader.fiber("p").unwrap().id();
        loader.apply(&[
            entry("p", "provider", "pg", 2, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert_ne!(
            loader.fiber("p").unwrap().id(),
            first,
            "未注册类型不参与值级 diff（String revision 语义保持）"
        );
    }

    /// G7 校验失败 = 配置错误（panic；与 ProvisionClash 同型）。
    #[test]
    #[should_panic(expected = "配置校验失败")]
    fn config_validate_failure_panics() {
        let (loader, _runtime) = loader();
        loader.register_config::<ValConfig>();
        loader.apply(&[
            val_config("", 1),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
    }

    /// G7 未注册类型无校验（opt-in）。
    #[test]
    fn unregistered_config_not_validated() {
        let (loader, runtime) = loader();
        loader.apply(&[entry("p", "provider", "", 1, false)]);
        assert!(loader.fiber("p").is_some(), "未注册类型不校验（激活）");
        assert!(runtime.is_quiet(), "静止");
    }

    #[test]
    fn isolate_local_creates_private_realms() {
        // Local：提供者与消费者各自私有 realm——消费者看不到绑定。
        {
            let (loader, runtime) = loader();
            loader.register_component("db", val_provider());
            loader.register_component("cons", sum_consumer());
            loader.apply(&[
                Entry::new("p", "db", Rc::new("v".to_string()), 0, false)
                    .with_isolate(Symbol::intern("val"), IsolateAnnotation::Local),
                Entry::new("c", "cons", Rc::new(()), 0, false)
                    .with_isolate(Symbol::intern("val"), IsolateAnnotation::Local),
            ]);
            assert!(loader.fiber("p").is_some(), "提供者激活");
            let c = loader
                .fiber("c")
                .expect("消费者 fiber 存在（未满足依赖 → Inactive）");
            assert!(
                matches!(&*c.state(), FiberState::Inactive(_)),
                "跨条目 Local realm：消费者看不到提供者的绑定（Inactive）"
            );
            assert!(runtime.is_quiet(), "静止（消费者 Inactive）");
        }
        // Global：命名共享——提供者与消费者在同一 realm，消费者激活。
        {
            let (loader, runtime) = loader();
            loader.register_component("db", val_provider());
            loader.register_component("cons", sum_consumer());
            loader.apply(&[
                Entry::new("p", "db", Rc::new("v".to_string()), 0, false).with_isolate(
                    Symbol::intern("val"),
                    IsolateAnnotation::Global("db".into()),
                ),
                Entry::new("c", "cons", Rc::new(()), 0, false).with_isolate(
                    Symbol::intern("val"),
                    IsolateAnnotation::Global("db".into()),
                ),
            ]);
            assert!(loader.fiber("p").is_some(), "提供者激活");
            assert!(loader.fiber("c").is_some(), "Global realm 共享：消费者激活");
            assert!(runtime.is_quiet(), "静止");
        }
    }

    #[test]
    fn group_keyed_diff_preserves_surviving_children() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("sum", sum_provider());
        let child = |id: &str, component: &str, value: &str| {
            Entry::new(id, component, Rc::new(value.to_string()), 0, false)
        };

        let g = Entry::group("g", vec![child("a", "db", "1"), child("b", "sum", "2")]);
        loader.apply(&[g]);
        let _a = loader.fiber("a").expect("a 激活");
        let b = loader.fiber("b").expect("b 激活");
        let b_id = b.id();

        // 子列表 keyed diff：a 移除、c 新增、b 幸存（fiber 不变）。
        let g2 = Entry::group("g", vec![child("b", "sum", "2"), child("c", "db", "3")]);
        loader.apply(&[g2]);
        assert!(loader.fiber("a").is_none(), "a 已移除");
        assert!(loader.fiber("c").is_some(), "c 已新增");
        assert_eq!(
            loader.fiber("b").expect("b 幸存").id(),
            b_id,
            "幸存子条目不重建"
        );
        assert!(runtime.is_quiet(), "静止");
    }

    #[test]
    fn group_teardown_removes_subtree() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("sum", sum_provider());
        let child = |id: &str, component: &str, value: &str| {
            Entry::new(id, component, Rc::new(value.to_string()), 0, false)
        };
        loader.apply(&[Entry::group(
            "g",
            vec![child("a", "db", "1"), child("b", "sum", "2")],
        )]);
        assert!(loader.fiber("a").is_some() && loader.fiber("b").is_some());

        // 移除组 → 整棵子树拆除（子代 fiber 也移出 registry）。
        loader.apply(&[]);
        assert!(loader.fiber("g").is_none());
        assert!(loader.fiber("a").is_none() && loader.fiber("b").is_none());
        assert_eq!(runtime.len(), 0, "registry 无残留 fiber");
        assert!(runtime.is_quiet(), "静止");
        assert!(runtime.store().symbols().next().is_none(), "绑定全清");
    }

    #[test]
    fn disabled_group_teardown_and_reenable() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        let child =
            |id: &str, value: &str| Entry::new(id, "db", Rc::new(value.to_string()), 0, false);
        loader.apply(&[Entry::group("g", vec![child("a", "1")])]);
        assert!(loader.fiber("a").is_some());

        let mut g = Entry::group("g", vec![child("a", "1")]);
        g.disabled = true;
        loader.apply(&[g]);
        assert!(loader.fiber("g").is_none());
        assert!(loader.fiber("a").is_none(), "组禁用 → 子代拆除");
        assert_eq!(runtime.len(), 0, "registry 无残留");

        loader.apply(&[Entry::group("g", vec![child("a", "1")])]);
        assert!(loader.fiber("a").is_some(), "重新启用 → 子代恢复");
        assert!(runtime.is_quiet(), "静止");
    }

    #[test]
    fn include_grafts_children() {
        let (loader, _runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("sum", sum_provider());
        // include = 外部配置嫁接（结构同 group）。
        let external = vec![
            Entry::new("x", "db", Rc::new("1".to_string()), 0, false),
            Entry::new("y", "sum", Rc::new("2".to_string()), 0, false),
        ];
        loader.apply(&[Entry::include("ext", external)]);
        assert!(loader.fiber("x").is_some());
        assert!(loader.fiber("y").is_some());
    }

    #[test]
    fn isolate_change_reassigns_realms_without_rebuild() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("cons", sum_consumer());
        let base_p = Entry::new("p", "db", Rc::new("v".to_string()), 0, false).with_isolate(
            Symbol::intern("val"),
            IsolateAnnotation::Global("db".into()),
        );
        let base_c = Entry::new("c", "cons", Rc::new(()), 0, false).with_isolate(
            Symbol::intern("val"),
            IsolateAnnotation::Global("db".into()),
        );
        loader.apply(&[base_p.clone(), base_c.clone()]);
        let p_first = loader.fiber("p").expect("p 激活").id();
        let c_first = loader.fiber("c").expect("c 激活（共享 realm）").id();
        assert!(
            runtime.store().contains(Symbol::intern("global:db:val")),
            "绑定在旧 realm"
        );

        // p 的 isolate 变更（Global db → db2）：**Algorithm 7 重指派**——
        // 不重建（fiber id 不变），绑定迁移到新 realm。
        loader.apply(&[
            base_p.clone().with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db2".into()),
            ),
            base_c.clone(),
        ]);
        assert_eq!(
            loader.fiber("p").expect("p 仍在").id(),
            p_first,
            "Algorithm 7 不重建（fiber 不变）"
        );
        let store = runtime.store();
        assert!(
            store.contains(Symbol::intern("global:db2:val")),
            "绑定迁移到新 realm"
        );
        assert!(
            !store.contains(Symbol::intern("global:db:val")),
            "旧 realm 无绑定"
        );
        drop(store);
        // c 仍解析旧 realm → 依赖丢失 → 停用（affected 通知）。
        assert!(
            matches!(
                *loader.fiber("c").expect("c 仍在").state(),
                FiberState::Inactive(_)
            ),
            "c 停用（提供者 realm 迁移，依赖不可见）"
        );
        assert!(runtime.is_quiet(), "静止");

        // c 也迁到 db2 → 重新激活（c 的 fiber 亦不变）。
        loader.apply(&[
            base_p.clone().with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db2".into()),
            ),
            base_c.clone().with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db2".into()),
            ),
        ]);
        assert_eq!(
            loader.fiber("c").expect("c 仍在").id(),
            c_first,
            "c 亦不重建"
        );
        assert!(
            matches!(
                *loader.fiber("c").unwrap().state(),
                FiberState::Active { .. }
            ),
            "c 在新 realm 重新激活"
        );
        assert!(runtime.is_quiet(), "静止");
    }

    /// Algorithm 7 子树场景：组内子条目的绑定随条目 realm 迁移（own 判定
    /// 覆盖子树成员）。
    #[test]
    fn isolate_change_moves_group_child_binding() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("cons", sum_consumer());
        let child = Entry::new("a", "db", Rc::new("1".to_string()), 0, false).with_isolate(
            Symbol::intern("val"),
            IsolateAnnotation::Global("db".into()),
        );
        loader.apply(&[
            Entry::group("g", vec![child]),
            Entry::new("c", "cons", Rc::new(()), 0, false).with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db".into()),
            ),
        ]);
        assert!(loader.fiber("a").is_some(), "组内子条目激活");
        assert!(
            matches!(
                *loader.fiber("c").unwrap().state(),
                FiberState::Active { .. }
            ),
            "c 依赖组内提供者激活"
        );

        // 组内子条目 a 的 isolate 变更：绑定迁移 + c 停用；a 不重建。
        loader.apply(&[
            Entry::group(
                "g",
                vec![
                    Entry::new("a", "db", Rc::new("1".to_string()), 0, false).with_isolate(
                        Symbol::intern("val"),
                        IsolateAnnotation::Global("db2".into()),
                    ),
                ],
            ),
            Entry::new("c", "cons", Rc::new(()), 0, false).with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db".into()),
            ),
        ]);
        let store = runtime.store();
        assert!(
            store.contains(Symbol::intern("global:db2:val")),
            "子条目绑定已迁移"
        );
        assert!(
            !store.contains(Symbol::intern("global:db:val")),
            "旧 realm 无绑定"
        );
        drop(store);
        assert!(
            matches!(*loader.fiber("c").unwrap().state(), FiberState::Inactive(_)),
            "c 停用（依赖随 realm 迁移）"
        );
        assert!(runtime.is_quiet(), "静止");
    }

    /// M3-PR3 处置⑩ 语义钉死（**组件→条目写回缺席**）：`fiber.retire()`
    /// 是**运行时杆杠**（退役标记 + 卸载级联），**不改写条目**——desired
    /// 树保持权威：条目未变则 apply 是零操作（退役粘滞），编排方须改动
    /// 条目（移除 / revision 递增）才恢复或重建。这是"双向写回未实现
    ///（条目权威）"（M2-PR3 已知边界①、THEORY-MAP 处置⑩）的可观察语义。
    #[test]
    fn retired_component_persists_across_unchanged_apply() {
        let (loader, runtime) = loader();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let provider = loader.fiber("provider").unwrap().clone();
        provider.retire();
        assert!(provider.retired(), "退役标记（组件侧杆杠）");
        assert!(
            matches!(
                *loader.fiber("consumer").unwrap().state(),
                FiberState::Inactive(_)
            ),
            "退役级联：consumer 停用"
        );
        // 条目未变 → apply 零操作：退役粘滞（无写回——若存在组件→条目
        // 写回方向，此处条目状态应被组件侧改动改写；现实是条目权威、
        // 退役仅为运行时态）。
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert!(
            provider.retired(),
            "退役跨越未变 apply 粘滞（组件→条目写回缺席：条目为权威）"
        );
        assert!(
            matches!(
                *loader.fiber("consumer").unwrap().state(),
                FiberState::Inactive(_)
            ),
            "consumer 仍停用（未变 apply 不恢复）"
        );
        // 编排方改条目（revision 递增）→ 重建恢复。
        loader.apply(&[
            entry("provider", "provider", "pg", 2, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert!(
            matches!(
                *loader.fiber("provider").unwrap().state(),
                FiberState::Active { .. }
            ),
            "条目变更（revision）→ 退役 fiber 被重建"
        );
        assert!(
            matches!(
                *loader.fiber("consumer").unwrap().state(),
                FiberState::Active { .. }
            ),
            "consumer 随 provider 重建恢复"
        );
        assert!(runtime.is_quiet(), "静止");
    }

    /// G1（TS-REFERENCE-GAP）双向绑定条目侧（§5.2.1 "the binding runs in
    /// both directions"；TS loader `internal/update` 参照）：`update_entry`
    /// 就地更新 config + 条目 fiber **就地重跑**（身份保留、依赖者级联），
    /// 不递增 revision——同 revision 的后续 apply 不重建（写回不被清除）。
    #[test]
    fn update_entry_replaces_config_in_place() {
        let (loader, runtime) = loader();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let provider_id = loader.fiber("provider").unwrap().id();
        let base = {
            let store = runtime.store();
            store
                .get_value(Symbol::intern("val"))
                .expect("val 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(base, "pg", "初始绑定 = 条目 config");

        loader.update_entry("provider", Rc::new("pg2".to_string()));
        assert_eq!(
            loader.fiber("provider").unwrap().id(),
            provider_id,
            "update_entry = 就地重跑（fiber 身份保留，非重建）"
        );
        assert!(matches!(
            &*loader.fiber("provider").unwrap().state(),
            FiberState::Active { .. }
        ));
        assert!(matches!(
            &*loader.fiber("consumer").unwrap().state(),
            FiberState::Active { .. }
        ));
        let value = {
            let store = runtime.store();
            store
                .get_value(Symbol::intern("val"))
                .expect("val 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "pg2", "绑定反映新 config");
        assert_eq!(
            loader
                .entry_config("provider")
                .unwrap()
                .downcast_ref::<String>()
                .unwrap(),
            "pg2",
            "条目书签已写回"
        );
        assert!(runtime.is_quiet(), "update_entry 后静止");

        // 同 revision 的 apply 零操作：**fiber 状态保留**（协调键未变、
        // 不重建——就地重跑后的新配置不被清除）；书签则回映 desired
        //（reconcile 的 no-op 分支把 desired config 拷回记录——书签是
        // 协调记录而非权威源，调用方作为树所有者决定持久化）。
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert_eq!(
            loader.fiber("provider").unwrap().id(),
            provider_id,
            "同 revision apply 零操作（fiber 身份保留）"
        );
        let value = {
            let store = runtime.store();
            store
                .get_value(Symbol::intern("val"))
                .expect("val 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "pg2", "fiber 层写回保留（不重建）");
        assert_eq!(
            loader
                .entry_config("provider")
                .unwrap()
                .downcast_ref::<String>()
                .unwrap(),
            "pg",
            "书签回映 desired（协调记录非权威源）"
        );
    }

    /// G1 组件侧自更新 → loader 观察者写回：`register_update_hook` 把
    /// runtime 钩子接到 loader——`Fiber::update` 触发时新 config 自动写入
    /// 所属条目书签（TS `internal/update` 的 loader 半段）。
    #[test]
    fn fiber_self_update_writes_back_through_loader_hook() {
        let (loader, runtime) = loader();
        loader.register_update_hook();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let provider_id = loader.fiber("provider").unwrap().id();

        loader
            .fiber("provider")
            .unwrap()
            .update(Rc::new("pg3".to_string()));
        assert_eq!(
            loader.fiber("provider").unwrap().id(),
            provider_id,
            "自更新 = 就地重跑"
        );
        assert_eq!(
            loader
                .entry_config("provider")
                .unwrap()
                .downcast_ref::<String>()
                .unwrap(),
            "pg3",
            "观察者已把新 config 写回条目书签"
        );
        let value = {
            let store = runtime.store();
            store
                .get_value(Symbol::intern("val"))
                .expect("val 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "pg3", "绑定反映自更新");
        assert!(runtime.is_quiet(), "自更新后静止");
    }

    /// G1 组内子条目自更新：写回映射（`entry_of`）命中嵌套条目。
    #[test]
    fn group_child_self_update_maps_to_nested_entry() {
        let (loader, _runtime) = loader();
        loader.register_update_hook();
        loader.apply(&[Entry::group(
            "g",
            vec![
                entry("child", "provider", "pg", 1, false),
                entry("consumer", "consumer", "ignored", 1, false),
            ],
        )]);
        loader
            .fiber("child")
            .unwrap()
            .update(Rc::new("pg4".to_string()));
        assert_eq!(
            loader
                .entry_config("child")
                .unwrap()
                .downcast_ref::<String>()
                .unwrap(),
            "pg4",
            "组内子条目的自更新写回其自身书签"
        );
    }

    // ── G2（TS-REFERENCE-GAP）：注入携带配置（Entry.inject）──────────

    /// 读取 `fs` 键拦截元数据的提供者（G2 消费端：`get_meta` 右偏合并）。
    fn meta_provider() -> Rc<TestComponent> {
        Rc::new(TestComponent {
            inject: spec(&[]),
            provide: spec(&["fs"]),
            effects: Box::new(|ctx, _config| {
                Box::new(once(Box::new(move || {
                    let meta = ctx.get_meta::<PathMeta>(Symbol::intern("fs"));
                    let value = match &meta {
                        Some(m) if m.read_only => "ro".to_string(),
                        Some(m) => format!(
                            "rw:{}",
                            m.paths.iter().cloned().collect::<Vec<_>>().join(",")
                        ),
                        None => "none".to_string(),
                    };
                    ctx.set::<FsKey>(value).expect("绑定 fs")
                })))
            }),
        })
    }

    /// G2 注入携带配置（TS `EntryOptions.inject` 参照）：条目注入的
    /// 每键配置经 `get_meta` 右偏合并被提供者消费（Def 30/31 的 `ι(k)`
    /// 实用化）；无注入时读不到元数据。
    ///
    /// 注：inject 变更与 config 同纪律——值不可比较，变更须随 revision
    /// 递增触发重建（reconcile 不感知 inject 字段）。
    #[test]
    fn entry_inject_config_consumed_via_get_meta() {
        // 有注入：提供者读到 `ro`。
        let (l1, rt1) = loader();
        l1.register_component("m", meta_provider());
        l1.apply(&[Entry::new("p", "m", Rc::new(()), 0, false)
            .with_inject(Symbol::intern("fs"), path_meta(&["/x"], true))]);
        let value = {
            let store = rt1.store();
            store
                .get_value(Symbol::intern("fs"))
                .expect("fs 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "ro", "注入携带配置经 get_meta 合并消费");
        assert!(rt1.is_quiet(), "静止");

        // 无注入：独立系统读不到元数据。
        let (l2, rt2) = loader();
        l2.register_component("m", meta_provider());
        l2.apply(&[Entry::new("p", "m", Rc::new(()), 0, false)]);
        let value = {
            let store = rt2.store();
            store
                .get_value(Symbol::intern("fs"))
                .expect("fs 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "none", "无注入携带配置时读不到元数据");
    }

    /// G2 遮蔽序：同键 inject 后应用，遮蔽 entry 层 intercept
    ///（TS fiber 层 inject 遮蔽 entry 层 intercept 的对应）。
    #[test]
    fn inject_shadows_intercept_for_same_key() {
        let (loader, runtime) = loader();
        loader.register_component("m", meta_provider());
        loader.apply(&[Entry::new("p", "m", Rc::new(()), 0, false)
            .with_intercept(Symbol::intern("fs"), path_meta(&["/i"], false))
            .with_inject(Symbol::intern("fs"), path_meta(&["/j"], true))]);
        let value = {
            let store = runtime.store();
            store
                .get_value(Symbol::intern("fs"))
                .expect("fs 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "ro", "inject（后应用）遮蔽 intercept");
        assert!(runtime.is_quiet(), "静止");
    }

    /// G2 组条目注入携带配置：经派生链拷贝继承给子条目（提供者读取）。
    #[test]
    fn group_inject_inherits_to_children() {
        let (loader, runtime) = loader();
        loader.register_component("m", meta_provider());
        loader.apply(&[
            Entry::group("g", vec![Entry::new("p", "m", Rc::new(()), 0, false)])
                .with_inject(Symbol::intern("fs"), path_meta(&["/g"], false)),
        ]);
        let value = {
            let store = runtime.store();
            store
                .get_value(Symbol::intern("fs"))
                .expect("fs 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(value, "rw:/g", "组注入配置经派生继承给子条目");
        assert!(runtime.is_quiet(), "静止");
    }

    /// G2 变更纪律负例（REVIEW-97bb598 nit-3）：`LoadedEntry` 不存
    /// `inject`，同 revision 的 inject 变更被 reconcile 忽略（须随
    /// revision 递增才重建）——钉死该纪律为可观察契约。
    #[test]
    fn inject_change_without_revision_is_ignored() {
        let (l1, rt1) = loader();
        l1.register_component("m", meta_provider());
        l1.apply(&[Entry::new("p", "m", Rc::new(()), 0, false)
            .with_inject(Symbol::intern("fs"), path_meta(&["/a"], false))]);
        let first = {
            let store = rt1.store();
            store
                .get_value(Symbol::intern("fs"))
                .expect("fs 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(first, "rw:/a", "初始注入生效");

        // 同 revision 换 inject（值不同）：reconcile 零操作，旧注入保持。
        l1.apply(&[Entry::new("p", "m", Rc::new(()), 0, false)
            .with_inject(Symbol::intern("fs"), path_meta(&["/b"], true))]);
        let value = {
            let store = rt1.store();
            store
                .get_value(Symbol::intern("fs"))
                .expect("fs 绑定")
                .downcast_ref::<String>()
                .unwrap()
                .clone()
        };
        assert_eq!(
            value, "rw:/a",
            "同 revision inject 变更被忽略（纪律：随 revision 递增）"
        );
        assert!(rt1.is_quiet(), "静止");
    }

    // ── G4/G1 剩余（TS-REFERENCE-GAP）：退役写回（self-dispose → disabled）──

    /// 组件自退役 → loader 观察者写回条目书签 `disabled = true`；随后
    /// desired 显式 `disabled=false` 的 apply **重新启用**（disabled 是
    /// 协调字段——与 update 写回不同：config 非协调字段、同 revision
    /// apply 不清除）。
    #[test]
    fn self_retire_writes_back_disabled_to_entry() {
        let (loader, runtime) = loader();
        loader.register_retire_hook();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        let first = loader.fiber("provider").unwrap().id();

        // 组件自退役（TS ctx.fiber.dispose() 同型）。
        loader.fiber("provider").unwrap().retire();
        assert!(
            matches!(
                &*loader.fiber("consumer").unwrap().state(),
                FiberState::Inactive(_)
            ),
            "退役级联：consumer 停用"
        );
        assert_eq!(
            loader.entry_disabled("provider"),
            Some(true),
            "观察者已把自退役写回条目书签 disabled=true"
        );
        assert!(runtime.is_quiet(), "自退役后静止");

        // desired 显式 disabled=false → 协调字段拉回：重新启用。
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert!(
            matches!(
                &*loader.fiber("provider").unwrap().state(),
                FiberState::Active { .. }
            ),
            "desired disabled=false → 重新启用"
        );
        assert!(
            loader.fiber("provider").unwrap().id() != first,
            "重新启用 = 新 fiber（退役已卸载旧 fiber）"
        );
        assert_eq!(
            loader.entry_disabled("provider"),
            Some(false),
            "书签随协调回映 desired"
        );
        assert!(runtime.is_quiet(), "重新启用后静止");

        // 自退役 + desired disabled=true → 保持禁用（fiber 不存在）。
        loader.fiber("provider").unwrap().retire();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, true),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert!(
            loader.fiber("provider").is_none(),
            "desired disabled=true → 条目无 fiber"
        );
        assert!(runtime.is_quiet(), "禁用后静止");
    }

    /// loader 驱动的操作不写回 disabled（过滤语义）：revision 重建 /
    /// disabled 切换 / 条目移除时条目已被移除或已置位，观察者忽略。
    #[test]
    fn loader_driven_operations_do_not_write_back_disabled() {
        let (loader, runtime) = loader();
        loader.register_retire_hook();
        loader.apply(&[
            entry("provider", "provider", "pg", 1, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);

        // revision 重建（teardown 内 retire）→ 书签 disabled 不被写回。
        loader.apply(&[
            entry("provider", "provider", "pg", 2, false),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert_eq!(
            loader.entry_disabled("provider"),
            Some(false),
            "重建不写回 disabled（条目已被移除 → 观察者忽略）"
        );
        assert!(
            matches!(
                &*loader.fiber("provider").unwrap().state(),
                FiberState::Active { .. }
            ),
            "重建后 Active"
        );

        // disabled 切换（卸载路径）→ 书签为 desired 的 true（非写回产物）。
        loader.apply(&[
            entry("provider", "provider", "pg", 2, true),
            entry("consumer", "consumer", "ignored", 1, false),
        ]);
        assert_eq!(loader.entry_disabled("provider"), Some(true));
        assert!(loader.fiber("provider").is_none());

        // 条目移除 → 无书签。
        loader.apply(&[entry("consumer", "consumer", "ignored", 1, false)]);
        assert_eq!(loader.entry_disabled("provider"), None, "条目已移除");
        assert!(runtime.is_quiet(), "静止");
    }

    /// 组内子条目自退役 → 写回映射（entry_of）命中嵌套条目书签。
    #[test]
    fn group_child_self_retire_maps_to_nested_entry() {
        let (loader, _runtime) = loader();
        loader.register_retire_hook();
        loader.apply(&[Entry::group(
            "g",
            vec![
                entry("child", "provider", "pg", 1, false),
                entry("consumer", "consumer", "ignored", 1, false),
            ],
        )]);
        loader.fiber("child").unwrap().retire();
        assert_eq!(
            loader.entry_disabled("child"),
            Some(true),
            "组内子条目自退役写回其自身书签"
        );
    }

    /// Algorithm 7 边界（审查 major1，REVIEW-ef57804）：Local → Global 与
    /// None（裸键 realm）→ Some 的迁移。
    #[test]
    fn isolate_change_boundaries_local_to_global() {
        // Local → Global：绑定从 local:<id>:val 迁到 global:db:val。
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        let local = Entry::new("p", "db", Rc::new("v".to_string()), 0, false)
            .with_isolate(Symbol::intern("val"), IsolateAnnotation::Local);
        loader.apply(&[local]);
        assert!(
            runtime.store().contains(Symbol::intern("local:p:val")),
            "绑定在 local realm"
        );

        loader.apply(&[
            Entry::new("p", "db", Rc::new("v".to_string()), 0, false).with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db".into()),
            ),
        ]);
        let store = runtime.store();
        assert!(
            store.contains(Symbol::intern("global:db:val")),
            "绑定迁到 global realm"
        );
        assert!(
            !store.contains(Symbol::intern("local:p:val")),
            "local realm 清空"
        );
        drop(store);
        assert!(runtime.is_quiet(), "静止");
    }

    /// Algorithm 7 边界：None（裸键 realm）→ Some（Global）。
    #[test]
    fn isolate_change_boundaries_none_to_global() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.apply(&[Entry::new("p", "db", Rc::new("v".to_string()), 0, false)]);
        assert!(
            runtime.store().contains(Symbol::intern("val")),
            "裸键 realm 绑定"
        );

        loader.apply(&[
            Entry::new("p", "db", Rc::new("v".to_string()), 0, false).with_isolate(
                Symbol::intern("val"),
                IsolateAnnotation::Global("db".into()),
            ),
        ]);
        let store = runtime.store();
        assert!(
            store.contains(Symbol::intern("global:db:val")),
            "绑定迁到 global realm"
        );
        assert!(!store.contains(Symbol::intern("val")), "裸键 realm 清空");
        drop(store);
        assert!(runtime.is_quiet(), "静止");
    }

    /// 组内同供给替换（REVIEW-24bfab5 nit6）：两阶段协调递归到组内——
    /// 组内子条目用不同组件提供同一键的替换可在单次 apply 完成（不
    /// ProvisionClash）。
    #[test]
    fn group_internal_same_supply_replacement() {
        let (loader, runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("db2", val_provider());
        let child = |id: &str, component: &str, value: &str| {
            Entry::new(id, component, Rc::new(value.to_string()), 0, false)
        };
        loader.apply(&[Entry::group("g", vec![child("x", "db", "1")])]);
        let x = loader.fiber("x").expect("x 激活").id();

        // 组内把提供 val 的 x 换成 db2 组件（同供给键，单次 apply）。
        loader.apply(&[Entry::group("g", vec![child("x", "db2", "1")])]);
        let x2 = loader.fiber("x").expect("x 重建后激活").id();
        assert_ne!(x, x2, "同供给替换 → 重建（新 fiber）");
        assert!(runtime.is_quiet(), "静止");
        assert!(
            runtime.store().contains(Symbol::intern("val")),
            "替换后绑定仍在（同键）"
        );
    }

    /// major1 负向直证（REVIEW-24bfab5）：组子条目覆写组拦截注解后再移除
    /// 自身注解 → 该键回到无元数据状态（不回退组继承值——扁平拷贝语义，
    /// 已知边界⑤）。
    #[test]
    fn intercept_clear_does_not_restore_group_inherited_value() {
        let (loader, _runtime) = loader();
        loader.register_component("db", val_provider());
        loader.register_component("cons", sum_consumer());
        // 组 g 带 fs 拦截注解；子条目 a 覆写同一键。
        let group_meta = path_meta(&["/group"], false);
        let mut g = Entry::group(
            "g",
            vec![
                Entry::new("a", "db", Rc::new("1".to_string()), 0, false)
                    .with_intercept(Symbol::intern("fs"), path_meta(&["/child"], true)),
            ],
        );
        g = g.with_intercept(Symbol::intern("fs"), group_meta.clone());
        loader.apply(&[g]);
        let a = loader.fiber("a").expect("a 激活");
        // 子条目覆写组值（replace 语义：子注解权威）。
        assert_eq!(
            a.ctx().get_meta::<PathMeta>(Symbol::intern("fs")),
            Some(path_meta(&["/child"], true)),
            "子覆写组拦截"
        );

        // 移除子条目自身注解 → 该键无元数据（不回退 /group）。
        let g2 = Entry::group(
            "g",
            vec![Entry::new("a", "db", Rc::new("1".to_string()), 0, false)],
        )
        .with_intercept(Symbol::intern("fs"), group_meta.clone());
        loader.apply(&[g2]);
        let a2 = loader.fiber("a").expect("a 仍在");
        assert_eq!(
            a2.ctx().get_meta::<PathMeta>(Symbol::intern("fs")),
            None,
            "移除注解 → 无元数据（不回退组继承值，扁平拷贝语义）"
        );
    }
}
