# Aperio 用語定義

このドキュメントはコードベース全体で使われる概念・用語の定義と用法を定めたものです。
新しい概念を追加する際は必ずここに記載し、一貫した命名を維持してください。

---

## Item（アイテム）

**定義:** タイムライン上に配置された編集要素の単位。映像・画像・テキストなどの Video アイテムと、音声の Audio アイテムの2種類がある。

**型:** `ItemStructure`（Video / Audio の union enum）

**共通フィールド:**

| フィールド | 型 | 意味 |
|---|---|---|
| `id` | `string` | UUID。アイテム固有の識別子 |
| `type` | `"Video" \| "Audio"` | アイテムの種別 |
| `layer` | `number` | レイヤー番号（z-index）。0が最下層 |
| `start` | `number` | 開始フレーム |
| `end` | `number` | 終了フレーム |
| `min` | `number` (optional) | 最小デュレーション（フレーム数）。プラグインの `minFrame` から自動設定 |
| `max` | `number` (optional) | 最大デュレーション（フレーム数）。プラグインの `maxFrame` から自動設定 |
| `object` | `GenerateStructure` | このアイテムのオブジェクト本体 |
| `effects` | `GenerateStructure[]` | 適用されているエフェクトのリスト |

**Video アイテム固有フィールド:**

| フィールド | 型 | 意味 |
|---|---|---|
| `x`, `y` | `number` | キャンバス上の位置（px） |
| `scale` | `number` | 拡大率（%、100が等倍） |
| `rotation` | `number` | 回転角（度） |
| `alpha` | `number` | 不透明度（%、100が不透明） |

**Audio アイテム固有フィールド:**

| フィールド | 型 | 意味 |
|---|---|---|
| `volume` | `number` | 音量（0.0〜1.0） |
| `pan` | `number` | パン（-1.0=左、0.0=中央、1.0=右） |

**Store上の名称:**
- `timelineItems: ItemStructure[]` — 全アイテムのリスト（VideoとAudio混在）
- `selectedItemIds: string[]` — 選択中アイテムのIDリスト（複数選択対応）
- `mainSelectedItemId: string | null` — パラメータ編集の基準となる「メイン選択」アイテムのID

**Store ユーティリティ関数:**
- `getCurrentVideoItems()` — 現在フレームに表示中の Video アイテム一覧
- `getCurrentAudioItems(duration)` — 現在フレームから `duration` フレーム内に含まれる Audio アイテム一覧

**注意:**
- ループ変数は `item` を使う（`layer` は使わない）。
- `min` / `max` は `GeneratorInformation.minFrame` / `maxFrame` の値をプラグイン呼び出し時に自動設定する。

### ItemStructure（ネイティブブリッジ型）

**定義:** napi-rs が生成する native の構造体。Rust → Python・TypeScript 間のアイテムデータ転送フォーマット。Video / Audio の2バリアントを持つ enum として定義される。

**型定義:**
- Rust: `src/native/src/structs/item_structures.rs` → `pub enum ItemStructure { Video { ... }, Audio { ... } }`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）
- Python: `src/native/src/structs/item_structures.rs`（pyo3 で定義、スタブは `src-python/out/aperio/item_structures.pyi`）→ `class ItemStructure.Video` / `class ItemStructure.Audio`

---

## Layer（レイヤー）

**定義:** タイムライン上のアイテムが上下（z-index）方向に占める位置番号。アイテム自体ではなく「位置」を指す。

**型:** `number`（0 が最下層）

**用法:**
- アイテムの `.layer` プロパティとして参照する（例: `item.layer`）
- レイヤー番号は0以上の整数。同じレイヤーには時間的に重なるアイテムを置けない。

**注意:**
- "Layer" という語をアイテム自体の名称に使わない。アイテムは `Item`、z-index位置は `layer`。

---

## Object（オブジェクト）

**定義:** アイテムの描画コンテンツ本体。Pythonプラグインで定義された種別（映像・画像・テキストなど）に対応する。

**型:** `GenerateStructure`

