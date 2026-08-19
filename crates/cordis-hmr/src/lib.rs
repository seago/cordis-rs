//! Cordis 热模块替换引擎（论文 §5.2.2，M2-PR5）。
//!
//! HMR 在**模块级**应用可逆效应模式：源文件变化时，系统原位替换受影响
//! 的模块而不重启进程。因为 fiber 已经界定了组件全部效应/共效应，组件
//! 模块可用 fiber 操作替换：退役旧 fiber 恢复其全部安装，新 fiber 从
//! 重载的模块重新安装（"HMR needs no developer-annotated acceptance
//!  boundaries"）。
//!
//! 三阶段（Algorithm 8/9/10 直译）：
//!
//! 1. **模块分类**（[`classify`]）：以 stashed（内容变化的文件 URL 集）与
//!    externals（不可热替换、触发整机重启的模块集）为种子，在导入依赖图
//!    上做不动点——模块一旦有导入被接受则接受；全部导入被拒绝则拒绝；
//!    未决（含导入环）默认拒绝。
//! 2. **过期条目检测**（[`detect`]）：沿每个条目的依赖树（[`get_dependencies`]，
//!    以 declined 为边界）找与 accepted 相交的条目（过期）。
//! 3. **事务性重载**（[`Hmr::reload`]）：备份 accepted 模块的缓存 → 逐个
//!    过期条目退役旧 fiber、以重载的模块实例化新 fiber → 任一失败则恢复
//!    缓存、用备份重建全部过期条目（**永不进入半重载状态**）。
//!
//! ## 模块图（[`ModuleGraph`]）
//!
//! `get_imports(url)` 的提供者：M2 提供数据驱动实现（[`HashMapGraph`]）与
//! wasm 叶子图（[`WasmLeafGraph`]——M1 的 wasm 插件只导入宿主 context
//! 接口，不导入其它插件，故为叶子）；native 的 cargo metadata 依赖图与
//! wasm 的 wit import 图解析为生产化适配器（M2 边界记录，THEORY-MAP
//! PR #21 行）。
//!
//! **⑫ 评估结案（M3-PR3）**：生产化适配器 = 解析 cargo metadata（JSON）
//! 或 Cargo.toml（TOML）→ 模块图。算法 crate 无 TOML/JSON 解析器依赖
//!（本 crate 仅 `anyhow` 错误处理；`serde` 为 wasmtime 传递依赖不可
//! 用），手写解析器对真实清单脆弱且高风险；`HashMapGraph` 已把算法
//!（`classify`/`detect`/`reload`）证明为数据驱动——适配器只是数据来源
//! 替换，不触碰算法。结论：适配器随 typed world / 编排工具层（届时允许
//! serde_json/toml 依赖）落地的构建工具 crate，本里程碑记录为公开差异关闭。
//!
//! ## 事务语义与失败模型（M2-PR1 协同）
//!
//! 重载失败（组件加载/实例化失败，fiber 进入 `Inactive(Some(ζ))`）由
//! 事务层检测（fiber 失败态）→ 回滚（恢复旧组件 + 重新 apply）。

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use cordis_core::FiberState;
use cordis_loader::{Entry, EntryOutcome, Loader};

/// 模块导入依赖图（`get_imports(url)`，§5.2.2）。
pub trait ModuleGraph {
    /// `url` 直接导入的模块 URL 列表。
    fn get_imports(&self, url: &str) -> Vec<String>;
}

/// 数据驱动实现：显式映射（编排方/解析器提供；cargo metadata / wit
/// import 解析为生产化适配器）。
pub struct HashMapGraph(pub HashMap<String, Vec<String>>);

impl ModuleGraph for HashMapGraph {
    fn get_imports(&self, url: &str) -> Vec<String> {
        self.0.get(url).cloned().unwrap_or_default()
    }
}

/// wasm 插件图（M1）：wasm 插件只导入宿主 `cordis:core/context` 接口，
/// 不导入其它插件——全部叶子（无模块级导入）。
pub struct WasmLeafGraph;

impl ModuleGraph for WasmLeafGraph {
    fn get_imports(&self, _url: &str) -> Vec<String> {
        Vec::new()
    }
}

/// 分类结果（Algorithm 8）：`accepted`（可热替换）与 `declared`（拒绝）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    /// 可热替换的模块集。
    pub accepted: BTreeSet<String>,
    /// 拒绝（不可热替换 / 整机重启）的模块集。
    pub declined: BTreeSet<String>,
}

