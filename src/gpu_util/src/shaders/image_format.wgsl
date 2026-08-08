// ImageGenerator::new() で選択されたフォーマットに応じて切り替わる型エイリアス。
// shader_defs (APERIO_FORMAT_RGBA8UNORM / APERIO_FORMAT_RGBA16FLOAT) は
// `CompiledWgsl::new`/`compose_new`が呼び出しごとの`output_format`から自動的に
// 導出・注入するため、シェーダー側は`ImageStorageTexture`という型名をそのまま
// 使うだけでよい(`#import`は不要)。
//
// 精度に敏感なシェーダー(HDR的な蓄積、ガモット外・unbounded値の保持など)は、
// このエイリアスを使わず `texture_storage_2d<rgba32float, write>` を
// 直接ハードコードし続けること。

#ifdef APERIO_FORMAT_RGBA8UNORM
alias ImageStorageTexture = texture_storage_2d<rgba8unorm, write>;
#else ifdef APERIO_FORMAT_RGBA16FLOAT
alias ImageStorageTexture = texture_storage_2d<rgba16float, write>;
#else
alias ImageStorageTexture = texture_storage_2d<rgba32float, write>;
#endif
