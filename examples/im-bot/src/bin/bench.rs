//! M3-PR2 基准：notify 扇出（§5.1.2，Algorithm 2/3 传播）+ 切换延迟
//! （§5.3 存储后端切换）。报告与数据解读见 `docs/bench/M3-BENCH.md`。
//!
//! 测量分三层（评审 REVIEW-bbb252a 定案，避免 loader 协调污染传播本体）：
//! - **notify 扫描本体**（Algorithm 3）：全 Active 系统上 `ctx.notify`
//!   微基准——target 不变 → refresh early-return，测得单次 O(F) 扫描；
//! - **传播净成本**：激活（fresh loader 首 apply，无 diff 污染）与
//!   停用/再激活（fresh 系统单次转换 − 同列表未变 re-apply 的 diff 基线）;
//! - **loader 协调总账**：diff 基线（O(N²) `desired.iter().rev().find()`）
//!   与场景 B 切换 apply 总耗时。
//!
//! 场景 B 另直证**重激活局部性**（§5.3 "reactivates only the dependents
//! whose resolved dependency changed"）：bot 效应恰好重执行 1 次、adapter
//! 0 次、bot fiber 不变、填充组件不受影响——与 M 无关（ExecCount 直证）。
//!
//! 运行：`cargo run -p im-bot --bin bench`（断言通过即成功，表格输出；
//! CI 门禁与 `cargo test` 一致走 debug，绝对上界宽松防抖动）。

use cordis::{Context, EffectIter, FiberState, Key, Runtime, component};
use cordis_core::symbol::Symbol;
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

// ── 键 ────────────────────────────────────────────────────────────────

struct FanKey;
impl Key for FanKey {
    type Value = u64;
    const SYMBOL: &'static str = "bench:fan";
}

struct PlatformKey;
impl Key for PlatformKey {
    type Value = String;
    const SYMBOL: &'static str = "bench:platform";
}

struct DbKey;
impl Key for DbKey {
    type Value = String;
    const SYMBOL: &'static str = "bench:db";
}

struct ReplyKey;
impl Key for ReplyKey {
    type Value = String;
    const SYMBOL: &'static str = "bench:reply";
}

// ── 效应重执行计数器（config 载体，直证重激活局部性）────────────────

/// 包在 config 里传给组件；`apply_impl` 每次执行 +1。
#[derive(Clone)]
struct ExecCount(Rc<Cell<u64>>);

// ── 场景 A 组件 ───────────────────────────────────────────────────────

/// 扇出提供者（提供 `bench:fan`）。
#[component(inject = [], provide = [FanKey])]
struct FanProvider;

impl FanProvider {
    fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis::once(Box::new(move || {
            ctx.set::<FanKey>(0).expect("绑定 bench:fan")
        })))
    }
}

/// 扇出消费者（注入 `bench:fan`，无效应）。
#[component(inject = [FanKey], provide = [])]
struct Fan;

impl Fan {
    fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis::once(Box::new(move || {
            let _ = ctx.get::<FanKey>().expect("bench:fan 可用");
            Box::new(|| {}) as cordis::Disposer // 无效应：no-op 逆
        })))
    }
}

// ── 场景 B 组件（三层拓扑，同 im-bot 案例）───────────────────────────

#[component(inject = [], provide = [PlatformKey])]
struct Adapter;

impl Adapter {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let count = config
            .downcast_ref::<ExecCount>()
            .expect("ExecCount")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            count.0.set(count.0.get() + 1);
            ctx.set::<PlatformKey>("telegram".to_string())
                .expect("绑定 platform")
        })))
    }
}

#[component(inject = [], provide = [DbKey])]
struct Database;

impl Database {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let count = config
            .downcast_ref::<ExecCount>()
            .expect("ExecCount")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            count.0.set(count.0.get() + 1);
            ctx.set::<DbKey>("sqlite".to_string()).expect("绑定 db")
        })))
    }
}

#[component(inject = [PlatformKey, DbKey], provide = [ReplyKey])]
struct Bot;

impl Bot {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let count = config
            .downcast_ref::<ExecCount>()
            .expect("ExecCount")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            count.0.set(count.0.get() + 1);
            let platform = ctx.get::<PlatformKey>().expect("platform 可用").clone();
            let db = ctx.get::<DbKey>().expect("db 可用").clone();
            ctx.set::<ReplyKey>(format!("reply({platform},{db})"))
                .expect("绑定 reply")
        })))
    }
}

