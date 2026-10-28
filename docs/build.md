# ビルド手順

## 初回セットアップ

1. 以下のソフトウェア・ライブラリを使用しているのでインストール
  - rust(cargo)
  - uv(python3.14)
  - node lts/bun(bunの使用を勧める。ビルドタスクを管理するmoonはbunの依存関係としてインストールされ、`bun moon` で実行する)
  - vcpkg
  - clang(bindgenのビルドに必要) [詳細](https://rust-lang.github.io/rust-bindgen/requirements.html)
  - Linuxのみ：cpal用の追加パッケージ(libpulse-dev libpipewire-0.3-dev libasound2-dev)
2. `bun install` もしくは `npm install` を実行して、Node.jsの依存関係をインストール
  - `apps/*` と `packages/*` はworkspaceになっているので、ルートで1回実行すればよい
  - bunは `bunfig.toml` でhoisted linkerを指定している
3. `uv sync` を実行して、Pythonの依存関係をインストール
  - `tools/` 配下のPythonスクリプトや、moonのタスクのうちPython系のものはこのuv環境で実行される
4. ルートディレクトリで、`bun moon run root:build` でビルド、`bun moon run root:dev` でデバッグモードでのビルド・起動が可能
  - ビルドされたファイルは `dist/app/` に出力される
  - タスクは [moon](https://moonrepo.dev/) で管理している。`<project>:<task>` の形式で実行し、`bun moon query tasks` で一覧を確認できる。設定は `.moon/` と各プロジェクトの `moon.yml`
  - ネイティブのビルドでは、Rustのコードをnapiで `.node` として出力し、あわせて実行時に必要な共有ライブラリ(slang, dxcompiler, libpython)を `dist/app/` にコピーする
    - 読み込みに失敗するとnpmのバグが起きた旨のエラーが出ることがあるが、これはPythonやslang、DXCのdllが見つからないことが原因である

---

## その他のコマンド

TypeScript/React のビルド・bundle 系(`ui:start`, `ui:build`, `main:build`, `electron:dev`, `lint`, `typecheck`)だけは `package.json` の scripts に残しており、`bun run <script>` で実行できる。
それ以外はすべて `bun moon run <project>:<task>` で実行する。

- `bun moon run napi:debug` または `napi:build` で、ネイティブ(Rust)のみをビルドして `dist/app/` に出力することも可能。
  - typescriptの型(`dist/app/native/index.d.ts`)も生成されるため、単に型の更新をするときはこれを利用する
- `bun moon run ui:build` / `main:build` で、UI(vite) / main・preload(rolldown) だけをビルドできる。`ui:start` はviteのdev server
- `bun moon run python:copy` で、libpython(`python314.dll` 等)だけを `dist/app/` にコピーできる(`napi:*` でも自動で行われる)
  - `bun moon run python:copy -- --python <実行ファイル>` で、uv環境以外のPythonを指定することもできる
- `bun moon run python:stubgen` で、RustのコードからPythonのstubファイルを `build/stub/aperio/` に生成することが可能
  - これにより、Rustコードの型定義をPython側で利用できるようになる
- `bun moon run plugin:stubgen` で、プラグインマネージャ(`python/aperio_plugin/`)のPythonコードからstubファイルを `build/stub/aperio_plugin/` に生成することが可能
  - これにより、Pythonコードの型定義をプラグイン側で利用できるようになる
  - stubは生成物でgit管理されない。`mypy` の `mypy_path` と `.vscode/settings.json` の `python.analysis.extraPaths` が `build/stub/` を参照する
- `bun moon run plugin:check` で、生成済みstubと実際のコードの不整合を検査する(必要なstubは自動で生成される)
- `bun moon run python:build` / `python:release` で、maturinによるPythonホイールのビルドができる
- `bun moon run root:dist` で、ビルドからelectron-builderによるパッケージングまで行う
- `bun run typecheck` で、`apps/` と `packages/` のTypeScriptを型チェックする

## ディレクトリ構成

```
apps/        Electronアプリ。main, preload, ui(メインUI), osr(オフスクリーンレンダリング), dialog, public(viteの静的ファイル)
packages/    JS/TSの共有パッケージ。shared(IPCのチャンネル名・Window型), ui(UI共通コンポーネント)
crates/      Rustのworkspace。aperio-napi(napi/pyo3の公開層), aperio-macros, aperio-gpu, aperio-media, aperio-text
native/      FFI対象のC++(cpp/)とvcpkgの設定
python/      プラグインマネージャ(aperio_plugin)のPythonコード
plugins/     組み込みプラグイン(PythonとSlangシェーダー)
tools/       moonのタスクから呼ばれるPythonスクリプト(update_uv.py, copy_python.py, vcpkg.py)
.moon/       moonのワークスペース設定。各タスクの定義は各プロジェクトの moon.yml (ルート, apps/ui, apps/main, crates/aperio-napi, python, python/aperio_plugin, native, tools)
docs/        ドキュメント
build/       (生成物) cargoのtarget/, Pythonのstub/, electron-builderのrelease/
dist/        (生成物) app/(アプリ本体), resources/(同梱するuv等)
```