/// **Algorithm 8：模块分类**——stashed 与 externals 为种子，在导入图上
/// 做不动点：
///
/// ```text
/// accepted ← stashed; declined ← externals
/// 传播：模块一旦有导入 ∈ accepted → accepted；
///       全部导入 ⊆ declined → declined；
///       未决（含导入环）→ declined
/// ```
pub fn classify(
    stashed: &[String],
    externals: &[String],
    graph: &dyn ModuleGraph,
) -> Classification {
    let mut accepted: BTreeSet<String> = stashed.iter().cloned().collect();
    let mut declined: BTreeSet<String> = externals.iter().cloned().collect();
    let mut pending: BTreeSet<String> = BTreeSet::new();
    for url in stashed {
        for imp in graph.get_imports(url) {
            if !accepted.contains(&imp) && !declined.contains(&imp) {
                pending.insert(imp);
            }
        }
    }

    loop {
        let mut progress = false;
        let mut next_pending: BTreeSet<String> = BTreeSet::new();
        for url in pending {
            let imports = graph.get_imports(&url);
            if imports.iter().any(|i| accepted.contains(i)) {
                accepted.insert(url);
                progress = true;
            } else if imports.iter().all(|i| declined.contains(i)) {
                declined.insert(url);
                progress = true;
            } else {
                // 未决（论文 `pending ← pending ∪ (get_imports(url) ∖
                // (accepted ∪ declined))`）：url 自身保留 + 扩展导入。
                next_pending.insert(url);
                for imp in imports {
                    if !accepted.contains(&imp) && !declined.contains(&imp) {
                        next_pending.insert(imp);
                    }
                }
            }
        }
        pending = next_pending;
        if !progress {
            break;
        }
    }
    // 残留未决（含导入环）默认拒绝。
    declined.extend(pending);
    Classification { accepted, declined }
}

/// **Algorithm 9 的 `get_dependencies`**：`root` 的传递导入集，以
/// `declined` 为边界（遇 declined 即停）。
pub fn get_dependencies(
    root: &str,
    declined: &BTreeSet<String>,
    graph: &dyn ModuleGraph,
) -> BTreeSet<String> {
    fn traverse(
        url: &str,
        declined: &BTreeSet<String>,
        graph: &dyn ModuleGraph,
        deps: &mut BTreeSet<String>,
    ) {
        if deps.contains(url) || declined.contains(url) {
            return;
        }
        deps.insert(url.to_string());
        for child in graph.get_imports(url) {
            traverse(&child, declined, graph, deps);
        }
    }
    let mut deps = BTreeSet::new();
    traverse(root, declined, graph, &mut deps);
    deps
}

/// **Algorithm 9：过期条目检测**——条目的依赖树与 `accepted` 相交即过期；
/// 过期树的全部模块并入 `accepted`（下一阶段全部失效）。
pub fn detect(
    entries: &[String],
    classification: &Classification,
    graph: &dyn ModuleGraph,
) -> Vec<String> {
    let mut accepted = classification.accepted.clone();
    let mut stale = Vec::new();
    for entry in entries {
        let tree = get_dependencies(entry, &classification.declined, graph);
        if !tree.is_disjoint(&accepted) {
            accepted.extend(tree);
            stale.push(entry.clone());
        }
    }
    stale
}

/// 模块加载抽象（Alg 10 的 `import(url)`）：url → 新组件实例。
///
/// M2 实现：
/// - wasm：重读文件（新字节 → 新组件，WASI 快照）；
/// - native：编排方提供新版本组件映射（Rust 无 dlopen；生产化经
///   动态链接）。
pub trait ModuleLoader {
    /// 加载 `url` 的新版本组件（失败 → `Err`，触发事务回滚）。
    fn load(&self, url: &str) -> anyhow::Result<std::rc::Rc<dyn cordis_core::Component>>;
}

/// 事务性重载引擎（Algorithm 10）。
pub struct Hmr {
    loader: Rc<Loader>,
    modules: Box<dyn ModuleLoader>,
}

impl Hmr {
    /// 创建 HMR 引擎（共享 loader）。
    pub fn new(loader: Rc<Loader>, modules: Box<dyn ModuleLoader>) -> Self {
        Self { loader, modules }
    }