| フィールド | 型 | 意味 |
|---|---|---|
| `id` | `string` | UUID。インスタンス固有の識別子 |
| `name` | `string` | プラグイン種別名。同種なら同じ値（例: `"image"`, `"text"`） |
| `displayName` | `string` | UIに表示する名前 |
| `parameters` | `Record<string, JsonValue>` | プラグインへ渡すパラメータ値の辞書 |

**アクセス方法:** `item.object`（`ItemStructure` のフィールド）

**注意:**
- Rust構造体での定義: `pub object: GenerateStructure`（`src/native/src/structs/item_structures.rs`）
- Pythonプラグインへ渡る際もフィールド名は `object` になる。

---

## Effect（エフェクト）

**定義:** アイテムに後から適用する映像効果。複数を順序付きで重ねることができる。

**型:** `GenerateStructure`（Object と同じ型）

**アクセス方法:** `item.effects`（`GenerateStructure[]`）

**用法:**
- 配列なので複数形 `effects` を使う。
- 個々の要素を指す変数名は `effect`（単数形）。

**注意:**
- Object と同じ `GenerateStructure` 型を共有している。意味的には別概念（コンテンツ本体 vs 後処理）。
- エフェクトの順序はUI上でドラッグまたはボタンで変更できる。

---

## Params（パラメータ値）

**定義:** オブジェクト・エフェクトのプラグインに渡す設定値の辞書。`Record<string, ConfigableValue>` 型。

**変数名の規則:**
- ローカル変数・state名は `params` / `setParams` を使う。
- 関数引数も `params`、新しい値なら `newParams`。
- 初期化済みを含む場合は `newParamsWithInit` など接尾語で区別。

**型の区別:**

| 名称 | 型 | 意味 |
|---|---|---|
| `params` | `Record<string, ConfigableValue>` | UI上で編集中のパラメータ値の辞書 |
| `parameters` | `Record<string, JsonValue>` | `GenerateStructure` のフィールド名（native型の一部）。直接操作しない |
| `structures` | `RequestStructureParameter[]` | パラメータの定義（型・タイトル・デフォルト値など） |

**注意:**
- `parameters` はnativeの構造体フィールド名なので変更しない。UIコード内のローカル変数として `parameters` を使うのは避ける。
- `values` という命名はパラメータ値の変数名として使わない（`params` に統一）。

---

## GenerateStructure

**定義:** Object・Effect 両方が共有する実行時のプラグインインスタンス構造。napi-rs により native から生成される。

**型定義:**
- Rust: `src/native/src/structs/item_structures.rs` → `pub struct GenerateStructure`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface GenerateStructure`
- Python: `src/native/src/structs/item_structures.rs`（pyo3 で定義、スタブは `src-python/out/aperio/item_structures.pyi`）→ `class GenerateStructure(dict)`

**用法:**
- `item.object` — オブジェクト本体としての GenerateStructure
- `item.effects[i]` — エフェクトとしての GenerateStructure

---

## ItemResult

**定義:** フレームレンダリング後、各アイテムの出力サイズ情報を格納する型。

**型定義:**
- Rust: `src/native/src/structs/item_structures.rs` → `pub struct ItemResult`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface ItemResult`
- Python: `src/native/src/structs/item_structures.rs`（pyo3 で定義、スタブは `src-python/out/aperio/item_structures.pyi`）→ `class ItemResult`

**フィールド:** `width: number`, `height: number`

**用法:**
- `frameResults: Record<string, ItemResult>` — ストアのトップレベルフィールド。アイテムIDをキーとした描画結果辞書
- `Overlay.tsx` でアイテムの表示サイズ計算に使用

---

## Frame（フレーム）

**定義:** 映像の1コマ。時間軸の単位。

**用法:**
- `start`, `end` — アイテムの開始・終了フレーム番号（整数）
- `viewerState.beginFrame` — 現在（一時停止時）または再生開始時のフレーム番号
- `frameState` — キャンバスの解像度とFPS（`FrameState: { width, height, fps }`）
- `frameResults` — ストアのトップレベルフィールド。各アイテムのレンダリング結果（`Record<string, ItemResult>`）

