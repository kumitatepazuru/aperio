# TODO
1. 以下のソフトウェア・ライブラリを使用しているのでインストール
  - rust(cargo)
  - uv(python3.14)
  - node lts/npm(bunの使用を勧める)
  - vcpkg
  - clang(bindgenのビルドに必要) [詳細](https://rust-lang.github.io/rust-bindgen/requirements.html)
2. scripts/copy-python.sh(Windowsの場合はcopy-python.ps1をpowershellで実行)を実行して、Pythonの必要なファイルをコピー
  - これによりdist/に.so/.dylib/.dllがコピーされる
  - pythonコマンドがuvでインストールしたPythonを指すようにしておくこと
    - 何らかの原因でshellがuvのPythonを認識しない場合は、--uvオプションをつけることでuvのPythonを直接呼び出すことができる
3. npm install もしくは bun install を実行して、Node.jsの依存関係をインストール
4. uv syncを実行して、Pythonの依存関係をインストール
  - numpyの他にaperioプラグインを開発するためのstubファイルもインストールされる
5. src/avloader-cpp/に移動し、vcpkg installを実行しC++の依存関係をインストール
  - 15分程度かかる
  - project rootからも実行できるようにmesonを使ったビルドスクリプトの作成を検討中
  - windowsの場合は利便性の観点からstatic tripletでのインストールが強く推奨される。--triplet=x64-windows-staticまたは--triplet=arm64-windows-staticをつけてインストールすること
6. uv run python scripts/update-uv.pyを実行して、uvを最新バージョンに更新
  - これにより、uvのバイナリがresources/[os]-[arch]/bin/にダウンロードされる
6. ルートディレクトリで、bun run buildでビルド、bun run devでデバッグモードでのビルドが可能
  - ビルドされたファイルはdist/に出力される
  - ビルドスクリプトは、Rustのコードをビルドしてnapiを使用しdist/に.nodeとして出力する
  - ビルドスクリプトは、Pythonの必要なdllファイルがdist/に存在することを前提としているため、先にscripts/copy-python.shを実行しておく必要がある。
    - 読み込みに失敗するとnpmのバグが起きた旨のエラーが出ることがあるが、これはPythonのdllが見つからないことが原因である

---

- bun run napi:debugまたはnapi:buildで、Rustコードのみをビルドしてnapiを使用しdist/に.nodeとして出力することも可能。
  - typescriptの型生成も行われるため、単に型の更新をするときはこれを利用する
- bun run wrapper:stubgenで、RustのコードからPythonのstubファイルを生成することが可能
  - これにより、Rustコードの型定義をPython側で利用できるようになる
- bun run python:stubgenで、プラグインマネージャのPythonコードからPythonのstubファイルを生成することが可能
  - これにより、Pythonコードの型定義をプラグイン側で利用できるようになる

---

- windows arm64のビルドを行うとlibdav1dの動作がおかしくなる
  - ファイルフォーマット情報などは読み込めるが、デコードがエラーなしで失敗する
  - 検索してみたが原因不明なので要調査