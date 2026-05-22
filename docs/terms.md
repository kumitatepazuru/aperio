# Aperio 用語定義

このドキュメントはコードベース全体で使われる概念・用語の定義と用法を定めたものです。
新しい概念を追加する際は必ずここに記載し、一貫した命名を維持してください。

---

## Item（アイテム）

**定義:** タイムライン上に配置された編集要素の単位。動画・画像・テキストなどのオブジェクトと、それに付随するエフェクトをまとめたもの。

**型:** `ItemStructure`

| フィールド | 型 | 意味 |
|---|---|---|
| `id` | `string` | UUID。アイテム固有の識別子 |
| `layer` | `number` | レイヤー番号（z-index）。0が最下層 |
| `from` | `number` | 開始フレーム |
| `to` | `number` | 終了フレーム |
| `min` | `number` (optional) | 最小デュレーション（フレーム数）。プラグインの `minFrame` から自動設定 |
| `max` | `number` (optional) | 最大デュレーション（フレーム数）。プラグインの `maxFrame` から自動設定 |
| `x`, `y` | `number` | キャンバス上の位置（px） |
| `scale` | `number` | 拡大率（%、100が等倍） |
| `rotation` | `number` | 回転角（度） |
| `alpha` | `number` | 不透明度（%、100が不透明） |
| `object` | `GenerateStructure` | このアイテムのオブジェクト本体 |
| `effects` | `GenerateStructure[]` | 適用されているエフェクトのリスト |

**Store上の名称:**
- `timelineItems: ItemStructure[]` — 全アイテムのリスト
- `selectedItemIds: string[]` — 選択中アイテムのIDリスト（複数選択対応）
- `mainSelectedItemId: string | null` — パラメータ編集の基準となる「メイン選択」アイテムのID

**注意:**
- 旧称は `timelineLayers` / `TimelineLayerStructure`。Layer はz-index概念に限定するためリネーム済み。
- `selectedItemIds` は複数形。かつての `selectedItemId: string[]`（単数形・複数値）から改名。
- ループ変数は `item` を使う（`layer` は使わない）。
- `min` / `max` は `GeneratorInformation.minFrame` / `maxFrame` の値をプラグイン呼び出し時に自動設定する。

### ItemStructure（ネイティブブリッジ型）

**定義:** napi-rs が生成する native の構造体。Rust → Python・TypeScript 間のアイテムデータ転送フォーマット。

**型定義:**
- Rust: `src/native/src/structs/frame_structure.rs` → `pub struct ItemStructure`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface ItemStructure`
- Python: `src/native/src/structs/frame_structure.rs`（pyo3 で定義、スタブは `src-python/out/aperio/frame_structure.pyi`）→ `class ItemStructure(dict)`

**フィールド（Python側）:**
- `id`, `layer`, `from`, `to`, `min`, `max`, `x`, `y`, `scale`, `rotation`, `alpha` — アイテムの基本プロパティ
- `object: GenerateStructure` — オブジェクトプラグインのインスタンス情報
- `effects: list[GenerateStructure]` — エフェクトプラグインのインスタンスリスト

**注意:**
- Python側では `item["object"]["name"]` でオブジェクト名にアクセスする。

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
- 旧称は `obj`。`object` に統一済み。
- Rust構造体での定義: `pub object: GenerateStructure`（`src/native/src/structs/frame_structure.rs`）
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
- Rust: `src/native/src/structs/frame_structure.rs` → `pub struct GenerateStructure`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface GenerateStructure`
- Python: `src/native/src/structs/frame_structure.rs`（pyo3 で定義、スタブは `src-python/out/aperio/frame_structure.pyi`）→ `class GenerateStructure(dict)`

**用法:**
- `item.object` — オブジェクト本体としての GenerateStructure
- `item.effects[i]` — エフェクトとしての GenerateStructure

---

## ItemResult

