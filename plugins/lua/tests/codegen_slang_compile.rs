// 生成した Slang を aperio-gpu(実際の Slang コンパイラ)でコンパイルできることを確かめる。
// Vulkan デバイスが無い環境ではスキップする。

mod support;

use aperio_gpu::compiled_shader::CompiledShader;
use aperio_gpu::image_generator::layout::InputArity;
use aperio_gpu::image_generator::ImageGenerator;
use aperio_gpu::rhi::{AddressMode, FilterMode, SamplerOptions};
use aperio_gpu::ImagePixelFormat;
use lua_gpu::codegen::slang;
use lua_gpu::diag::Diagnostics;
use lua_gpu::spec::Gen;
use support::*;

fn compile_all(reg: &lua_gpu::spec::Registry, src: &str) -> Vec<String> {
    let a = analyze_ok(reg, src);
    let mut diags = Diagnostics::new();
    let mut out = Vec::new();
    for k in &a.plan.kernels {
        let s = slang::generate(&a, k, reg, &mut diags).unwrap_or_else(|| panic!("codegen failed:\n{diags}"));
        out.push(s.source);
    }
    assert!(!diags.has_errors(), "{diags}");
    out
}

fn try_compile(name: &str, source: &str) {
    // LUA_GPU_DUMP=<ディレクトリ> で生成したシェーダーを書き出す(デバッグ用)。
    if let Some(dir) = std::env::var_os("LUA_GPU_DUMP") {
        let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.slang")), source);
    }
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba8Unorm) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping {name}: no Vulkan device ({e:#})");
            return;
        }
    };
    let sampler = SamplerOptions {
        address_mode: AddressMode::ClampToEdge,
        filter: FilterMode::Nearest,
    };
    if let Err(e) = CompiledShader::from_slang(
        name,
        source,
        "main",
        &generator,
        generator.image_format(),
        InputArity::Fixed,
        &[],
        &[],
        Some(&sampler),
    ) {
        panic!("{name} failed to compile: {e:#}\n----\n{source}");
    }
}

/// テスト用の obj API(実際の Slang を返す)。
fn real_obj_registry() -> lua_gpu::spec::Registry {
    registry(vec![
        gpu_fn(
            "obj.getpixel",
            &["float", "float"],
            &["float", "float", "float", "float"],
            std::sync::Arc::new(|_: &lua_gpu::spec::CallSite| {
                Ok(Some(Gen {
                    stmts: "float4 c = inputs.tex0.Load(int3(int(a1), int(a2), 0)) * 255.0;".into(),
                    exprs: vec!["c.r".into(), "c.g".into(), "c.b".into(), "c.a".into()],
                    defs: String::new(),
                }))
            }),
        ),
        gpu_fn(
            "obj.putpixel",
            &["float", "float", "float", "float", "float", "float"],
            &[],
            fixed(Gen {
                stmts: "res.outputTex[int2(int(a1), int(a2))] = float4(a3, a4, a5, a6) / 255.0;".into(),
                ..Gen::default()
            }),
        ),
        cpu_prop(
            "obj.w",
            fixed(Gen::expr("${w}")),
            vec![lua_gpu::spec::ExternalSpec::new("w", ty("int"))],
        ),
        cpu_prop(
            "obj.h",
            fixed(Gen::expr("${h}")),
            vec![lua_gpu::spec::ExternalSpec::new("h", ty("int"))],
        ),
    ])
}

#[test]
fn invert_kernel_compiles() {
    let src = r#"
local w, h = obj.w, obj.h
for y = 0, h - 1 do
  for x = 0, w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, 255 - r, 255 - g, 255 - b, a)
  end
end"#;
    for (i, s) in compile_all(&real_obj_registry(), src).iter().enumerate() {
        try_compile(&format!("lua_gpu_invert_{i}"), s);
    }
}

#[test]
fn rich_kernel_compiles() {
    // 捕捉した配列・レコード、カーネル内の配列、ユーザー関数(多値)、math / bit、
    // 持ち出し・記録、内側のループ、goto continue、while。
    let src = r#"
local weights = {1, 2, 1}
local conf = { gain = 1.5, offs = {-1, 0, 1} }
local mode = "add"
local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)), v end
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local acc = 0
    local tmp = {0, 0, 0}
    local jitter = 0
    if mode == "add" then jitter = math.random() end
    for i = 1, #weights do
      local r = obj.getpixel(x + conf.offs[i], y)
      tmp[i] = r * weights[i]
      acc = acc + tmp[i]
    end
    if acc < 0 then goto continue end
    local n = 0
    while n < 3 do n = n + 1 end
    local v, raw = clamp(acc / 4 * conf.gain, 0, 255)
    v = v + jitter
    local b = bit.band(math.floor(v), 0xff) % 256 + 2 ^ 2 - #tmp
    obj.putpixel(x, y, v, v, b, 255)
    ::continue::
  end
end"#;
    for (i, s) in compile_all(&real_obj_registry(), src).iter().enumerate() {
        try_compile(&format!("lua_gpu_rich_{i}"), s);
    }
}

#[test]
fn single_thread_kernel_compiles() {
    let src = "local r, g, b, a = obj.getpixel(0, 0)\nobj.putpixel(1, 1, r, g, b, a)";
    for (i, s) in compile_all(&real_obj_registry(), src).iter().enumerate() {
        try_compile(&format!("lua_gpu_single_{i}"), s);
    }
}
