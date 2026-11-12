use std::collections::HashMap;

use aperio_macros::PyDataclass;
use napi_derive::napi;
use pyo3::{prelude::*, types::PyDict};

use crate::utils::json_value::JsonValue;

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct FileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

/// 1アイテムの生成結果。変形はすべて画面上の絶対値で持つ。
// TODO: アイテムの基礎情報を別にまとめることで、OffScreenSourceを別途定義しなくていいようにする
#[napi(object)]
#[pyclass(from_py_object, get_all, set_all, eq)]
#[derive(Clone, PartialEq, Debug)]
pub struct ItemResult {
    pub width: i32,
    pub height: i32,
    // キャンバス中央を原点とした座標(px)。Z は奥行き
    pub pos: (f64, f64, f64),
    // 回転・拡縮の基点オフセット(拡大前のテクスチャpx)。実機は Q12(1/4096px)で持つので、半画素の
    // マージン(`領域拡張` の `(左-右)/2`)を落とさないよう整数ではなく実数。
    pub center_x: f64,
    pub center_y: f64,
    // 拡大率(1.0 = 等倍)
    pub scale: (f64, f64),
    // X/Y/Z 軸の回転(度)
    pub rotate: (f64, f64, f64),
    // 不透明度(1.0 = 不透明)
    pub alpha: f64,
    // エフェクトが「自分の背面/前面に追加のアイテムを流し込みたい」場合に積む。
    // 合成ループはチェーンの最後にまとめて、実アイテムと全く同じコードパスで処理する。
    pub additional_items: Vec<AdditionalItem>,
    // オフスクリーン描画済み(アイテムの変形をテクスチャに焼き込み済み)のフラグ
    pub offscreen: bool,
    // 焼き込み直前のオブジェクトの形。offscreenではないときはNone。
    pub offscreen_source: Option<OffscreenSource>,
}

/// オフスクリーン描画で焼き込まれる直前の、オブジェクトの幅・高さと累積変形(絶対値)。
#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug)]
pub struct OffscreenSource {
    pub width: i32,
    pub height: i32,
    pub pos: (f64, f64, f64),
    pub center_x: f64,
    pub center_y: f64,
    pub scale: (f64, f64),
    pub rotate: (f64, f64, f64),
    pub alpha: f64,
}

impl From<&ItemResult> for OffscreenSource {
    fn from(r: &ItemResult) -> Self {
        Self {
            width: r.width,
            height: r.height,
            pos: r.pos,
            center_x: r.center_x,
            center_y: r.center_y,
            scale: r.scale,
            rotate: r.rotate,
            alpha: r.alpha,
        }
    }
}