**注意:**
- `Frame` クラス（`src/renderer/bridge.ts`）はフレームバッファの送受信を担うネットワーク層のクラス。映像フレームの概念とは別物。

---

## Plugin（プラグイン）

**定義:** Pythonで実装されたオブジェクト・エフェクトの処理単位。映像系（Video）と音声系（Audio）の2系統がある。

**種別:**

| 種別 | 基底クラス | 意味 |
|---|---|---|
| Video Object Plugin | `VideoObjectGeneratorBase` | アイテムの映像コンテンツを生成するプラグイン |
| Video Effect Plugin | `VideoEffectGeneratorBase` | アイテムに映像効果を適用するプラグイン |
| Audio Object Plugin | `AudioObjectGeneratorBase` | オーディオサンプルを生成するプラグイン |
| Audio Effect Plugin | `AudioEffectGeneratorBase` | オーディオサンプルにエフェクトを適用するプラグイン |

**参照:** `PluginNameInfo.basePlugin`, `PluginNameInfo.videoObjectPlugins`, `PluginNameInfo.videoEffectPlugins`, `PluginNameInfo.audioObjectPlugins`, `PluginNameInfo.audioEffectPlugins`

**イベントシステム:**
- プラグインのイベントハンドラーは `@event(type=GeneratorEvent.New)` 等のデコレーターで登録する。
- `EventManager.call_event(plugin_name, type, params)` で統一的に呼び出す。
- `params` 引数は常に `dict`。Effect の `New` イベントでも空の dict を渡す。

---

## VideoGeneratorBase / AudioGeneratorBase

**定義:** プラグインがフレーム・サンプルを生成するための基底クラス群。映像系と音声系で分かれている。

### 映像系

| クラス | 用途 |
|---|---|
| `VideoGeneratorBase` | 映像フレームを生成する基底クラス |
| `VideoObjectGeneratorBase` | 映像オブジェクトプラグインの基底クラス |
| `VideoEffectGeneratorBase` | 映像エフェクトプラグインの基底クラス |

**`generate` メソッドの引数型:** `VideoGenerateParameters`

| フィールド | 型 | 意味 |
|---|---|---|
| `frame_number` | `int` | 生成するフレーム番号 |
| `layer` | `ItemStructure.Video` | 対象アイテムの情報 |
| `args` | `dict` | パラメータ値の辞書 |
| `width` | `int` | キャンバス幅（px） |
| `height` | `int` | キャンバス高さ（px） |

**戻り値:** `GeneratorWgslReturn | GeneratorFuncReturn | GeneratorTextureReturn | None`

### 音声系

| クラス | 用途 |
|---|---|
| `AudioGeneratorBase` | オーディオサンプルを生成する基底クラス |
| `AudioObjectGeneratorBase` | オーディオオブジェクトプラグインの基底クラス |
| `AudioEffectGeneratorBase` | オーディオエフェクトプラグインの基底クラス |

**`generate` メソッドの引数型:** `AudioGenerateParameters`

| フィールド | 型 | 意味 |
|---|---|---|
| `start_time` | `float` | 生成開始時刻（秒） |
| `layer` | `ItemStructure.Audio` | 対象アイテムの情報 |
| `sample_rate` | `int` | サンプルレート（Hz） |
| `channels` | `int` | チャンネル数 |
| `sample_count` | `int` | 生成するサンプル数 |
| `args` | `dict` | パラメータ値の辞書 |
| `input_samples` | `npt.NDArray[np.float32] \| None` | エフェクト用の入力サンプル（Objectは `None`） |

**戻り値:** `npt.NDArray[np.float32] | None`（インターリーブ形式の f32 配列）

---

## AudioManager

**定義:** cpal を使ってオーディオを再生するネイティブマネージャー。プラグインが生成したサンプルをバッファに積み上げ（`stack_audio`）、CPALストリームで順次再生する。

**型定義:**
- Rust: `src/native/src/managers/audio_manager.rs` → `pub struct AudioManager`
- Python スタブ: `src-python/out/aperio/audio.pyi` → `class AudioManager`

