# ビルド手順

## 初回セットアップ

1. 以下のソフトウェア・ライブラリを使用しているのでインストール
  - rust(cargo)
  - uv(python3.14)
  - node lts/npm(bunの使用を勧める。ビルド用スクリプトはbunで実行される)
  - vcpkg
  - clang(bindgenのビルドに必要) [詳細](https://rust-lang.github.io/rust-bindgen/requirements.html)
  - Linuxのみ：cpal用の追加パッケージ(libpulse-dev libpipewire-0.3-dev libasound2-dev)
2. `bun install` もしくは `npm install` を実行して、Node.jsの依存関係をインストール
  - `apps/*` と `packages/*` はworkspaceになっているので、ルートで1回実行すればよい
  - bunは `bunfig.toml` でhoisted linkerを指定している
3. `uv sync` を実行して、Pythonの依存関係をインストール
  - `tools/` 配下のスクリプトはこのuv環境で実行される(`uv run tools/<name>.py`)
4. `native/` に移動し、`vcpkg install` を実行してC++の依存関係をインストール
  - avloader(ffmpeg)とgpu_util(shader-slang, directx-dxc)の両方の依存関係が `native/vcpkg.json` にまとまっている。インストール先は `native/vcpkg_installed/`
  - 15分程度かかる
  - windowsの場合は利便性の観点からstatic tripletでのインストールをする
  --triplet=x64-windows-static-mdまたは--triplet=arm64-windows-static-mdをつけてインストールすること
5. `uv run tools/update_uv.py` を実行して、同梱するuvを最新バージョンに更新
  - これにより、uvのバイナリが `dist/resources/[os]-[arch]/bin/` にダウンロードされる(`-a` で全プラットフォーム分)
6. ルートディレクトリで、`bun run build` でビルド、`bun run dev` でデバッグモードでのビルド・起動が可能
  - ビルドされたファイルは `dist/app/` に出力される
  - ビルドは `tools/build.py` が次の順序で行う: UI(vite) → ネイティブ(napi) → main/preload(rolldown)
    - viteが `dist/app/` を空にするため、UIを先に実行する。main/preloadはnapiが生成する `dist/app/native/index.js` を取り込むため、napiの後に実行する
  - ネイティブのビルドでは、Rustのコードをnapiで `.node` として出力し、あわせて実行時に必要な共有ライブラリ(slang, dxcompiler, libpython)を `dist/app/` にコピーする
    - 読み込みに失敗するとnpmのバグが起きた旨のエラーが出ることがあるが、これはPythonやslang、DXCのdllが見つからないことが原因である

---

## その他のコマンド

- `bun run napi:debug` または `napi:build` で、ネイティブ(Rust)のみをビルドして `dist/app/` に出力することも可能。
  - typescriptの型(`dist/app/native/index.d.ts`)も生成されるため、単に型の更新をするときはこれを利用する
- `bun run python:copy` で、libpython(`python314.dll` 等)だけを `dist/app/` にコピーできる(`napi:*` でも自動で行われる)
  - `uv run tools/copy_python.py --python <実行ファイル>` で、uv環境以外のPythonを指定することもできる
- `bun run python:stubgen` で、RustのコードからPythonのstubファイルを `build/stub/aperio/` に生成することが可能
  - これにより、Rustコードの型定義をPython側で利用できるようになる
- `bun run plugin:stubgen` で、プラグインマネージャ(`python/aperio_plugin/`)のPythonコードからstubファイルを `build/stub/aperio_plugin/` に生成することが可能
  - これにより、Pythonコードの型定義をプラグイン側で利用できるようになる
  - stubは生成物でgit管理されない。`mypy` の `mypy_path` と `.vscode/settings.json` の `python.analysis.extraPaths` が `build/stub/` を参照する
- `bun run plugin:check` で、生成済みstubと実際のコードの不整合を検査する
- `bun run typecheck` で、`apps/` と `packages/` のTypeScriptを型チェックする

## ディレクトリ構成

```
apps/        Electronアプリ。main, preload, ui(メインUI), osr(オフスクリーンレンダリング), dialog, public(viteの静的ファイル)
packages/    JS/TSの共有パッケージ。shared(IPCのチャンネル名・Window型), ui(UI共通コンポーネント)
crates/      Rustのworkspace。aperio-napi(napi/pyo3の公開層), aperio-macros, aperio-gpu, aperio-media, aperio-text
native/      FFI対象のC++(cpp/)とvcpkgの設定
python/      プラグインマネージャ(aperio_plugin)のPythonコード
plugins/     組み込みプラグイン(PythonとSlangシェーダー)
tools/       ビルド・コード生成用のPythonスクリプト
docs/        ドキュメント
build/       (生成物) cargoのtarget/, Pythonのstub/, electron-builderのrelease/
dist/        (生成物) app/(アプリ本体), resources/(同梱するuv等)
```
