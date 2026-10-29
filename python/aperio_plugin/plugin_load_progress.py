import tkinter as tk
from tkinter import ttk

from .plugin_base import GeneratorBase


class PluginLoadProgressWindow:
    """
    プラグイン読み込み中の進捗を表示する ttk ベースのウィンドウ。
    mainloop を起動しない同期処理中に使うため、各更新メソッドが手動で
    イベントループを1回分回して再描画する。
    """

    BAR_LENGTH = 320

    def __init__(self) -> None:
        self.root = tk.Tk()
        self.root.title("Aperio: プラグインを読み込み中...")
        self.root.resizable(False, False)
        self.root.protocol("WM_DELETE_WINDOW", lambda: None)
        self.root.attributes("-topmost", True)

        self._label1 = ttk.Label(self.root, text="")
        self._bar1 = ttk.Progressbar(
            self.root, maximum=100, length=self.BAR_LENGTH, mode="determinate"
        )
        self._label2 = ttk.Label(self.root, text="")
        self._bar2 = ttk.Progressbar(
            self.root, maximum=1, length=self.BAR_LENGTH, mode="determinate"
        )
        self._label3 = ttk.Label(self.root, text="")
        # bar3 はメインプラグインごとのジェネレーター登録の進捗。総数が事前に分からないので不確定表示にする
        self._bar3 = ttk.Progressbar(
            self.root, length=self.BAR_LENGTH, mode="indeterminate"
        )

        self._label1.grid(row=0, column=0, sticky="w", padx=12, pady=(12, 0))
        self._bar1.grid(row=1, column=0, sticky="ew", padx=12, pady=(0, 8))
        self._label2.grid(row=2, column=0, sticky="w", padx=12)
        self._bar2.grid(row=3, column=0, sticky="ew", padx=12, pady=(0, 8))
        self._label3.grid(row=4, column=0, sticky="w", padx=12)
        self._bar3.grid(row=5, column=0, sticky="ew", padx=12, pady=(0, 12))
        self._label3.grid_remove()
        self._bar3.grid_remove()

        self.root.update_idletasks()
        self.root.eval("tk::PlaceWindow . center")
        self._refresh()

    def _refresh(self) -> None:
        self.root.update_idletasks()
        self.root.update()

    def set_bar1(self, value: int, message: str) -> None:
        self._bar1["value"] = value
        self._label1["text"] = message
        self._refresh()

    def start_bar2(self, maximum: int, message: str) -> None:
        self._bar2["maximum"] = max(maximum, 1)
        self._bar2["value"] = 0
        self._label2["text"] = message
        self._refresh()

    def step_bar2(self, message: str) -> None:
        self._bar2["value"] += 1
        self._label2["text"] = message
        self._refresh()

    def show_bar3(self) -> None:
        self._bar3["value"] = 0
        self._label3["text"] = ""
        self._label3.grid()
        self._bar3.grid()
        self._refresh()

    def step_bar3(self, message: str) -> None:
        self._bar3.step(10)
        self._label3["text"] = message
        self._refresh()

    def hide_bar3(self) -> None:
        self._label3.grid_remove()
        self._bar3.grid_remove()
        self._refresh()

    def close(self) -> None:
        self.root.destroy()


class LoadProgressReporter:
    """
    プラグイン読み込みの各段階を PluginLoadProgressWindow の表示に変換する。
    with 文で使い、抜けるとウィンドウを閉じる。

    - bar1: 全体の段階 (entry point の import → メインプラグインのインスタンス化)
    - bar2: 段階内のメインプラグイン単位の進捗
    - bar3: インスタンス化中のメインプラグインが登録したジェネレーター
    """

    def __init__(self) -> None:
        self._window = PluginLoadProgressWindow()
        self._generator_count = 0

    def __enter__(self) -> "LoadProgressReporter":
        return self

    def __exit__(self, *_exc: object) -> None:
        self._window.close()

    def begin_import(self, total: int) -> None:
        self._window.set_bar1(50, "プラグインを読み込み中")
        self._window.start_bar2(total, "")

    def step_import(self, name: str) -> None:
        self._window.step_bar2(name)

    def begin_instantiate(self, total: int) -> None:
        self._window.set_bar1(100, "ジェネレーターを登録中")
        self._window.start_bar2(total, "")

    def begin_plugin(self, name: str) -> None:
        self._window.step_bar2(name)
        self._window.hide_bar3()
        self._generator_count = 0

    def generator_added(self, generator: GeneratorBase) -> None:
        if self._generator_count == 0:
            self._window.show_bar3()
        self._generator_count += 1
        self._window.step_bar3(f"({self._generator_count}) {generator.display_name}")
