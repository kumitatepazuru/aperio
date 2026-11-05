// GPU 差分テスト(忠実性の確認)。
//
// - 参照: 元のスクリプトをそのまま LuaJIT で 1 回実行する(obj.getpixel / putpixel は CPU の画像バッファ)。
// - 変換: convert → 実行時(CPU 用 Lua)→ ディスパッチ列を aperio-gpu で実際に GPU 実行する。
// 両方の画像(8 bit)を比べる。Vulkan デバイスが無い環境ではスキップする。
//
// getpixel は自分のピクセル(または直前のカーネルの結果)だけを読むスクリプトにしている
// (同じループ内で書き込み済みの近傍を読む場合の意味の違いは、独自実装側で扱う予定のため)。

mod support;

use std::sync::{Arc, Mutex};

use aperio_gpu::compiled_shader::CompiledShader;
use aperio_gpu::image_generate_builder::ImageGenerateBuilder;
use aperio_gpu::image_generator::layout::InputArity;
use aperio_gpu::image_generator::ImageGenerator;
use aperio_gpu::rhi::{AddressMode, FilterMode, SamplerOptions};
use aperio_gpu::ImagePixelFormat;
use indexmap::IndexMap;
use lua_gpu::runtime::{new_lua_state, ExtValue, Runtime};
use lua_gpu::spec::{CallSite, ExternalSpec, Gen, Registry};
use support::*;

const W: u32 = 24;
const H: u32 = 16;

/// 入力画像のパターン(0〜255 のスケール)。GPU 側のパターン生成シェーダーと同じ式。
fn pattern(x: u32, y: u32) -> [f64; 4] {
    [
        (x * 10 % 256) as f64,
        (y * 15 % 256) as f64,
        ((x + y) * 7 % 256) as f64,
        255.0,
    ]
}

const PATTERN_SLANG: &str = r#"
struct EffectResources {
    [vk::image_format(APERIO_IMAGE_FORMAT)]
    RWTexture2D<float4> outputTex;
};
[vk::binding(0, 1)]
ParameterBlock<EffectResources> res;
[shader("compute")]
[numthreads(16, 16, 1)]
void main(uint3 gid : SV_DispatchThreadID) {
    uint w, h;
    res.outputTex.GetDimensions(w, h);
    if (gid.x >= w || gid.y >= h) return;
    uint x = gid.x; uint y = gid.y;
    res.outputTex[gid.xy] = float4(float(x * 10u % 256u), float(y * 15u % 256u), float((x + y) * 7u % 256u), 255.0) / 255.0;
}
"#;