**API:**

| メソッド/プロパティ | 型 | 意味 |
|---|---|---|
| `sample_rate` | `int` (get/set) | サンプルレート（Hz）。変更するとストリームを再構築 |
| `channels` | `int` (get/set) | チャンネル数。変更するとストリームを再構築 |
| `bit_depth` | `int` (get/set) | ビット深度 |
| `current_time` | `float` (get) | 現在の再生位置（秒） |
| `pending_samples` | `int` (get) | 再生位置より先にバッファされているサンプル数 |
| `stack_audio(data, start_time)` | — | サンプルデータを `start_time`（絶対サンプルインデックス）から積む |
| `stop()` | — | 再生を停止してバッファをクリア |

**動作:**
- `stack_audio` 呼び出し時に停止中なら自動的に再生開始する。
- 再生済みサンプルは自動的にバッファから除去される。
- バッファが空になると自動停止（アンダーラン時）。
- インスタンス破棄時に CPAL ストリームが自動停止する。

**Python側グローバル参照:** `aperio_plugin.audio_manager`

---

## AudioState

**定義:** ストアに保存される音声設定。プロジェクト全体の音声出力パラメータを定める。

**型定義:**
- Rust: `src/native/src/managers/store_manager.rs` → `pub struct AudioState`
- TypeScript: `dist/native/index.d.ts` → `interface AudioState`

**フィールド:**

| フィールド | 型 | デフォルト | 意味 |
|---|---|---|---|
| `channels` | `number` | `2` | チャンネル数 |
| `sampleRate` | `number` | `44100` | サンプルレート（Hz） |
| `bitDepth` | `number` | `16` | ビット深度 |

**Store上の名称:** `audioState: AudioState`

---

## Managers（src/native/src/managers/）

**定義:** ネイティブ層のステート管理を集約したモジュール群。

**モジュール構成:**

| モジュール | 役割 |
|---|---|
| `managers/store_manager.rs` | ストア状態（`SyncableState`）の読み書き |
| `managers/config_manager.rs` | アプリケーション設定の読み書き |
| `managers/audio_manager.rs` | 音声バッファ・CPAL ストリーム管理 |
| `managers/wrappers/` | Python向けラッパー（`PyManagers` など） |

**`PyManagers`:** Python側へ渡すマネージャー群のコンテナ。`AperioManager.__init__` の引数 `managers: PyManagers` として受け取る。`aperio_plugin.manager`（`AperioManager` インスタンス）を通じて各マネージャーにアクセスできる。

---

## PyAudioLoader

**定義:** 音声ファイルのデコード・サンプル取得を担う Python クラス。内部的に C++ の avloader を利用する。

**型定義:**
- Rust: `src/native/src/python/modules/avloader.rs`
- Python スタブ: `src-python/out/aperio/avloader.pyi` → `class PyAudioLoader`

**API:**

| メソッド/プロパティ | 型 | 意味 |
|---|---|---|
| `PyAudioLoader(path)` | — | 音声ファイルを開く |
| `sampling_rate` | `int` | サンプルレート（Hz） |
| `chs` | `int` | チャンネル数 |
| `bit_depth` | `int` | ビット深度 |
| `duration` | `float` | 全体の長さ（秒） |
| `get_audio(time, duration)` | `ndarray` | 指定区間のサンプルを取得 |

---

## Window IPC API（音声関連）

**定義:** レンダラプロセスから音声再生を制御するための IPC ブリッジ。`window.audio` 経由でアクセスする。

**型定義:** `src/renderer/types.d.ts`

| メソッド | 意味 |
|---|---|
| `window.audio.play(audioStructure, sampleRate, channels, startTime, duration)` | Audio アイテムを生成・再生する |
| `window.audio.stop()` | 再生を停止する |
| `window.audio.getPendingSamples()` | バッファ済みサンプル数を取得する |

**対応する Python メソッド:** `AperioManager.play_audio(audio_structure, sample_rate, channels, start_time, duration)`

---

## GeneratorEvent