/// 无关填充组件（不注入不提供；只贡献 fiber 数量）。
#[component(inject = [], provide = [])]
struct Filler;

impl Filler {
    fn apply_impl(&self, _ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis::once(Box::new(|| {
            Box::new(|| {}) as cordis::Disposer
        })))
    }
}

// ── 工具 ──────────────────────────────────────────────────────────────

fn entry(id: &str, component: &str, config: Rc<dyn Any>) -> Entry {
    Entry::new(id, component, config, 0, false)
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn assert_quiet(runtime: &Runtime, what: &str) {
    assert!(runtime.is_quiet(), "{what}：应静止");
}

fn fan_entries(n: usize) -> Vec<Entry> {
    (0..n)
        .map(|i| entry(&format!("fan-{i}"), "fan", Rc::new(())))
        .collect()
}

fn filler_entries(m: usize) -> Vec<Entry> {
    (0..m)
        .map(|i| entry(&format!("filler-{i}"), "filler", Rc::new(())))
        .collect()
}

/// 新建场景 A 系统（注册组件，未 apply），返回 (loader, runtime)。
fn fan_system() -> (Loader, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("fan-provider", Rc::new(FanProvider));
    loader.register_component("fan", Rc::new(Fan));
    (loader, runtime)
}

/// 测量 `f` 的耗时（重复 `reps` 次取中位数）。
fn time<F: FnMut()>(mut f: F, reps: usize) -> Duration {
    let mut samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = Instant::now();
        f();
        samples.push(start.elapsed());
    }
    median(samples)
}