fn obj_registry_real() -> Registry {
    registry(vec![
        gpu_fn(
            "obj.getpixel",
            &["float", "float"],
            &["float", "float", "float", "float"],
            Arc::new(|_: &CallSite| {
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
        cpu_prop("obj.w", fixed(Gen::expr("${w}")), vec![ExternalSpec::new("w", ty("int"))]),
        cpu_prop("obj.h", fixed(Gen::expr("${h}")), vec![ExternalSpec::new("h", ty("int"))]),
    ])
}

type Image = Vec<[f64; 4]>;

/// 参照: 元のスクリプトを LuaJIT でそのまま実行する。
fn reference(src: &str) -> Image {
    let img: Arc<Mutex<Image>> = Arc::new(Mutex::new(
        (0..H).flat_map(|y| (0..W).map(move |x| pattern(x, y))).collect(),
    ));
    let lua = new_lua_state();
    let obj = lua.create_table().unwrap();
    obj.set("w", W).unwrap();
    obj.set("h", H).unwrap();
    let i1 = img.clone();
    obj.set(
        "getpixel",
        lua.create_function(move |_, (x, y): (f64, f64)| {
            let (x, y) = (x.trunc() as i64, y.trunc() as i64);
            if x < 0 || y < 0 || x >= W as i64 || y >= H as i64 {
                return Ok((0.0, 0.0, 0.0, 0.0));
            }
            let p = i1.lock().unwrap()[(y as u32 * W + x as u32) as usize];
            Ok((p[0], p[1], p[2], p[3]))
        })
        .unwrap(),
    )
    .unwrap();
    let i2 = img.clone();
    obj.set(
        "putpixel",
        lua.create_function(move |_, (x, y, r, g, b, a): (f64, f64, f64, f64, f64, f64)| {
            let (x, y) = (x.trunc() as i64, y.trunc() as i64);
            if x >= 0 && y >= 0 && x < W as i64 && y < H as i64 {
                i2.lock().unwrap()[(y as u32 * W + x as u32) as usize] = [r, g, b, a];
            }
            Ok(())
        })
        .unwrap(),
    )
    .unwrap();
    lua.globals().set("obj", obj).unwrap();
    lua.load(src).set_name("=reference").exec().unwrap();
    let out = img.lock().unwrap().clone();
    out
}

/// 変換して GPU で実行する。Vulkan が無ければ None。
fn on_gpu(src: &str) -> Option<Image> {
    let generator = match ImageGenerator::new(ImagePixelFormat::Rgba32Float) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU differential test: no Vulkan device ({e:#})");
            return None;
        }
    };
    let reg = obj_registry_real();
    let out = lua_gpu::convert(src, "main.lua", None, &reg);
    let errors: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| d.severity == lua_gpu::diag::Severity::Error)
        .map(|d| d.to_string())
        .collect();
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let c = out.result.unwrap();
    support::dump::dump("differential.lua", &c.lua);
    for (i, s) in c.shaders.iter().enumerate() {
        support::dump::dump(&format!("differential_{i}.slang"), s);
    }

    let sampler = SamplerOptions {
        address_mode: AddressMode::ClampToEdge,
        filter: FilterMode::Nearest,
    };
    let pattern_shader = CompiledShader::from_slang(
        "lua_gpu_diff_pattern",
        PATTERN_SLANG,
        "main",
        &generator,
        generator.image_format(),
        InputArity::Fixed,
        &[],
        &[],
        None,
    )
    .expect("pattern shader");
    // 名前はパイプラインキャッシュのキーなので、ソースごとに変える。
    let tag = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        src.hash(&mut h);
        h.finish()
    };
    let kernels: Vec<Arc<CompiledShader>> = c
        .shaders
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Arc::new(
                CompiledShader::from_slang(
                    &format!("lua_gpu_diff_{tag:x}_{i}"),
                    s,
                    "main",
                    &generator,
                    generator.image_format(),
                    InputArity::Fixed,
                    &[],
                    &[],
                    Some(&sampler),
                )
                .unwrap_or_else(|e| panic!("kernel {i} failed to compile: {e:#}\n{s}")),
            )
        })
        .collect();

    let rt = Runtime::new(&reg).unwrap();
    let mut ext = IndexMap::new();
    ext.insert("w".to_string(), ExtValue::Number(W as f64));
    ext.insert("h".to_string(), ExtValue::Number(H as f64));
    let mut builder = ImageGenerateBuilder::new().add_shader(pattern_shader, None, W, H);
    let mut steps = Vec::new();
    rt.run(&c.lua, &ext, &mut |k, bytes| {
        steps.push((k, bytes));
        Ok(())
    })
    .unwrap_or_else(|e| panic!("run failed: {e}\n{}", c.lua));
    for (k, bytes) in steps {
        builder = builder.add_shader(kernels[k as usize].clone(), Some(bytes), W, H);
    }
    let rt_tokio = tokio::runtime::Runtime::new().unwrap();
    let bytes = rt_tokio.block_on(generator.generate_buf(builder)).expect("gpu run");
    Some(
        bytes
            .chunks_exact(4)
            .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64])
            .collect(),
    )
}

/// 0〜255 のスケールの値を、GPU の最終変換(clamp → * 255 → 切り捨て)と同じく 8 bit にする。
fn quantize(v: f64) -> f64 {
    ((v / 255.0).clamp(0.0, 1.0) * 255.0).floor()
}

