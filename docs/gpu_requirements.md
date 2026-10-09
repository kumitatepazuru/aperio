# GPU・OS の動作要件

aperio の GPU 処理は、自前の RHI (`src/gpu_util/src/rhi/`) を介して **Vulkan だけ** で動く。
シェーダーは Slang (または HLSL) から SPIR-V にコンパイルし、wgpu / naga / WGSL は使わない。
このドキュメントは「コードが実際に要求していること」をまとめたもので、動作確認済みの環境は末尾に分けて書く。

## 必須要件

デバイス作成時に次のいずれかを満たさない物理デバイスは候補から外れる
(`rhi/vulkan/device.rs` の `supports_required_features`)。
満たすデバイスが一つも無いと `Device::new()` / `ImageGenerator::new()` がエラーになる。

| 要件                                                  | 理由                                                                                                      |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| **Vulkan 1.2**                             | descriptor indexing をコア機能として使う                                                                  |
| `shaderSampledImageArrayNonUniformIndexing` (Vulkan 1.2)           | 入力テクスチャ配列を `NonUniformResourceIndex` で引く (`common/select.slang` など)                        |
| `descriptorBindingVariableDescriptorCount` (Vulkan 1.2)           | レイヤー数がフレームごとに変わる可変長テクスチャ配列 (`compose.slang`、`input_texture_layout="variable"`) |
| `runtimeDescriptorArray` (Vulkan 1.2)                              | サイズ未指定の `Texture2D tex[]`                                                                          |
| `shaderDrawParameters` (Vulkan 1.1)              | Slang / DXC が `SV_VertexID` を `DrawParameters` capability 付きで出力する                                |
| `VK_KHR_dynamic_rendering` と `dynamicRendering` (Vulkan 1.3 or 1.2 Extension) | render pass / framebuffer を作らずにラスター描画する                                                      |

可変長テクスチャ配列の最大本数はデバイス依存で、
`min(maxPerStageDescriptorSampledImages, maxDescriptorSetSampledImages, maxPerStageResources - 3)` になる
(`Device::max_variable_texture_array_len`)。レイヤー数がこれを超えるとディスパッチ時にエラーになる。

## 任意の機能

無くても起動はできるが、対応する機能だけが使えなくなる。使おうとした時点で理由付きのエラーになる。

| 機能                                                             | 必要なもの                                                                                | 使えないと困ること                                              |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| 16bit 正規化テクスチャ (`R16Unorm` / `Rg16Unorm`) のサンプリング | フォーマット機能 `SAMPLED_IMAGE`                                                          | 10bit / 16bit 動画 (P010 など) の GPU 変換。8bit 動画は影響なし |
| 共有テクスチャへの書き出し (Windows)                             | `VK_KHR_external_memory` + `VK_KHR_external_memory_win32`                                 | `generate_shared_texture`                                       |
| 共有テクスチャへの書き出し (Linux)                               | `VK_KHR_external_memory` + `VK_KHR_external_memory_fd` + `VK_EXT_external_memory_dma_buf` | 同上                                                            |
| 非 LINEAR な DRM modifier の dmabuf (Linux)                      | `VK_EXT_image_drm_format_modifier`                                                        | LINEAR 以外の dmabuf の取り込み                                 |

内部の作業フォーマット (`Rgba8Unorm` / `Rgba16Float` / `Rgba32Float`) と、共有テクスチャの書き込み先
(`Rgba16Float` / `Bgra8Unorm`) は、ストレージ画像・サンプリング・カラーアタッチメントとして使える必要がある。
一般的なデスクトップ GPU ではどれも満たされる。

## OS

| OS                    | 状態         | 備考                                                                                                                                                 |
| --------------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| Windows 10 / 11 (x64) | 動作確認済み | Vulkan 1.2 対応のドライバ (`vulkan-1.dll`) が必要。共有テクスチャは D3D12 のリソースを取り込む                                                       |
| Linux (x64)           | **未検証**   | Vulkan 1.2 対応の Mesa / NVIDIA ドライバが必要。dmabuf 取り込み (`import_dmabuf_texture`) は書いてあるが、この環境ではコンパイルも実行もできていない |
| macOS                 | 未対応       | Vulkan バックエンドのみ。MoltenVK での動作は確認していない                                                                                           |

