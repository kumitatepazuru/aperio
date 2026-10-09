module aperio_color;

// BT.601フルレンジでの輝度Y。
public float bt601_luma(float3 rgb) {
    return dot(rgb, float3(0.299, 0.587, 0.114));
}

// RGB(ストレートアルファのrgb)をBT.601の輝度Y・色差Cr/Cbへ変換する。
// 戻り値は float3(cr, cb, y)。
public float3 bt601_encode(float3 rgb) {
    let y = bt601_luma(rgb);
    let cr = (rgb.r - y) / 1.402000;
    let cb = (rgb.b - y) / 1.772000;
    return float3(cr, cb, y);
}

// bt601_encodeの逆変換。cr/cb/yからRGBを復元する。
public float3 bt601_decode(float cr, float cb, float y) {
    let r = y + 1.402000 * cr;
    let g = y - 0.344136 * cb - 0.714136 * cr;
    let b = y + 1.772000 * cb;
    return float3(r, g, b);
}

public float3 rgb_to_hsl(float3 rgb) {
    let cmax = max(max(rgb.r, rgb.g), rgb.b);
    let cmin = min(min(rgb.r, rgb.g), rgb.b);
    let delta = cmax - cmin;
    let l = (cmax + cmin) * 0.5;
    var s: float = 0.0;
    var h: float = 0.0;

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

    return float3(h, s, l);
}

public float3 hsl_to_rgb(float3 hsl) {
    let h = hsl.x;
    let s = hsl.y;
    let l = hsl.z;
    let c = (1.0 - abs(2.0 * l - 1.0)) * s;
    let hi = h / 60.0;
    let x = c * (1.0 - abs(hi - 2.0 * floor(hi / 2.0) - 1.0));
    let m = l - c * 0.5;
    var rgb: float3;

    if (h < 60.0) {
        rgb = float3(c, x, 0.0);
    } else if (h < 120.0) {
        rgb = float3(x, c, 0.0);
    } else if (h < 180.0) {
        rgb = float3(0.0, c, x);
    } else if (h < 240.0) {
        rgb = float3(0.0, x, c);
    } else if (h < 300.0) {
        rgb = float3(x, 0.0, c);
    } else {
        rgb = float3(c, 0.0, x);
    }

    return rgb + float3(m, m, m);
}