    /// **Algorithm 10：事务性模块重载**。
    ///
    /// ```text
    /// backup ← invalidate_caches(accepted)
    /// try:
    ///   for entry in stale_entries:
    ///     entry.fiber.dispose()
    ///     entry.fiber ← ctx.use(import(entry.url), entry.config)
    /// catch:
    ///   restore_caches(backup)
    ///   for entry in stale_entries:
    ///     entry.fiber.dispose()
    ///     entry.fiber ← ctx.use(backup[entry.url], entry.config)
    ///   throw
    /// ```
    ///
    /// 本实现的缓存 = loader 的组件注册表（url → 组件）；"失效并备份" =
    /// 取出旧组件；重载 = 注册新组件 + `apply`（revision 递增触发重建）；
    /// 失败检测 = 实例化后的 fiber 失败态（L-Raise，`Inactive(Some(ζ))`，
    /// M2-PR1）与加载错误；回滚 = 恢复旧组件注册 + 重新 apply。
    ///
    /// **事务的 panic 安全（REVIEW-4c6e7fc major1）**：`Loader::apply` 以
    /// panic 表达配置错误（供给冲突 / 未知组件 / 实例化失败）——整个事务
    /// 以 `catch_unwind` 包裹：**任何 panic 都先回滚再重抛**（panic = bug
    /// 纪律 + 永不半重载保证；回滚本身恢复旧组件 + 旧供给格局，不再触发
    /// 冲突）。
    pub fn reload(
        &self,
        stashed: &[String],
        externals: &[String],
        graph: &dyn ModuleGraph,
        desired: &[Entry],
    ) -> anyhow::Result<Vec<String>> {
        // 阶段 1：分类（Alg 8）。
        let classification = classify(stashed, externals, graph);
        // 阶段 2：过期条目（Alg 9）。url 去重（多条目共享同一组件名，
        // REVIEW-4c6e7fc nit2）。
        let entry_urls: BTreeSet<String> = desired.iter().map(|e| e.component.clone()).collect();
        let stale = detect(
            &entry_urls.into_iter().collect::<Vec<_>>(),
            &classification,
            graph,
        );
        if stale.is_empty() {
            return Ok(stale);
        }

        // 阶段 3：事务性重载（Alg 10）。
        // 备份：每个过期条目的旧组件（失效缓存）。
        let mut backup: HashMap<String, std::rc::Rc<dyn cordis_core::Component>> = HashMap::new();
        for url in &stale {
            if let Some(component) = self.loader.component_of(url) {
                backup.insert(url.clone(), component);
            }
        }

        let reload_all = || -> anyhow::Result<()> {
            for url in &stale {
                let component = self.modules.load(url)?;
                self.loader.register_component(url.clone(), component);
            }
            // revision 递增触发重建——**仅过期条目**（其余条目零操作、
            // 幂等；其他组件状态保留）。
            let bumped: Vec<Entry> = desired
                .iter()
                .map(|e| {
                    let mut e = e.clone();
                    if stale.contains(&e.component) {
                        e.revision += 1;
                    }
                    e
                })
                .collect();
            let report = self.loader.apply(&bumped);
            // 错误策略 v0.2：OrchestrationError（校验失败/供给冲突/未知
            // 组件）经 report 表现——不 panic，bail 触发回滚。
            // 只看 `Failed`（未挂载的 OrchestrationError：校验/供给/未知
            // 组件）——`FailedFiber`（已挂载 Inactive）由下方 L-Raise 检查
            // 处理（且带 stale 过滤，REVIEW-4c6e7fc nit3）。`Failed` 均为
            // 本次 apply 新产生（未挂载条目不入树），重载引入任何失败 →
            // 回滚（可能由 stale 组件跨条目供给变化引起，如 db 重载致 c
            // Clash——失败条目 id 不必 ∈ stale）。
            if report
                .outcomes
                .iter()
                .any(|o| matches!(o, EntryOutcome::Failed(_)))
            {
                anyhow::bail!("条目加载失败（OrchestrationError，校验/供给/组件问题，触发回滚）");
            }
            // 失败检测：**仅过期条目**的 fiber 失败态（L-Raise；非过期
            // 条目未重建、其既有失败 fiber 不得误判，REVIEW-4c6e7fc
            // nit3）。
            for entry in bumped.iter().filter(|e| stale.contains(&e.component)) {
                if let Some(fiber) = self.loader.fiber(&entry.id)
                    && matches!(&*fiber.state(), FiberState::Inactive(Some(_)))
                {
                    anyhow::bail!("条目 `{}` 重载后失败（组件运行失败，触发回滚）", entry.id);
                }
            }
            Ok(())
        };

        // 回滚：恢复旧组件注册 + 重新 apply。revision +2 是"回滚后的新
        // 基线"启发式（+1 已由失败尝试消费；若调用方随后以相同 desired
        // 重试，revision 继续单调，不会与既有加载态混淆——REVIEW-4c6e7fc
        // nit1）。
        let rollback = |backup: &HashMap<String, Rc<dyn cordis_core::Component>>| {
            for (url, component) in backup {
                self.loader
                    .register_component(url.clone(), Rc::clone(component));
            }
            let rolled: Vec<Entry> = desired
                .iter()
                .map(|e| {
                    let mut e = e.clone();
                    if stale.contains(&e.component) {
                        e.revision += 2;
                    }
                    e
                })
                .collect();
            self.loader.apply(&rolled);
        };

        // 事务：任何 panic（配置错误 = bug）都先回滚再重抛——永不半重载。
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(reload_all));
        match outcome {
            Ok(Ok(())) => Ok(stale),
            Ok(Err(err)) => {
                rollback(&backup);
                Err(err.context("事务性重载失败，已回滚到备份版本"))
            }
            Err(payload) => {
                rollback(&backup);
                std::panic::resume_unwind(payload);
            }
        }
    }

    /// 当前 loader 引用（测试/编排用）。
    pub fn loader(&self) -> &Loader {
        &self.loader
    }
}
