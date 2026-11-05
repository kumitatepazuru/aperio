// lua_gpu: Lua (LuaJIT) の意味論に合わせるための補助関数。
// 生成コードの先頭に、使われるかどうかに関わらずまとめて置く(未使用の関数は Slang が除去する)。

struct LgPair { float r0; float r1; };

// params(32 bit ワード列)の読み出し。
float lg_f(uint i) { return asfloat(res.params[i].v); }
uint lg_u(uint i) { return res.params[i].v; }
bool lg_b(uint i) { return res.params[i].v != 0u; }

// Lua の添字(数値)を配列の位置に。整数でなければ -1(Lua では別のキーなので nil になる)。
int lg_idx(float i) { return (floor(i) == i && abs(i) < 2147483647.0) ? int(i) : -1; }

// a % b = a - floor(a / b) * b(LuaJIT の lj_vm_foldarith と同じ)。
float lg_mod(float a, float b) { return a - floor(a / b) * b; }

// C の pow と同じ扱い(負の底と整数の指数、0 の負べき)。Slang の pow は負の底で NaN になる。
float lg_pow(float a, float b) {
    if (b == 0.0) return 1.0;
    if (isnan(a) || isnan(b)) return a + b;
    if (a == 0.0) return (b > 0.0) ? 0.0 : (1.0 / 0.0);
    if (a < 0.0) {
        if (floor(b) != b) return asfloat(0x7fc00000u);
        float r = pow(-a, b);
        return (fmod(abs(b), 2.0) == 1.0) ? -r : r;
    }
    return pow(a, b);
}

// math.max / math.min は引数を左から maxsd / minsd で畳み込む(LuaJIT の vm_x64.dasc)。
// maxsd(acc, x) は acc > x なら acc、そうでなければ(NaN を含む)x。
float lg_max(float acc, float x) { return (acc > x) ? acc : x; }
float lg_min(float acc, float x) { return (acc < x) ? acc : x; }

float lg_log(float x) { return log(x); }
float lg_log2arg(float x, float base) {
    if (base == 2.0) return log2(x);
    if (base == 10.0) return log10(x);
    return log(x) / log(base);
}

LgPair lg_modf(float x) {
    LgPair p;
    float ip = (x >= 0.0) ? floor(x) : ceil(x);
    p.r0 = ip;
    p.r1 = isinf(x) ? 0.0 : (x - ip);
    return p;
}

LgPair lg_frexp(float x) {
    LgPair p;
    if (x == 0.0 || isnan(x) || isinf(x)) { p.r0 = x; p.r1 = 0.0; return p; }
    float e = floor(log2(abs(x))) + 1.0;
    float m = x / exp2(e);
    // 誤差で [0.5, 1) を外れたら補正する。
    if (abs(m) >= 1.0) { m *= 0.5; e += 1.0; }
    if (abs(m) < 0.5) { m *= 2.0; e -= 1.0; }
    p.r0 = m;
    p.r1 = e;
    return p;
}

float lg_difftime(float t2, float t1) { return t2 - t1; }

// bit ライブラリ: 数値を 32 bit 整数に正規化(最近接偶数への丸め + 2^32 を法とする)。
int lg_tobiti(float x) {
    float f = floor(x);
    float d = x - f;
    if (d > 0.5 || (d == 0.5 && fmod(f, 2.0) != 0.0)) f += 1.0;
    float m = f - 4294967296.0 * floor(f / 4294967296.0);
    return asint(uint(m));
}
float lg_tobit(float x) { return float(lg_tobiti(x)); }
float lg_bnot(float x) { return float(~lg_tobiti(x)); }
float lg_band(float a, float b) { return float(lg_tobiti(a) & lg_tobiti(b)); }
float lg_bor(float a, float b) { return float(lg_tobiti(a) | lg_tobiti(b)); }
float lg_bxor(float a, float b) { return float(lg_tobiti(a) ^ lg_tobiti(b)); }
float lg_lshift(float a, float n) { return float(asint(asuint(lg_tobiti(a)) << (asuint(lg_tobiti(n)) & 31u))); }
float lg_rshift(float a, float n) { return float(asint(asuint(lg_tobiti(a)) >> (asuint(lg_tobiti(n)) & 31u))); }
float lg_arshift(float a, float n) { return float(lg_tobiti(a) >> (asuint(lg_tobiti(n)) & 31u)); }
float lg_rol(float a, float n) {
    uint x = asuint(lg_tobiti(a));
    uint s = asuint(lg_tobiti(n)) & 31u;
    return float(asint((x << s) | (x >> ((32u - s) & 31u))));
}
float lg_ror(float a, float n) {
    uint x = asuint(lg_tobiti(a));
    uint s = asuint(lg_tobiti(n)) & 31u;
    return float(asint((x >> s) | (x << ((32u - s) & 31u))));
}
float lg_bswap(float a) {
    uint x = asuint(lg_tobiti(a));
    return float(asint((x >> 24) | ((x >> 8) & 0xff00u) | ((x << 8) & 0xff0000u) | (x << 24)));
}