ソフトウェア実装 (WARP 系・llvmpipe など) は想定していない。動いたとしても、外部メモリ拡張が無いため共有テクスチャは使えない。

## 共有テクスチャ (`generate_shared_texture`)

### Windows (D3D12 → Vulkan)

D3D12 側がリソースを作り、Vulkan がそれを取り込んで直接書き込む
(以前の「D3D12 側で OpenSharedHandle して書く」方式の逆向き)。呼び出し側は次の条件を守る。

- Vulkan と **同じアダプタ** (LUID が一致) の D3D12 デバイスで作る。`rhi::Device::adapter_luid()` で取得できる。
- `D3D12_HEAP_FLAG_SHARED` 付きの committed resource とし、`CreateSharedHandle` で得た NT ハンドルを渡す。
- `D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET` を付ける。
- フォーマットは `DXGI_FORMAT_R16G16B16A16_FLOAT` か `DXGI_FORMAT_B8G8R8A8_UNORM`、サイズは出力と同じにする。
- ハンドルの所有権は呼び出し側に残る (閉じるのは呼び出し側)。

書き込みは同期 submit で完了してから戻るので、関数が戻った後は D3D12 側でそのまま読める
(D3D12 側のリソースは `COMMON` 状態のままで、読む前に自分で遷移させる)。
動作は `tests/native_texture_import_d3d12_bgra8.rs` / `native_texture_import_d3d12_f16.rs` で確認している。

### Linux (dmabuf)

単一プレーンの dmabuf のみ対応。fd は複製してから取り込むので、呼び出し側の fd は開いたまま残る。
modifier が LINEAR / INVALID なら LINEAR タイリング、それ以外なら DRM format modifier を使う。

## 実行時に必要なファイル

ビルド時に `build.rs` が次を実行ファイル (cdylib) と同じディレクトリへコピーする。配布物にも同梱すること。

- Slang: `slang.dll`, `slang-compiler.dll`, `slang-glsl-module.dll`, `slang-glslang.dll`, `slang-llvm.dll`, `slang-rt.dll`
- DXC (HLSL を使う場合): `dxcompiler.dll`

## シェーダーの書き方との関係

- Slang の束縛規約: 入力は `[vk::binding(0, 0)] ParameterBlock<...> inputs;` (set 0)、
  出力・サンプラー・params は `[vk::binding(0, 1)] ParameterBlock<...> res;` (set 1)。
  `tests/plugin_shaders_compile.rs` が全プラグインのシェーダーでこの固定を検査する。
- HLSL (`PyCompiledShader.from_hlsl`) も同じ規約を `[[vk::binding(binding, set)]]` で明示する。
  Slang と違い、エントリポイント名は SPIR-V にそのまま残る。
- ラスター描画では NDC の y+ を画面上方向に揃えるため、ビューポートを上下反転している。
  D3D / wgpu 向けに書かれたシェーダーがそのまま同じ向きで描ける。

## 既知の制約・未対応

- fp16 (`SHADER_F16` 相当) は有効にしていない。以前の wgpu 版で AMD / Qualcomm が「使えるように見えて未サポート」と
  報告する問題があったため、Vulkan 移行後も使えるとは決めつけていない。必要になった時点で
  `VK_KHR_shader_float16_int8` のサポートをベンダごとに確認すること。
- すべての submit は同期 (完了を待つ)。ディスクリプタプールもディスパッチごとに作って捨てている。
  負荷の高いパイプラインではここが次の最適化対象になる。

## 動作確認済みの環境

| 項目         | 内容                                                                                                                                                                              |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OS           | Windows 11 Pro                                                                                                                                                                    |
| GPU          | NVIDIA GeForce RTX 3070 Ti (Vulkan 1.4.341 ドライバ)                                                                                                                              |
| 確認した範囲 | `cargo test --workspace` の全テスト、`tests/native_texture_import_d3d12_*.rs` (実際の D3D12 共有テクスチャ)、Python からの `PyCompiledShader.from_slang` / `PyCompiledShader.from_hlsl` / `generate_buf` |

GPU が無い環境では、各テストは `Skipping ...` を表示して成功扱いで抜ける。
自分の環境が要件を満たすかは、`vulkaninfo` で上の機能を確認するか、`cargo test -p gpu_util --test rhi_device_init -- --nocapture` が
`Skipping` を出さずに通るかで確認できる。
