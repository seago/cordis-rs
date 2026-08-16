//! PR #12 审查 m1（REVIEW-54a9b08）：isolate × wasm 交叉回归测试。
//!
//! 固化 REVIEW-2a7a686 m1/m2 修复（`set_dyn`/`get_dyn` 按键判定 +
//! 内部 `resolve_realm`）在**隔离上下文 + wasm 组件**组合下的正确性：
//!
//! - wasm 提供者在隔离上下文激活 → 绑定落在**隔离 realm**（ρ 解析）；
//! - 同 realm 消费者注入满足激活、跨 realm 消费者保持 Inactive；
//! - 供给纪律按**键**判定（隔离后解析到 realm 仍合法）。

use cordis_core::FiberState;
use cordis_core::runtime::Runtime;
use cordis_core::symbol::Symbol;
use cordis_wasm::WasmComponent;
use std::rc::Rc;

fn load_guest(example: &str) -> Rc<WasmComponent> {
    let engine = wasmtime::Engine::default();
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join(format!(
        "../../examples/{example}/target/wasm32-wasip2/debug/{}.wasm",
        example.replace('-', "_")
    ));
    let bytes = std::fs::read(path).expect("先构建 guest（CI 步骤 build wasm guest）");
    WasmComponent::load(&engine, &bytes).expect("组件加载")
}

#[test]
fn isolated_wasm_provider_binds_to_realm() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    // 隔离：db → realm-a（provider 绑定应落 realm-a，而非键本身）。
    let ctx_a = root.isolate(Symbol::intern("db"), Symbol::intern("realm-a"));

    let provider = load_guest("wasm-plugin-rust");
    let p_fiber = ctx_a
        .use_component(provider, Rc::new(()))
        .expect("隔离上下文实例化 provider");
    assert!(
        matches!(&*p_fiber.state(), FiberState::Active { .. }),
        "wasm provider 在隔离上下文应激活"
    );

    // 绑定落在隔离 realm（ρ 解析由核心承担，REVIEW-2a7a686 m2）。
    assert!(
        runtime.store().contains(Symbol::intern("realm-a")),
        "绑定在隔离 realm"
    );
    assert!(
        !runtime.store().contains(Symbol::intern("db")),
        "键本身不被绑定（未隔离路径无绑定）"
    );

    // 供给纪律按键判定（m1）：realm 解析后仍合法（激活即 set 成功）。
    // Ref 为 Drop 类型、借用活到作用域末——块级隔离（不得跨 retire）。
    {
        let store = runtime.store();
        let value = store
            .get_value(Symbol::intern("realm-a"))
            .expect("realm-a 有绑定");
        assert!(
            value.is::<cordis_wasm::wit::cordis::core::context::Value>(),
            "值为 wit Value 装箱"
        );
    }

    // 退役 → 隔离 realm 绑定清除。
    p_fiber.retire();
    assert!(runtime.store().symbols().next().is_none(), "退役后绑定全清");
}

#[test]
fn wasm_consumer_isolation_blocks_cross_realm() {
    let runtime = Rc::new(Runtime::new());
    let root = runtime.context();
    let db = Symbol::intern("db");
    let realm_a = Symbol::intern("realm-a");
    let realm_b = Symbol::intern("realm-b");

    // provider 在 realm-a 激活。
    let ctx_a = root.isolate(db, realm_a);
    let provider = load_guest("wasm-plugin-rust");
    let p_fiber = ctx_a
        .use_component(provider, Rc::new(()))
        .expect("provider 实例化");
    assert!(matches!(&*p_fiber.state(), FiberState::Active { .. }));

    // 跨 realm 消费者（db → realm-b）：注入不满足 → Inactive。
    // （供给 derived 的键级不相交是 O-Insert 语义——断言后退役并移除
    // 该 consumer，释放 derived 供给名再注册同 realm consumer。）
    let ctx_b = root.isolate(db, realm_b);
    let consumer_b = load_guest("wasm-plugin-rust-consumer");
    let c_b = ctx_b
        .use_component(consumer_b, Rc::new(()))
        .expect("consumer-b 实例化");
    assert!(
        matches!(&*c_b.state(), FiberState::Inactive(_)),
        "跨 realm 注入不满足：保持 Inactive"
    );
    c_b.retire();
    runtime
        .remove_fiber(c_b.id())
        .expect("退役且 Inactive 后可移除");

    // 同 realm 消费者（db → realm-a）：注入满足 → 激活并派生 derived。
    let consumer_a = load_guest("wasm-plugin-rust-consumer");
    let c_a = ctx_a
        .use_component(consumer_a, Rc::new(()))
        .expect("consumer-a 实例化");
    assert!(
        matches!(&*c_a.state(), FiberState::Active { .. }),
        "同 realm 注入满足：激活"
    );
    assert!(
        runtime.store().contains(Symbol::intern("derived")),
        "derived 已绑定"
    );

    // 退役 provider → 同 realm consumer 级联停用。
    p_fiber.retire();
    assert!(
        matches!(&*c_a.state(), FiberState::Inactive(_)),
        "provider 退役 → 同 realm consumer 级联停用"
    );
    assert!(runtime.store().symbols().next().is_none(), "绑定全清");
}
