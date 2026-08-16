//! cordis 构建工具：把 wasip1 核心模块（如标准 go 编译的 guest）编码为
//! 组件模型组件，供 cordis-wasm 宿主加载。
//!
//! 用法：
//! ```text
//! componentize <core-module.wasm> <adapter.wasm> <wit-dir> <world-name> <output.wasm>
//! ```
//!
//! - `<core-module.wasm>`：wasip1 核心模块（`GOOS=wasip1 go build` 产物）。
//! - `<adapter.wasm>`：wasi preview1 → preview2 的 reactor 适配器，随仓库
//!   提供：`third_party/wasi-preview1-adapter/wasi_snapshot_preview1.reactor.wasm`
//!   （来自 wasi-preview1-component-adapter-provider 47.0.3，Apache-2.0 WITH
//!   LLVM-exception）。适配器把 guest 的 `wasi_snapshot_preview1.*` 导入映射
//!   为 wasip2 接口（时钟、随机等），使组件可直接用 wasmtime 47 的 wasip2
//!   链接器实例化。
//! - `<wit-dir>` + `<world-name>`：描述该模块接口的 WIT 世界（cordis 场景下是
//!   `crates/cordis-wasm/wit` + `cordis`）。核心模块本身不带组件元数据，需先把
//!   世界嵌入为 `component-type` 自定义段（`wasm-tools component embed` 的等价
//!   实现），编码器才能把导入/导出归类为接口导入/导出。

use std::process::ExitCode;

use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::Resolve;

/// 预览1 适配器的名字（`ComponentEncoder::adapter` 要求）。
const WASI_SNAPSHOT_PREVIEW1: &str = "wasi_snapshot_preview1";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(module_path), Some(adapter_path), Some(wit_dir), Some(world_name), Some(output_path)) = (
        args.next(),
        args.next(),
        args.next(),
        args.next(),
        args.next(),
    ) else {
        eprintln!(
            "用法: componentize <core-module.wasm> <adapter.wasm> <wit-dir> <world-name> <output.wasm>"
        );
        return ExitCode::FAILURE;
    };

    match run(
        &module_path,
        &adapter_path,
        &wit_dir,
        &world_name,
        &output_path,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("componentize 失败: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    module_path: &str,
    adapter_path: &str,
    wit_dir: &str,
    world_name: &str,
    output_path: &str,
) -> anyhow::Result<()> {
    let module = std::fs::read(module_path)?;
    let adapter = std::fs::read(adapter_path)?;

    // 解析 WIT 世界并嵌入为 component-type 自定义段。
    let mut resolve = Resolve::default();
    let (pkg, _) = resolve.push_dir(wit_dir)?;
    let world = resolve.select_world(&[pkg], Some(world_name))?;

    let mut wasm = module;
    embed_component_metadata(&mut wasm, &resolve, world, StringEncoding::UTF8)?;

    let mut encoder = ComponentEncoder::default().validate(true).module(&wasm)?;
    encoder = encoder.adapter(WASI_SNAPSHOT_PREVIEW1, &adapter)?;
    let component = encoder.encode()?;

    std::fs::write(output_path, component)?;
    eprintln!("组件已写出: {output_path}");
    Ok(())
}