impl ItemResult {
    /// デフォルト値
    pub fn identity(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            pos: (0.0, 0.0, 0.0),
            center_x: 0.0,
            center_y: 0.0,
            scale: (1.0, 1.0),
            rotate: (0.0, 0.0, 0.0),
            alpha: 1.0,
            additional_items: Vec::new(),
            offscreen: false,
            offscreen_source: None,
        }
    }

    /// Video ItemからResultを作る
    pub fn from_item_structure(item: &ItemStructure, width: i32, height: i32) -> Option<Self> {
        let ItemStructure::Video {
            x,
            y,
            scale,
            rotation,
            alpha,
            ..
        } = item
        else {
            return None;
        };
        let mut r = Self::identity(width, height);
        r.add_pos_inner(*x, *y, 0.0);
        r.mul_scale_inner(scale / 100.0, scale / 100.0);
        r.add_rotate_inner(0.0, 0.0, *rotation);
        r.mul_alpha_inner(alpha / 100.0);
        Some(r)
    }

    fn add_pos_inner(&mut self, x: f64, y: f64, z: f64) {
        self.pos = (self.pos.0 + x, self.pos.1 + y, self.pos.2 + z);
    }

    fn add_center_inner(&mut self, x: f64, y: f64) {
        self.center_x += x;
        self.center_y += y;
    }

    fn mul_scale_inner(&mut self, x: f64, y: f64) {
        self.scale = (self.scale.0 * x, self.scale.1 * y);
    }

    fn add_rotate_inner(&mut self, x: f64, y: f64, z: f64) {
        self.rotate = (self.rotate.0 + x, self.rotate.1 + y, self.rotate.2 + z);
    }

    fn mul_alpha_inner(&mut self, a: f64) {
        self.alpha *= a;
    }

    /// otherを差分として合成する。pos/center/rotate は加算、scale/alpha は乗算、additional_items は連結。
    /// サイズは自身のまま変えない。
    pub fn add_assign_delta(&mut self, other: &ItemResult) {
        self.add_pos_inner(other.pos.0, other.pos.1, other.pos.2);
        self.add_center_inner(other.center_x, other.center_y);
        self.mul_scale_inner(other.scale.0, other.scale.1);
        self.add_rotate_inner(other.rotate.0, other.rotate.1, other.rotate.2);
        self.mul_alpha_inner(other.alpha);
        self.additional_items.extend(other.additional_items.iter().cloned());
    }

    /// 現在の変形をキャンバス大のテクスチャに焼き込んだ後の結果。焼き込み前の形を offscreen_source に残す。
    pub fn to_offscreen_inner(&self, width: i32, height: i32) -> Self {
        let mut r = Self::identity(width, height);
        r.offscreen = true;
        r.offscreen_source = Some(OffscreenSource::from(self));
        r.additional_items = self.additional_items.clone();
        r
    }
}

#[pymethods]
impl ItemResult {
    #[new]
    #[pyo3(signature = (width, height, pos=None, center_x=None, center_y=None, scale=None, rotate=None, alpha=None, additional_items=None, offscreen=false))]
    fn new(
        width: i32,
        height: i32,
        pos: Option<(f64, f64, f64)>,
        center_x: Option<f64>,
        center_y: Option<f64>,
        scale: Option<(f64, f64)>,
        rotate: Option<(f64, f64, f64)>,
        alpha: Option<f64>,
        additional_items: Option<Vec<AdditionalItem>>,
        offscreen: bool,
    ) -> Self {
        let mut r = Self::identity(width, height);
        if let Some(v) = pos {
            r.pos = v;
        }
        if let Some(v) = center_x {
            r.center_x = v;
        }
        if let Some(v) = center_y {
            r.center_y = v;
        }
        if let Some(v) = scale {
            r.scale = v;
        }
        if let Some(v) = rotate {
            r.rotate = v;
        }
        if let Some(v) = alpha {
            r.alpha = v;
        }
        if let Some(v) = additional_items {
            r.additional_items = v;
        }
        r.offscreen = offscreen;
        r
    }

    /// Video ItemからResultを作る
    #[staticmethod]
    #[pyo3(signature = (item, width=0, height=0))]
    fn from_item(item: ItemStructure, width: i32, height: i32) -> PyResult<Self> {
        Self::from_item_structure(&item, width, height)
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("ItemResult.from_item only accepts ItemStructure.Video"))
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    fn __copy__(&self) -> Self {
        self.clone()
    }

    /// 累積した変形を保ったまま、サイズだけを変えたコピーを返す。
    fn with_size(&self, width: i32, height: i32) -> Self {
        let mut r = self.clone();
        r.width = width;
        r.height = height;
        r
    }

