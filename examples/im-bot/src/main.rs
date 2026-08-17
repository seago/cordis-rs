//! IM-bot 迷你案例（论文 §5.3 Koishi 式三层依赖拓扑，M3-PR1）。
//!
//! 拓扑：**adapter 层**（提供 `platform`，消息平台接入）→ **database
//! 层**（提供 `db`，持久存储）→ **功能插件**（`bot` 注入两者，提供
//! `reply`）。§5.3 的运行时操作直证：
//!
//! 1. **切换存储后端**（同一条目 revision 递增 → database 重建，新
//!    fiber 重新供给 `db` 键）：只重激活解析依赖变化的依赖者（bot 重激活）；
//!    adapter 不受影响（fiber 不变）；
//! 2. **重连 adapter**（退役 → 移除 → 重装）：bot 级联停用再自动重连；
//!    database 不受影响（fiber 不变）；
//! 3. **依赖不可用**：移除 adapter → bot 保持 Inactive（不报错）；
//!    adapter 重新出现 → bot 自动激活（§5.3 "stays inactive until it
//!    appears, without erroring"）。
//!
//! 运行：`cargo run -p im-bot`（全部断言通过即成功）。

use cordis::{Context, EffectIter, FiberState, Key, Runtime, component};
use cordis_core::symbol::Symbol;
use cordis_loader::{Entry, Loader};
use std::any::Any;
use std::rc::Rc;

// ── 三层拓扑的键 ──────────────────────────────────────────────────────

/// adapter 层：消息平台接入（共效应）。
struct PlatformKey;
impl Key for PlatformKey {
    type Value = String;
    const SYMBOL: &'static str = "platform";
}

/// database 层：持久存储（共效应）。
struct DbKey;
impl Key for DbKey {
    type Value = String;
    const SYMBOL: &'static str = "db";
}

/// 功能插件：回复（效应）。
struct ReplyKey;
impl Key for ReplyKey {
    type Value = String;
    const SYMBOL: &'static str = "reply";
}

// ── adapter 层组件（提供 platform）───────────────────────────────────

/// 消息平台适配器（config = 平台名）。
#[component(inject = [], provide = [PlatformKey])]
struct Adapter;

impl Adapter {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let name = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            ctx.set::<PlatformKey>(name).expect("绑定 platform")
        })))
    }
}

// ── database 层组件（提供 db）─────────────────────────────────────────

/// 存储后端（config = 后端名）。
#[component(inject = [], provide = [DbKey])]
struct Database;

impl Database {
    fn apply_impl(&self, ctx: Rc<Context>, config: &dyn Any) -> Box<dyn EffectIter> {
        let name = config
            .downcast_ref::<String>()
            .expect("config 为 String")
            .clone();
        Box::new(cordis::once(Box::new(move || {
            ctx.set::<DbKey>(name).expect("绑定 db")
        })))
    }
}

// ── 功能插件（注入 platform + db，提供 reply）─────────────────────────

/// bot：声明两层依赖为共效应并访问（§5.3 "functional plugins declare
/// these as coeffects and access them"）。
#[component(inject = [PlatformKey, DbKey], provide = [ReplyKey])]
struct Bot;

impl Bot {
    fn apply_impl(&self, ctx: Rc<Context>, _config: &dyn Any) -> Box<dyn EffectIter> {
        Box::new(cordis::once(Box::new(move || {
            let platform = ctx.get::<PlatformKey>().expect("platform 可用").clone();
            let db = ctx.get::<DbKey>().expect("db 可用").clone();
            ctx.set::<ReplyKey>(format!("reply({platform},{db})"))
                .expect("绑定 reply")
        })))
    }
}

fn entry(id: &str, component: &str, config: &str) -> Entry {
    Entry::new(id, component, Rc::new(config.to_string()), 0, false)
}

fn assert_quiet(runtime: &Runtime, what: &str) {
    assert!(runtime.is_quiet(), "{what}：应静止");
}