fn check(name: &str, src: &str) {
    let Some(gpu) = on_gpu(src) else { return };
    let cpu = reference(src);
    let mut bad = Vec::new();
    for (i, (g, c)) in gpu.iter().zip(cpu.iter()).enumerate() {
        for ch in 0..4 {
            let want = quantize(c[ch]);
            if (g[ch] - want).abs() > 1.0 {
                bad.push(format!(
                    "pixel ({}, {}) channel {ch}: gpu {} vs lua {} (raw {})",
                    i as u32 % W,
                    i as u32 / W,
                    g[ch],
                    want,
                    c[ch]
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{name}: {} mismatches, e.g.\n{}",
        bad.len(),
        bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn invert() {
    check(
        "invert",
        r#"
local w, h = obj.w, obj.h
for y = 0, h - 1 do
  for x = 0, w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, 255 - r, 255 - g, 255 - b, a)
  end
end"#,
    );
}

#[test]
fn lua_arithmetic_semantics() {
    // % / ^ / max / min / floor / ceil / fmod / modf / and-or / not / bit を色に書き出す。
    check(
        "arithmetic",
        r#"
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local a = (x - 12) % 7            -- 負の数の floor 剰余
    local b = (-2) ^ (y % 4)          -- 負の底と整数指数
    local c = math.max(x, y, 5) + math.min(x, y) * 2
    local d = math.fmod(x - 10, 3) + 3
    local ip, fp = math.modf((x - 11) / 4)
    local e = (x > y and 30 or 60) + ((x % 2 == 0) and 0 or 1)
    local f = (not (x == y)) and 100 or 50
    local g = bit.band(x * 37, 0xf0) + bit.bxor(y, 5)
    obj.putpixel(x, y, a * 30 + b, c + d * 10, (ip + fp) * 10 + 100 + e, f + g)
  end
end"#,
    );
}

#[test]
fn functions_tables_and_inner_loops() {
    check(
        "functions",
        r#"
local weights = {1, 2, 1}
local conf = { gain = 1.5, base = 10 }
local function blend(v, k) return v * k + conf.base, v end
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    local acc = 0
    for i = 1, #weights do
      acc = acc + weights[i] * (i == 2 and r or g)
    end
    local tmp = {r, g}
    tmp[3] = b
    local n = 0
    while n < (x % 3) do n = n + 1 end
    local v, raw = blend(acc / 4, conf.gain)
    -- 書き込まれないピクセルの扱いはホスト任せ(S3)なので、飛ばす前にも書き込む。
    if x == y then obj.putpixel(x, y, 0, 0, 0, 255) goto continue end
    obj.putpixel(x, y, v, raw + n * 10, #tmp * 20 + tmp[3] / 4, a)
    ::continue::
  end
end"#,
    );
}

#[test]
fn cpu_replay_with_state_and_random() {
    // カウンター(CPU の状態)と乱数(LuaJIT の乱数列)は CPU で再生し、反復ごとに記録する。
    check(
        "replay",
        r#"
math.randomseed(42)
local count = 0
local mode = "add"
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    count = count + 1
    local noise = math.random(0, 50)
    local r, g, b, a = obj.getpixel(x, y)
    if mode == "add" then
      obj.putpixel(x, y, r + noise, count % 256, b, a)
    else
      obj.putpixel(x, y, r - noise, count % 256, b, a)
    end
  end
end
assert(count == obj.w * obj.h)"#,
    );
}

#[test]
fn random_count_inner_loop() {
    check(
        "random-count",
        r#"
math.randomseed(7)
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local v = 0
    for k = 1, math.random(1, 4) do
      v = v + math.random(0, 40)
    end
    obj.putpixel(x, y, v, 0, 0, 255)
  end
end"#,
    );
}

#[test]
fn hoisted_file_read() {
    let path = std::env::temp_dir().join(format!("lua_gpu_gain_{}.txt", std::process::id()));
    std::fs::write(&path, "0.5").unwrap();
    let p = path.display().to_string().replace('\\', "/");
    let src = format!(
        r#"
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local gain = tonumber(io.open("{p}"):read("*a"))
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, r * gain, g, b, a)
  end
end"#
    );
    check("hoisted-file", &src);
    std::fs::remove_file(&path).ok();
}

#[test]
fn two_kernels_in_sequence() {
    // 2 つ目のカーネルは 1 つ目の結果を読む(ディスパッチ列の順に実行される)。
    check(
        "two-kernels",
        r#"
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, g, b, r, a)
  end
end
for y = 0, obj.h - 1 do
  for x = 0, obj.w - 1 do
    local r, g, b, a = obj.getpixel(x, y)
    obj.putpixel(x, y, r / 2, g, b + 20, a)
  end
end"#,
    );
}

/// 標準ライブラリのうち GPU で実装する関数(分類 A / B)を 1 つずつ LuaJIT と比べる。
#[test]
fn stdlib_functions_on_gpu() {
    // u は (0, 1]、v は [-2, 2) の範囲。出力が 0〜255 付近に収まるように拡大・平行移動している。
    let cases: &[(&str, &str)] = &[
        ("abs", "math.abs(v * 40)"),
        ("acos", "math.acos(u * 2 - 1) * 60"),
        ("asin", "math.asin(u * 2 - 1) * 60 + 100"),
        ("atan", "math.atan(v) * 60 + 100"),
        ("atan2", "math.atan2(v, u - 0.5) * 30 + 100"),
        ("ceil", "math.ceil(v * 10 + 0.5) + 100"),
        ("floor", "math.floor(v * 10 + 0.5) + 100"),
        ("cos", "math.cos(v) * 100 + 120"),
        ("cosh", "math.cosh(v) * 20"),
        ("sin", "math.sin(v) * 100 + 120"),
        ("sinh", "math.sinh(v) * 20 + 100"),
        ("tan", "math.tan(u) * 50"),
        ("tanh", "math.tanh(v) * 100 + 120"),
        ("exp", "math.exp(u * 5)"),
        ("sqrt", "math.sqrt(u) * 200"),
        ("log10", "math.log10(u * 100) * 100"),
        ("log", "math.log(u * 10) * 50"),
        ("log2arg", "math.log(u * 100, 10) * 50 + math.log(u * 8, 2) * 10"),
        ("ldexp", "math.ldexp(u, 3) * 20"),
        ("deg", "math.deg(u) * 2"),
        ("rad", "math.rad(x * 10) * 20"),
        ("fmod", "math.fmod(v * 10, 3) * 30 + 100"),
        ("pow", "math.pow(v, 3) * 10 + 120"),
        ("pow-op", "(v * 2) ^ 3 + 120"),
        ("mod-op", "(v * 13) % 7 * 30"),
        ("max-min", "math.max(v, u, -1) * 50 + math.min(u, v * 2) * 20 + 100"),
        ("modf", "select(1, math.modf(v * 3.3)) * 20 + select(2, math.modf(v * 3.3)) * 50 + 100"),
        ("frexp", "select(1, math.frexp(u * 100)) * 200 + select(2, math.frexp(u * 100)) * 10"),
        ("tobit", "bit.tobit(x * 1.5) * 4"),
        ("bnot", "bit.rshift(bit.bnot(x), 24)"),
        ("band-bor-bxor", "bit.band(x * 37, 0xf0) + bit.bor(y, 3) + bit.bxor(x, y, 1)"),
        ("shifts", "bit.band(bit.lshift(x, 3), 255) + bit.arshift(-x * 8, 2) + 128"),
        ("rol-ror", "bit.band(bit.rol(x, 4), 255) + bit.rshift(bit.ror(x, 4), 28)"),
        ("bswap", "bit.rshift(bit.bswap(x + y * 3), 24)"),
        ("difftime", "os.difftime(x, y) + 128"),
        ("string-const", "string.len(\"hello\") * 20 + string.byte(\"ABC\", x % 3 + 1)"),
        ("type-fold", "(type(x) == \"number\" and 200 or 0) + (type(x) == \"string\" and 50 or 0)"),
        ("raw", "(rawequal(x, y) and 255 or 0) + tonumber(x)"),
        ("pi-huge", "math.pi * 30 + (math.huge > 1e30 and 20 or 0)"),
    ];
    let mut failures = Vec::new();
    for (name, expr) in cases {
        let src = format!(
            "for y = 0, obj.h - 1 do for x = 0, obj.w - 1 do\n  local u = (x + 1) / obj.w\n  local v = (y - obj.h / 2) / 4\n  obj.putpixel(x, y, {expr}, 0, 0, 255)\nend end"
        );
        // select は GPU では使えないので、modf / frexp は多値を受け取る形に書き換える。
        let src = if expr.contains("select(") {
            let f = if *name == "modf" { "math.modf(v * 3.3)" } else { "math.frexp(u * 100)" };
            let (k1, k2) = if *name == "modf" { (20, 50) } else { (200, 10) };
            let off = if *name == "modf" { 100 } else { 0 };
            format!(
                "for y = 0, obj.h - 1 do for x = 0, obj.w - 1 do\n  local u = (x + 1) / obj.w\n  local v = (y - obj.h / 2) / 4\n  local a, b = {f}\n  obj.putpixel(x, y, a * {k1} + b * {k2} + {off}, 0, 0, 255)\nend end"
            )
        } else {
            src
        };
        if let Err(e) = std::panic::catch_unwind(|| check(name, &src)) {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            failures.push(format!("{name}: {msg}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn kernel_inside_cpu_loop() {
    // 外側のループ(並列化できない: 前の反復の値を使う)は CPU に残り、毎回ディスパッチする。
    check(
        "cpu-outer-loop",
        r#"
local shift = 0
for pass = 1, 3 do
  shift = shift + pass * 10
  for y = 0, obj.h - 1 do
    for x = 0, obj.w - 1 do
      local r, g, b, a = obj.getpixel(x, y)
      obj.putpixel(x, y, (r + shift) % 256, g, b, a)
    end
  end
end"#,
    );
}