**定義:** フレームレンダリング後、各アイテムの出力サイズ情報を格納する型。

**型定義:**
- Rust: `src/native/src/structs/frame_structure.rs` → `pub struct ItemResult`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface ItemResult`
- Python: `src/native/src/structs/frame_structure.rs`（pyo3 で定義、スタブは `src-python/out/aperio/frame_structure.pyi`）→ `class ItemResult`

**フィールド:** `width: number`, `height: number`

**用法:**
- `frameResults: Record<string, ItemResult>` — ストアのトップレベルフィールド。アイテムIDをキーとした描画結果辞書
- `Overlay.tsx` でアイテムの表示サイズ計算に使用

---

## Frame（フレーム）

**定義:** 映像の1コマ。時間軸の単位。

**用法:**
- `from`, `to` — アイテムの開始・終了フレーム番号（整数）
- `viewerState.beginFrame` — 現在（一時停止時）または再生開始時のフレーム番号
- `frameState` — キャンバスの解像度とFPS（`FrameState: { width, height, fps }`）
- `frameResults` — ストアのトップレベルフィールド。各アイテムのレンダリング結果（`Record<string, ItemResult>`）

**注意:**
- `Frame` クラス（`src/renderer/bridge.ts`）はフレームバッファの送受信を担うネットワーク層のクラス。映像フレームの概念とは別物。

---

## Plugin（プラグイン）

**定義:** Pythonで実装されたオブジェクト・エフェクトの処理単位。

**種別:**

| 種別 | 意味 |
|---|---|
| Object Plugin | アイテムの映像コンテンツを生成するプラグイン |
| Effect Plugin | アイテムに映像効果を適用するプラグイン |

**参照:** `PluginNameInfo.basePlugin`, `PluginNameInfo.objectPlugins`, `PluginNameInfo.effectPlugins`

**イベントシステム:**
- プラグインのイベントハンドラーは `@event(type=GeneratorEvent.New)` 等のデコレーターで登録する。
- `EventManager.call_event(plugin_name, type, params)` で統一的に呼び出す。
- `params` 引数は常に `dict`。Effect の `New` イベントでも空の dict を渡す。

---

## GeneratorEvent

**定義:** プラグインのイベント種別を識別するenum。Rustで定義しPyO3・napi経由でPython・TypeScriptに公開する。両言語間の手動同期は不要。

**型定義:**
- Rust: `src/native/src/structs/frame_structure.rs` → `pub enum GeneratorEvent`
- Python: `aperio.frame_structure.GeneratorEvent`（PyO3経由、スタブは `src-python/out/aperio/frame_structure.pyi`）

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
- Rust: `src/native/src/structs/frame_structure.rs` → `pub struct GeneratorInformation`
- TypeScript: `dist/native/index.d.ts`（napi-rs による自動生成）→ `interface GeneratorInformation`
- Python: `src/native/src/structs/frame_structure.rs`（pyo3 で定義、スタブは `src-python/out/aperio/frame_structure.pyi`）→ `class GeneratorInformation`

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
- TS側では `getPluginNames()` の戻り値 `basePlugin`・`objectPlugins`・`effectPlugins` のキーとして使われる
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
| タイムライン上の要素 | `item: ItemStructure` | `timelineItems` |
| 選択中IDリスト | `selectedItemIds: string[]` | — |
| メイン選択ID | `mainSelectedItemId: string \| null` | — |
| z-index位置 | `item.layer: number` | — |
| オブジェクト本体 | `item.object: GenerateStructure` | — |
| エフェクト | `effect: GenerateStructure` | `item.effects` |
| パラメータ値辞書 | `params: Record<string, ConfigableValue>` | — |
| パラメータ定義リスト | `structures: RequestStructureParameter[]` | — |
| native ブリッジ型（TS/Rust/Python） | `ItemStructure` | — |
| フレームレンダリング結果 | `ItemResult` | `frameResults: Record<string, ItemResult>` |