fn main() {
    let runtime = Rc::new(Runtime::new());
    let loader = Loader::new(Rc::clone(&runtime));
    loader.register_component("adapter", Rc::new(Adapter));
    loader.register_component("database", Rc::new(Database));
    loader.register_component("bot", Rc::new(Bot));

    // ── 初始装配：三层全激活 ────────────────────────────────────────
    loader.apply(&[
        entry("adapter", "adapter", "telegram"),
        entry("database", "database", "sqlite"),
        entry("bot", "bot", ""),
    ]);
    let adapter_first = loader.fiber("adapter").expect("adapter 激活").id();
    let database_first = loader.fiber("database").expect("database 激活").id();
    let bot_first = loader.fiber("bot").expect("bot 激活").id();
    let reply = {
        let store = runtime.store();
        store
            .get_value(Symbol::intern("reply"))
            .expect("reply 绑定")
            .downcast_ref::<String>()
            .expect("String")
            .clone()
    };
    assert_eq!(
        reply, "reply(telegram,sqlite)",
        "三层装配：bot 读取两层依赖"
    );
    assert_quiet(&runtime, "初始装配");

    // ── 1. 切换存储后端（sqlite → postgres：同一条目 revision 递增 ──
    //    → database 重建 → 新 fiber 重供 db 键）
    loader.apply(&[
        entry("adapter", "adapter", "telegram"),
        Entry::new(
            "database",
            "database",
            Rc::new("postgres".to_string()),
            1,
            false,
        ),
        entry("bot", "bot", ""),
    ]);
    // 只重激活解析依赖变化的依赖者（§5.3 "reactivates only the
    // dependents whose resolved dependency changed"）：bot 级联重激活
    //（条目未变 → fiber 不变），其 reply 反映新存储后端。
    let bot_switched = loader.fiber("bot").expect("bot 激活").id();
    assert_eq!(
        bot_switched, bot_first,
        "bot 条目未变 → fiber 不变（重激活）"
    );
    assert_eq!(
        loader.fiber("adapter").expect("adapter").id(),
        adapter_first,
        "adapter 不受影响（fiber 不变）"
    );
    let reply = {
        let store = runtime.store();
        store
            .get_value(Symbol::intern("reply"))
            .expect("reply 绑定")
            .downcast_ref::<String>()
            .expect("String")
            .clone()
    };
    assert_eq!(reply, "reply(telegram,postgres)", "bot 读到新存储后端");
    assert_quiet(&runtime, "切换存储后端");
    // 切换后 database 已重建（revision 变更）——作为后续场景的基线。
    let database_switched = loader.fiber("database").expect("database").id();
    assert_ne!(
        database_switched, database_first,
        "存储后端切换 → database 重建"
    );

    // ── 2. 重连 adapter（退役 → 移除 → 重装）─────────────────────────
    let adapter = loader.fiber("adapter").expect("adapter").clone();
    adapter.retire();
    loader.apply(&[
        Entry::new(
            "database",
            "database",
            Rc::new("postgres".to_string()),
            1,
            false,
        ),
        entry("bot", "bot", ""),
    ]);
    assert!(
        matches!(
            *loader.fiber("bot").expect("bot").state(),
            FiberState::Inactive(_)
        ),
        "adapter 断开 → bot 级联停用"
    );
    assert_eq!(
        loader.fiber("database").expect("database").id(),
        database_switched,
        "database 不受影响（fiber 不变）"
    );
    loader.apply(&[
        entry("adapter", "adapter", "telegram"),
        Entry::new(
            "database",
            "database",
            Rc::new("postgres".to_string()),
            1,
            false,
        ),
        entry("bot", "bot", ""),
    ]);
    assert!(
        matches!(
            *loader.fiber("bot").expect("bot").state(),
            FiberState::Active { .. }
        ),
        "adapter 重连 → bot 自动重连"
    );
    let reply = {
        let store = runtime.store();
        store
            .get_value(Symbol::intern("reply"))
            .expect("reply 绑定")
            .downcast_ref::<String>()
            .expect("String")
            .clone()
    };
    assert_eq!(reply, "reply(telegram,postgres)", "重连后回复恢复");
    assert_quiet(&runtime, "重连 adapter");

    // ── 3. 依赖不可用：bot 保持 Inactive 直到依赖出现 ───────────────
    let adapter2 = loader.fiber("adapter").expect("adapter").clone();
    adapter2.retire();
    loader.apply(&[
        Entry::new(
            "database",
            "database",
            Rc::new("postgres".to_string()),
            1,
            false,
        ),
        entry("bot", "bot", ""),
    ]);
    assert!(
        matches!(
            *loader.fiber("bot").expect("bot").state(),
            FiberState::Inactive(_)
        ),
        "依赖不可用 → bot Inactive（不报错）"
    );
    assert_quiet(&runtime, "依赖不可用");
    loader.apply(&[
        entry("adapter", "adapter", "discord"),
        Entry::new(
            "database",
            "database",
            Rc::new("postgres".to_string()),
            1,
            false,
        ),
        entry("bot", "bot", ""),
    ]);
    assert!(
        matches!(
            *loader.fiber("bot").expect("bot").state(),
            FiberState::Active { .. }
        ),
        "adapter 出现 → bot 自动激活（§5.3）"
    );
    let reply = {
        let store = runtime.store();
        store
            .get_value(Symbol::intern("reply"))
            .expect("reply 绑定")
            .downcast_ref::<String>()
            .expect("String")
            .clone()
    };
    assert_eq!(reply, "reply(discord,postgres)", "bot 连上新 adapter");
    assert_quiet(&runtime, "依赖重新出现");

    println!("✓ im-bot：三层拓扑案例全部断言通过（切换后端 / 重连 adapter / 依赖不可用）");
}