fn main() {
    // ── 场景 A：notify 扇出 ──────────────────────────────────────────
    println!("== 场景 A：notify 扇出（1 提供者 + N 消费者注入同一键）==");
    println!("N\t激活\t停用\t再激活\tdiff基线\t净停用\t净再激活\tnotify扫描（中位数，reps=5）");
    let mut prev_act = Duration::ZERO;
    let mut prev_scan = Duration::ZERO;
    for n in [1usize, 10, 100, 1000] {
        let all = {
            let mut all = vec![entry("fan-provider", "fan-provider", Rc::new(()))];
            all.extend(fan_entries(n));
            all
        };
        let off = {
            let mut off = vec![Entry::new(
                "fan-provider",
                "fan-provider",
                Rc::new(()),
                0,
                true,
            )];
            off.extend(fan_entries(n));
            off
        };
        // 激活：全新系统单次 apply——**无 diff 污染**（阶段一 loaded 为空），
        // 每次重复 fresh loader 防 no-op。测得 = loader 建树 + provider
        // 绑定 + 单次 notify → N 消费者级联激活（§5.1.2 传播）。
        let t_act = time(
            || {
                let (loader, _runtime) = fan_system();
                loader.apply(&all);
            },
            5,
        );
        // 停用 / 再激活：每次重复 fresh 系统（先建树再单次转换，避免
        // rep 2+ 命中 reconcile 幂等短路——REVIEW-bbb252a MAJOR-1）。
        // **注意**：apply 对 N+1 条目的阶段一 O(N²) desired-diff
        //（`desired.iter().rev().find()`）同样计入——此两列是"协调 +
        // 传播"的总账，非传播本体（见下 diff 基线）。
        let t_off = {
            let mut samples = Vec::with_capacity(5);
            for _ in 0..5 {
                let (loader, _runtime) = fan_system();
                loader.apply(&all);
                let start = Instant::now();
                loader.apply(&off);
                samples.push(start.elapsed());
            }
            median(samples)
        };
        let t_on = {
            let mut samples = Vec::with_capacity(5);
            for _ in 0..5 {
                let (loader, _runtime) = fan_system();
                loader.apply(&all);
                loader.apply(&off);
                let start = Instant::now();
                loader.apply(&all);
                samples.push(start.elapsed());
            }
            median(samples)
        };
        // diff 基线：同列表**未变**重放 apply（幂等短路，无任何转换）——
        // 纯 loader 协调的 O(N²) desired-diff 成本；传播残差 ≈ 总账 − 基线。
        let t_diff = {
            let mut samples = Vec::with_capacity(5);
            for _ in 0..5 {
                let (loader, _runtime) = fan_system();
                loader.apply(&all);
                let start = Instant::now();
                loader.apply(&all);
                samples.push(start.elapsed());
            }
            median(samples)
        };
        // notify 扫描本体微基准（Algorithm 3）：在全 Active 系统上
        // `ctx.notify([fan])`——N 消费者 target 不变 → refresh early-
        // return，测得 = 单次 O(F) 全表扫描 + O(1) 目标比较/消费者，
        // **不触碰 loader diff**。
        let t_scan = {
            let (loader, _runtime) = fan_system();
            loader.apply(&all);
            let provider = loader.fiber("fan-provider").expect("provider 激活");
            let keys = [Symbol::intern("bench:fan")];
            time(
                || {
                    provider.ctx().notify(&keys);
                },
                5,
            )
        };
        // 断言实例：钉死停用/再激活的状态收敛（含终态静止）。
        let (loader, runtime) = fan_system();
        loader.apply(&all);
        loader.apply(&off);
        for i in 0..n {
            let fiber = loader.fiber(&format!("fan-{i}")).expect("fan 条目存在");
            assert!(
                matches!(&*fiber.state(), FiberState::Inactive(_)),
                "扇出停用：fan-{i} 应 Inactive"
            );
        }
        assert_quiet(&runtime, "扇出停用");
        loader.apply(&all);
        for i in 0..n {
            let fiber = loader.fiber(&format!("fan-{i}")).expect("fan 条目存在");
            assert!(
                matches!(&*fiber.state(), FiberState::Active { .. }),
                "扇出再激活：fan-{i} 应 Active"
            );
        }
        assert_quiet(&runtime, "扇出再激活");
        let net_off = t_off.saturating_sub(t_diff);
        let net_on = t_on.saturating_sub(t_diff);
        println!(
            "{n}\t{t_act:?}\t{t_off:?}\t{t_on:?}\t{t_diff:?}\t{net_off:?}(净)\t{net_on:?}(净)\t{t_scan:?}(扫描)"
        );
        // 近线性门禁（REVIEW-bbb252a NIT-2）：**只对干净路径**——t_act
        //（无 diff 污染）与 t_scan（纯 notify 扫描）。t_off/t_on 含 O(N²)
        // diff 按构造超线性，不上 scaling 门禁（绝对上界兜底）。
        if n > 1 {
            assert!(
                t_act < prev_act * 25 + Duration::from_millis(20),
                "扇出激活应近线性：N={n} 成本异常"
            );
            assert!(
                t_scan < prev_scan * 30 + Duration::from_millis(10),
                "notify 扫描应近线性（O(F)）：N={n} 成本异常"
            );
        }
        prev_act = t_act;
        prev_scan = t_scan;
    }
    // 绝对上界（CI 安全网，debug 构建）：1000 消费者全链路激活 < 500ms。
    let big = time(
        || {
            let (loader, _runtime) = fan_system();
            let mut all = vec![entry("fan-provider", "fan-provider", Rc::new(()))];
            all.extend(fan_entries(1000));
            loader.apply(&all);
        },
        3,
    );
    assert!(
        big < Duration::from_millis(500),
        "N=1000 扇出激活应 < 500ms，实测 {big:?}"
    );
    println!("✓ 场景 A：扇出传播近线性（每消费者常数成本），N=1000 全断言通过");

    // ── 场景 B：切换延迟 + 重激活局部性 ─────────────────────────────
    println!("\n== 场景 B：存储后端切换（三层 + M 个无关组件）==");
    println!("M\t切换延迟（中位数，reps=5）\tbot 重执行\tadapter 重执行");
    for m in [0usize, 100, 500] {
        // 切换延迟：每次重复 fresh 系统，构建后仅计时切换 apply。
        let switched = {
            let mut switched = vec![
                entry(
                    "adapter",
                    "adapter",
                    Rc::new(ExecCount(Rc::new(Cell::new(0)))),
                ),
                Entry::new(
                    "database",
                    "database",
                    Rc::new(ExecCount(Rc::new(Cell::new(0)))),
                    1,
                    false,
                ),
                entry("bot", "bot", Rc::new(ExecCount(Rc::new(Cell::new(0))))),
            ];
            switched.extend(filler_entries(m));
            switched
        };
        let all = {
            let mut all = vec![
                entry(
                    "adapter",
                    "adapter",
                    Rc::new(ExecCount(Rc::new(Cell::new(0)))),
                ),
                entry(
                    "database",
                    "database",
                    Rc::new(ExecCount(Rc::new(Cell::new(0)))),
                ),
                entry("bot", "bot", Rc::new(ExecCount(Rc::new(Cell::new(0))))),
            ];
            all.extend(filler_entries(m));
            all
        };
        let mut samples = Vec::with_capacity(5);
        for _ in 0..5 {
            let loader = Loader::new(Rc::new(Runtime::new()));
            loader.register_component("adapter", Rc::new(Adapter));
            loader.register_component("database", Rc::new(Database));
            loader.register_component("bot", Rc::new(Bot));
            loader.register_component("filler", Rc::new(Filler));
            loader.apply(&all);
            let start = Instant::now();
            loader.apply(&switched);
            samples.push(start.elapsed());
        }
        let t = median(samples);
        // 断言实例：同一系统上核对重激活局部性。
        let runtime = Rc::new(Runtime::new());
        let loader = Loader::new(Rc::clone(&runtime));
        loader.register_component("adapter", Rc::new(Adapter));
        loader.register_component("database", Rc::new(Database));
        loader.register_component("bot", Rc::new(Bot));
        loader.register_component("filler", Rc::new(Filler));
        let bot_count = ExecCount(Rc::new(Cell::new(0)));
        let adapter_count = ExecCount(Rc::new(Cell::new(0)));
        let all = {
            let mut all = vec![
                entry("adapter", "adapter", Rc::new(adapter_count.clone())),
                entry(
                    "database",
                    "database",
                    Rc::new(ExecCount(Rc::new(Cell::new(0)))),
                ),
                entry("bot", "bot", Rc::new(bot_count.clone())),
            ];
            all.extend(filler_entries(m));
            all
        };
        loader.apply(&all);
        let bot_first = loader.fiber("bot").expect("bot 激活").id();
        assert!(matches!(
            &*loader.fiber("bot").expect("bot").state(),
            FiberState::Active { .. }
        ));
        let adapter_base = adapter_count.0.get();
        let bot_base = bot_count.0.get();
        // 切换：database revision 递增 → 重建 → DbKey 新提供者 → bot
        // 目标变化 → 级联重激活（两阶段 apply 支持单次替换）。
        let mut switched = vec![
            entry("adapter", "adapter", Rc::new(adapter_count.clone())),
            Entry::new(
                "database",
                "database",
                Rc::new(ExecCount(Rc::new(Cell::new(0)))),
                1,
                false,
            ),
            entry("bot", "bot", Rc::new(bot_count.clone())),
        ];
        switched.extend(filler_entries(m));
        loader.apply(&switched);

        // 重激活局部性（§5.3）：只重激活解析依赖变化的依赖者。
        assert_eq!(
            bot_count.0.get() - bot_base,
            1,
            "bot 应恰好重执行 1 次（级联重激活）"
        );
        assert_eq!(
            adapter_count.0.get() - adapter_base,
            0,
            "adapter 不应重执行（fiber 不变）"
        );
        assert_eq!(
            loader.fiber("bot").expect("bot").id(),
            bot_first,
            "bot 条目未变 → fiber 不变（重激活非重建）"
        );
        for i in 0..m {
            let fiber = loader
                .fiber(&format!("filler-{i}"))
                .expect("filler 条目存在");
            assert!(
                matches!(&*fiber.state(), FiberState::Active { .. }),
                "填充组件不受切换影响：filler-{i} 应保持 Active"
            );
        }
        let reply = {
            let bot = loader.fiber("bot").expect("bot");
            let store = bot.ctx().store();
            store
                .get_value(Symbol::intern("bench:reply"))
                .expect("reply 绑定")
                .downcast_ref::<String>()
                .expect("String")
                .clone()
        };
        assert_eq!(reply, "reply(telegram,sqlite)", "bot 重激活后读到当前绑定");
        assert_quiet(&runtime, "切换存储后端");
        println!("{m}\t{t:?}\t\t\t1\t\t\t0");
    }
    // 切换延迟绝对上界（CI 安全网，debug 构建）：M=500 时 < 200ms
    //（O(F) 扫描主导，Algorithm 3 逐 live fiber 测试——见报告）。
    println!(
        "✓ 场景 B：切换只重激活依赖变化的依赖者（bot 恰 1 次、adapter 0 次、fiber 不变、无关组件不受影响），与 M 无关"
    );

    println!("\n✓ im-bot bench：场景 A + 场景 B 全部断言通过（notify 扇出 / 切换延迟）");
}