    #[pyo3(signature = (x, y, z=0.0))]
    fn add_pos(mut slf: PyRefMut<'_, Self>, x: f64, y: f64, z: f64) -> PyRefMut<'_, Self> {
        slf.add_pos_inner(x, y, z);
        slf
    }

    fn add_center(mut slf: PyRefMut<'_, Self>, x: f64, y: f64) -> PyRefMut<'_, Self> {
        slf.add_center_inner(x, y);
        slf
    }

    fn mul_scale(mut slf: PyRefMut<'_, Self>, x: f64, y: f64) -> PyRefMut<'_, Self> {
        slf.mul_scale_inner(x, y);
        slf
    }

    fn add_rotate(mut slf: PyRefMut<'_, Self>, x: f64, y: f64, z: f64) -> PyRefMut<'_, Self> {
        slf.add_rotate_inner(x, y, z);
        slf
    }

    fn mul_alpha(mut slf: PyRefMut<'_, Self>, a: f64) -> PyRefMut<'_, Self> {
        slf.mul_alpha_inner(a);
        slf
    }

    fn add_additional_item(mut slf: PyRefMut<'_, Self>, item: AdditionalItem) -> PyRefMut<'_, Self> {
        slf.additional_items.push(item);
        slf
    }

    fn __add__(&self, other: ItemResult) -> Self {
        let mut r = self.clone();
        r.add_assign_delta(&other);
        r
    }

    fn __iadd__(&mut self, other: ItemResult) {
        self.add_assign_delta(&other);
    }

    /// 現在の変形を width x height のテクスチャに焼き込んだ後の結果を返す。
    fn to_offscreen(&self, width: i32, height: i32) -> Self {
        self.to_offscreen_inner(width, height)
    }

    fn __repr__(&self) -> String {
        format!(
            "ItemResult(width={:?}, height={:?}, pos={:?}, center_x={:?}, center_y={:?}, scale={:?}, rotate={:?}, alpha={:?}, additional_items={:?}, offscreen={:?}, offscreen_source={:?})",
            self.width, self.height, self.pos, self.center_x, self.center_y, self.scale, self.rotate, self.alpha, self.additional_items, self.offscreen, self.offscreen_source
        )
    }
}

#[napi]
#[pyclass(from_py_object)]
#[derive(Clone, Debug, PartialEq)]
pub enum RequestStructureParameter {
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Float {
        id: String,
        title: String,
        default_value: f64,
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Int {
        id: String,
        title: String,
        default_value: i64,
        suffix: Option<String>,
        min: Option<i64>,
        max: Option<i64>,
    },
    Bool {
        id: String,
        title: String,
        default_value: bool,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec2Int {
        id: String,
        title: String,
        default_value: (i32, i32),
        suffix: Option<String>,
        min: Option<i32>,
        max: Option<i32>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec2Float {
        id: String,
        title: String,
        default_value: (f64, f64),
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec3Int {
        id: String,
        title: String,
        default_value: (i32, i32, i32),
        suffix: Option<String>,
        min: Option<i32>,
        max: Option<i32>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec3Float {
        id: String,
        title: String,
        default_value: (f64, f64, f64),
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec4Int {
        id: String,
        title: String,
        default_value: (i32, i32, i32, i32),
        suffix: Option<String>,
        min: Option<i32>,
        max: Option<i32>,
    },
    #[pyo3(constructor = (id, title, default_value, suffix=None, min=None, max=None))]
    Vec4Float {
        id: String,
        title: String,
        default_value: (f64, f64, f64, f64),
        suffix: Option<String>,
        min: Option<f64>,
        max: Option<f64>,
    },
    String {
        id: String,
        title: String,
        default_value: String,
    },
    Color {
        id: String,
        title: String,
        default_value: (f64, f64, f64, f64), // RGBA形式で0.0〜1.0の範囲
        use_alpha: bool,
    },
    List {
        id: String,
        title: String,
        values: HashMap<String, String>, // key: valueのペア
        default_value: String,
    },
    Font {
        id: String,
        title: String,
        // デフォルト値は固定: family=None, weight=400
    },
    Textarea {
        id: String,
        title: String,
        default_value: String,
    },
    File {
        id: String,
        title: String,
        multi_selections: bool,
        open_type: String, // "file" | "directory"
        filters: Vec<FileFilter>,
    },
}

#[napi(object)]
#[pyclass(from_py_object, extends = PyDict)]
#[derive(Clone, IntoPyObject, PartialEq, Debug)]
pub struct GenerateStructure {
    pub id: String,   // UUIDが期待される
    pub name: String, // オブジェクトやエフェクトの固有名でIDとは違い種類が同じであれば同じになる
    pub display_name: String,
    pub parameters: HashMap<String, JsonValue>, // パラメータの具体的な型はエフェクトによって異なるため、単にdict(JsonValue)型とする
    // Some の場合、このエントリは実体を持たず「link_id が指す別エントリの計算結果を
    // フレーム内で使い回す」ことだけを意味する(name/display_name/parameters は無視される)。
    pub link_id: Option<String>,
}

#[pymethods]
impl GenerateStructure {
    /// Python側から additionalItem 用に GenerateStructure を新規構築するためのコンストラクタ。
    #[new]
    #[pyo3(signature = (id, name, display_name, parameters, link_id=None))]
    fn new(
        py: Python<'_>,
        id: String,
        name: String,
        display_name: String,
        parameters: Py<PyDict>,
        link_id: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            id,
            name,
            display_name,
            parameters: parameters.bind(py).extract()?,
            link_id,
        })
    }
}

#[napi]
#[pyclass(from_py_object)]
#[derive(Clone, Debug, PartialEq)]
pub enum ItemStructure {
    #[pyo3(constructor = (id, layer, start, end, x, y, scale, rotation, alpha, object, effects, min=None, max=None))]
    Video {
        id: String,
        layer: i32,
        start: i32, // アイテムの開始フレーム
        end: i32,   // アイテムの終了フレーム
        x: f64,
        y: f64,
        scale: f64,
        rotation: f64,
        alpha: f64,
        object: GenerateStructure,
        effects: Vec<GenerateStructure>,
        min: Option<i32>, // アイテムの有効な最小フレーム（省略可能）
        max: Option<i32>, // アイテムの有効な最大フレーム（省略可能）
    },
    #[pyo3(constructor = (id, layer, start, end, volume, pan, object, effects, min=None, max=None))]
    Audio {
        id: String,
        layer: i32,
        start: i32, // アイテムの開始フレーム
        end: i32,   // アイテムの終了フレーム
        volume: f64,
        pan: f64,
        object: GenerateStructure,
        effects: Vec<GenerateStructure>,
        min: Option<i32>,
        max: Option<i32>,
    },
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct AdditionalItem {
    pub item: ItemStructure,
    /// true = 挿入元アイテムより背面(下)に挿入 / false = 前面(上)に挿入
    pub behind: bool,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct GeneratorInformation {
    pub display_name: String,
    pub duration_frames: Option<i32>,
    pub max_frame: Option<i32>,
    pub min_frame: Option<i32>,
    pub structure: Vec<RequestStructureParameter>,
}


#[napi]
#[pyclass(eq, from_py_object)]
#[derive(PartialEq, Clone, Debug)]
pub enum GeneratorEvent {
    New,
    RequestStructure,
    BeforeMakeFrame,
    AfterMakeFrame,
    ItemMove,
    ItemScale,
    ItemRotate,
}

#[napi(object)]
#[pyclass(from_py_object, get_all, eq)]
#[derive(Clone, PartialEq, Debug, PyDataclass)]
pub struct PluginNameInfo {
    pub base_plugin: HashMap<String, String>,
    pub video_object_generators: HashMap<String, String>,
    pub video_effect_generators: HashMap<String, String>,
    pub audio_object_generators: HashMap<String, String>,
    pub audio_effect_generators: HashMap<String, String>,
}

// ---- モジュール登録 -------------------------------------------------------

#[pymodule(module = "aperio.item_structures")]
pub mod item_structures {
    #[pymodule_export]
    use super::AdditionalItem;
    #[pymodule_export]
    use super::FileFilter;
    #[pymodule_export]
    use super::GenerateStructure;
    #[pymodule_export]
    use super::GeneratorEvent;
    #[pymodule_export]
    use super::GeneratorInformation;
    #[pymodule_export]
    use super::ItemResult;
    #[pymodule_export]
    use super::ItemStructure;
    #[pymodule_export]
    use super::OffscreenSource;
    #[pymodule_export]
    use super::PluginNameInfo;
    #[pymodule_export]
    use super::RequestStructureParameter;
}
