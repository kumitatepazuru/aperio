#define_import_path aperio::color

// BT.601フルレンジでの輝度Y。
fn bt601_luma(rgb: vec3<f32>) -> f32 {
    return dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
}

// RGB(ストレートアルファのrgb)をBT.601の輝度Y・色差Cr/Cbへ変換する。
// 戻り値は vec3(cr, cb, y)。
fn bt601_encode(rgb: vec3<f32>) -> vec3<f32> {
    let y = bt601_luma(rgb);
    let cr = (rgb.r - y) / 1.402000;
    let cb = (rgb.b - y) / 1.772000;
    return vec3<f32>(cr, cb, y);
}

// bt601_encodeの逆変換。cr/cb/yからRGBを復元する。
fn bt601_decode(cr: f32, cb: f32, y: f32) -> vec3<f32> {
    let r = y + 1.402000 * cr;
    let g = y - 0.344136 * cb - 0.714136 * cr;
    let b = y + 1.772000 * cb;
    return vec3<f32>(r, g, b);
}

fn rgb_to_hsl(rgb: vec3<f32>) -> vec3<f32> {
    let cmax = max(max(rgb.r, rgb.g), rgb.b);
    let cmin = min(min(rgb.r, rgb.g), rgb.b);
    let delta = cmax - cmin;
    let l = (cmax + cmin) * 0.5;
    var s: f32 = 0.0;
    var h: f32 = 0.0;

    if (delta > 0.0001) {
        s = delta / (1.0 - abs(2.0 * l - 1.0));
        if (cmax == rgb.r) {
            let sector = (rgb.g - rgb.b) / delta;
            h = 60.0 * (sector - 6.0 * floor(sector / 6.0));
        } else if (cmax == rgb.g) {
            h = 60.0 * ((rgb.b - rgb.r) / delta + 2.0);
        } else {
            h = 60.0 * ((rgb.r - rgb.g) / delta + 4.0);
        }
        if (h < 0.0) { h += 360.0; }
    }

    return vec3<f32>(h, s, l);
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let h = hsl.x;
    let s = hsl.y;
    let l = hsl.z;
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let hi = h / 60.0;
    let x = c * (1.0 - abs(hi - 2.0 * floor(hi / 2.0) - 1.0));
    let m = l - c * 0.5;
    var rgb: vec3<f32>;

    if (h < 60.0) {
        rgb = vec3<f32>(c, x, 0.0);
    } else if (h < 120.0) {
        rgb = vec3<f32>(x, c, 0.0);
    } else if (h < 180.0) {
        rgb = vec3<f32>(0.0, c, x);
    } else if (h < 240.0) {
        rgb = vec3<f32>(0.0, x, c);
    } else if (h < 300.0) {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }

    return rgb + vec3<f32>(m);
}
