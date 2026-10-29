from collections.abc import Mapping

from aperio import gpu_util
from aperio.item_structures import GeneratorEvent, GeneratorInformation, ItemResult, ItemStructure, PluginNameInfo

from .audio_renderer import AudioRenderer
from .component_registry import ComponentRegistry
from .context import AperioContext
from .event_manager import EventDispatcher
from .generator_registry import GeneratorRegistry
from .plugin_base import GeneratorBase, MainPluginBase, PluginBase
from .plugin_base.generator_base import GeneratorKind
from .plugin_loader import PluginLoader
from .video_renderer import VideoRenderer


class PluginManager:
    """
    Aperioのプラグイン実行環境のファサード。Rust 側とプラグインの両方からaperio_plugin.plugin_managerとして参照される。
    実際の処理はプラグインの読み込み(PluginLoader)・イベント(EventDispatcher)・描画(VideoRenderer/AudioRenderer)などに委譲する。
    """

    def __init__(self, context: AperioContext) -> None:
        self.context = context
        self.generators = GeneratorRegistry()
        self.components = ComponentRegistry(context.store_manager)
        self._loader = PluginLoader(self.generators)
        self._events = EventDispatcher(self.generators)
        self._video = VideoRenderer(context, self.generators)
        self._audio = AudioRenderer(context, self.generators)

    # --- プラグイン管理 ---

    @property
    def plugins(self) -> Mapping[str, MainPluginBase]:
        """読み込み済みのメインプラグイン (名前 → インスタンス)"""
        return self._loader.plugins

    def load_plugins(self) -> None:
        """インストール済みのプラグインパッケージを entry point から読み込む。"""
        self._loader.load_all()

    def add_generator(self, generator: GeneratorBase) -> None:
        """
        ジェネレーターを登録する。メインプラグインの__init__の中から呼ぶ。
        ジェネレーター名はメインプラグイン名 + . で始まる必要がある。

        Args:
            generator: 登録するジェネレーターのインスタンス
        """
        self._loader.add_generator(generator)

    def add_component(self, plugin: PluginBase, dir: str, type: str) -> None:
        """
        UI のPluginComponentに差し込む React コンポーネントを登録する。
        登録情報はstoreのplugin_componentsに保存され、UIはplugin://{plugin.name}/{type}/として読み込む。
        同じ(plugin.name, type)を再登録した場合は警告を出して無視する

        Args:
            plugin: コンポーネントを登録するプラグインのインスタンス (plugin.nameがpluginIdになる)
            dir: ビルド済みの index.jsがあるディレクトリのパス (絶対パスに変換される)
            type: 差し込み先の type
        """
        self.components.add(plugin, dir, type)

    def get_plugin_names(self) -> PluginNameInfo:
        """
        登録されているプラグインの name と display_name の対応表を返す。

        Returns:
            PluginNameInfo: プラグイン名と表示名の辞書
        """
        return PluginNameInfo(
            base_plugin={plugin.name: plugin.display_name for plugin in self.plugins.values()},
            video_object_plugins=self.generators.display_names(GeneratorKind.VIDEO_OBJECT),
            video_effect_plugins=self.generators.display_names(GeneratorKind.VIDEO_EFFECT),
            audio_object_plugins=self.generators.display_names(GeneratorKind.AUDIO_OBJECT),
            audio_effect_plugins=self.generators.display_names(GeneratorKind.AUDIO_EFFECT),
        )

    # --- イベント ---

    def call_event(self, plugin_name: str, type: GeneratorEvent, params: dict) -> GeneratorInformation:
        """
        指定ジェネレーターの、指定イベント種別に対応するデコレーター付きメソッドを呼び出す。

        Args:
            plugin_name: 対象ジェネレーターの名前
            type: イベント種別
            params: イベントハンドラーに渡すパラメータ辞書

        Returns:
            イベント種別に応じた戻り値
        """
        return self._events.call_event(plugin_name, type, params)

    # --- 描画 ---

    def get_fonts_list(self) -> dict[str, list[int]]:
        """
        システムにインストールされているフォントの一覧を返す。

        Returns:
            dict[str, list[int]]: フォントファミリー名, ウェイト値（100/200/…/900）のリスト
        """
        return self.context.text_renderer.get_fonts_list()

    def make_frame_buf(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        buffer_ptr: int,
    ) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定されたバッファに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            buffer_ptr (int): 書き込み先バッファのポインタ

        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
        return self._video.render_buf(frame_number, frame_structure, width, height, buffer_ptr)

    def make_frame_shared_texture(
        self,
        frame_number: int,
        frame_structure: list[ItemStructure],
        width: int,
        height: int,
        texture_handle: gpu_util.PySharedTextureHandle,
        format: gpu_util.WrappedSharedTextureFormat,
    ) -> dict[str, ItemResult]:
        """
        指定されたフレーム構造に基づいてフレームを生成し、指定された共有テクスチャに書き込むメソッド。

        Args:
            frame_number (int): 生成するフレームの番号
            frame_structure (list[ItemStructure]): フレーム構造のリスト
            width (int): フレームの幅
            height (int): フレームの高さ
            texture_handle (gpu_util.PySharedTextureHandle): 書き込み先の共有テクスチャハンドル
            format (gpu_util.WrappedSharedTextureFormat): 共有テクスチャのフォーマット

        Returns:
            dict[str, ItemResult]: 各レイヤーのフレーム生成結果の辞書
        """
        return self._video.render_shared_texture(frame_number, frame_structure, width, height, texture_handle, format)

    def play_audio(self, audio_structure: list[ItemStructure], sample_rate: int, channels: int, start_time: float, duration: float) -> None:
        """
        指定されたオーディオ構造に基づいてオーディオを生成し、再生するメソッド。

        Args:
            audio_structure (list[ItemStructure]): オーディオ構造のリスト
            sample_rate (int): サンプルレート
            channels (int): チャンネル数
            start_time (float): 再生を開始する時間（秒）
            duration (float): 再生する期間（秒）
        """
        self._audio.play(audio_structure, sample_rate, channels, start_time, duration)
