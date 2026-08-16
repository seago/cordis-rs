module wit_component

go 1.25

require go.bytecodealliance.org/pkg v0.2.2

// cordis-rs fork：移除 tinygo 专有符号 runtime.sbrk（标准 go 编译必需），
// 并补 runtime.Handle.TakeHandle（wit-bindgen 0.60 生成代码所需）。
replace go.bytecodealliance.org/pkg => ../../third_party/go-pkg