**定義:** プラグインのイベント種別を識別するenum。Rustで定義しPyO3・napi経由でPython・TypeScriptに公開する。両言語間の手動同期は不要。

**型定義:**
- Rust: `src/native/src/structs/item_structures.rs` → `pub enum GeneratorEvent`
- Python: `aperio.item_structures.GeneratorEvent`（PyO3経由、スタブは `src-python/out/aperio/item_structures.pyi`）

| バリアント | 意味 |
|---|---|
| `GeneratorEvent.New` | プラグインが新規追加されたときのイベント |
| `GeneratorEvent.RequestStructure` | パラメータ構造の再取得イベント |

**用法:**
- `@event(type=GeneratorEvent.New)` — プラグインメソッドにデコレーターで指定
- `call_event(plugin_name, GeneratorEvent.New, params)` — EventManagerから呼び出す
- Rust側: `GeneratorEvent::New` として `call_method1` の引数に渡す

---

## GeneratorInformation

**定義:** プラグインイベント（`GeneratorEvent.New` および `GeneratorEvent.RequestStructure`）の共通戻り値型。プラグインの表示名・初期デュレーション・フレーム範囲・パラメータ構造をまとめて返す。

**型定義:**
- Rust: `src/native/src/structs/item_structures.rs` → `pub struct GeneratorInformation`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface GeneratorInformation`
- Python: `src/native/src/structs/item_structures.rs`（pyo3 で定義、スタブは `src-python/out/aperio/item_structures.pyi`）→ `class GeneratorInformation`

| TypeScript フィールド | Python フィールド | TypeScript 型 | Python 型 | 意味 |
|---|---|---|---|---|
| `displayName` | `display_name` | `string` | `str` | UI表示名 |
| `durationFrames` | `duration_frames` | `number \| undefined` | `int \| None` | 初期デュレーション（フレーム数）。Effectは `undefined` / `None` |
| `maxFrame` | `max_frame` | `number \| undefined` | `int \| None` | 最大フレーム数の上限。制限なしは `undefined` / `None` |
| `minFrame` | `min_frame` | `number \| undefined` | `int \| None` | 最小フレーム数の下限。制限なしは `undefined` / `None` |
| `structure` | `structure` | `RequestStructureParameter[]` | `list[RequestStructureParameter]` | パラメータ構造定義 |

**用法:**
- `@event(type=GeneratorEvent.New)` デコレーターを付けたハンドラーの戻り値として返す
- `@event(type=GeneratorEvent.RequestStructure)` デコレーターを付けたハンドラーの戻り値としても同型を返す
- `call_event(plugin_name, GeneratorEvent.New, params)` / `call_event(plugin_name, GeneratorEvent.RequestStructure, params)` の戻り値型として型付けされている

---

## ID・文字列識別子の命名規則

このプロジェクトには「UUID」と「プラグイン種別名」という2種類の文字列 ID が混在する。混同しないこと。

### UUID（インスタンス識別子）

**定義:** `uuid` パッケージの `v4()` で生成したランダム文字列。インスタンスを一意に識別する。

| フィールド | 型 | 意味 |
|---|---|---|
| `item.id` | `string` (UUID) | タイムラインアイテムのインスタンス識別子 |
| `item.object.id` | `string` (UUID) | オブジェクトプラグインインスタンスの識別子 |
| `effect.id` | `string` (UUID) | エフェクトプラグインインスタンスの識別子 |

**注意:**
- `item.id` と `GenerateStructure.id` はそれぞれ独立した UUID。同一アイテムの `item.id` と `item.object.id` は**異なる値**になる。
- アイテムやエフェクトを配列から検索・更新するときは常にこの UUID で照合する（例: `item.id === selectedItemId`、`e.id === effect.id`）。
- TypeScript では `import { v4 as uuidv4 } from "uuid"` で生成する。

---

### Plugin Name（プラグイン種別名）

**定義:** プラグインの**種類**を表す文字列識別子。インスタンスが異なっても同じ種類なら同じ値になる（UUID ではない）。

**命名規則:** `"{basePlugin}.{subPlugin}"` のドット区切り形式

| 例 | 意味 |
|---|---|
| `"base.test_object"` | base プラグインの test_object オブジェクト |
| `"base.blur_effect"` | base プラグインの blur エフェクト |

**用途:**
- `item.object.name` — プラグインレジストリの検索キー（`object_plugins[name]` で引く）
- `effect.name` — エフェクトプラグインの検索キー（`effect_plugins[name]` で引く）
- TS側では `getPluginNames()` の戻り値 `basePlugin`・`videoObjectPlugins`・`videoEffectPlugins` のキーとして使われる
- エフェクトメニューでは `effectId.startsWith(baseId + ".")` でフィルタリングしている

**注意:**
- `GenerateStructure.name` は UUID ではない。インスタンスを一意に識別するには `id` を使うこと。
- Python プラグインでは `self.name = "base.blur_effect"` のように設定し、`register_sub_plugin()` で登録される。

---

### Parameter ID（パラメータ識別子）

**定義:** `RequestStructureParameter` の各パラメータを識別する文字列。プラグイン内で一意であれば良く、UUID ではない。

**命名規則:** `snake_case` の短い英語名

| 例 | 意味 |
|---|---|
| `"blur_radius"` | ブラー半径パラメータ |
| `"draw_text"` | テキスト描画フラグ |
| `"text_pos"` | テキスト位置 |

**用途:**
- `parameters` 辞書のキー（`item.object.parameters["blur_radius"]` のようにアクセス）
- `params` 辞書のキー（UI側 `ConfigableValue` の辞書でも同じキーを使う）
- `RequestStructureParameter.id` として定義し、`param.defaultValue` とセットで扱う

**注意:**
- パラメータ ID はプラグインをまたいで重複してもよい（スコープがプラグイン単位のため）。
- プラグイン内での重複は TODO としてチェックが必要（`src/renderer/parameterEditor/BaseParameter.tsx` のコメント参照）。

---

### Display Name（表示名）

**定義:** UI に表示する人間向けの名前。ユーザーが見る文字列。

| フィールド | 設定場所 | 例 |
|---|---|---|
| `item.object.displayName` | プラグインの `display_name` | `"テストオブジェクト"` |
| `effect.displayName` | プラグインの `display_name` | `"ブラー"` |
| `plugin.display_name` | `MainPluginBase` サブクラス | `"基本"` |

**注意:**
- Python 側は `snake_case`（`display_name`）、TypeScript 側は `camelCase`（`displayName`）。napi-rs が自動変換する。
- ユーザーが編集することはない読み取り専用の文字列。

---

## 命名チートシート

| 概念 | TypeScript変数/型 | 複数形 |
|---|---|---|
| タイムライン上の要素（Video/Audio混在） | `item: ItemStructure` | `timelineItems` |
| 映像アイテム | `item: ItemStructure` (`type === "Video"`) | `getCurrentVideoItems()` |
| 音声アイテム | `item: ItemStructure` (`type === "Audio"`) | `getCurrentAudioItems(duration)` |
| 選択中IDリスト | `selectedItemIds: string[]` | — |
| メイン選択ID | `mainSelectedItemId: string \| null` | — |
| z-index位置 | `item.layer: number` | — |
| 開始・終了フレーム | `item.start`, `item.end` | — |
| オブジェクト本体 | `item.object: GenerateStructure` | — |
| エフェクト | `effect: GenerateStructure` | `item.effects` |
| パラメータ値辞書 | `params: Record<string, ConfigableValue>` | — |
| パラメータ定義リスト | `structures: RequestStructureParameter[]` | — |
| native ブリッジ型（TS/Rust/Python） | `ItemStructure` | — |
| フレームレンダリング結果 | `ItemResult` | `frameResults: Record<string, ItemResult>` |
| 音声設定 | `audioState: AudioState` | — |
| 映像プラグイン一覧 | `PluginNameInfo.videoObjectPlugins` / `videoEffectPlugins` | — |
| 音声プラグイン一覧 | `PluginNameInfo.audioObjectPlugins` / `audioEffectPlugins` | — |
