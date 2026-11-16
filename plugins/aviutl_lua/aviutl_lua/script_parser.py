"""AviUtl 形式の Lua スクリプトの解析。

- ``split_sections``: ファイルを ``@表示名`` の行で区切る(無ければファイル丸ごと 1 つ)。
- ``parse_parameters``: セクション先頭に続く ``--track0:...`` などのパラメータ定義行を解析する。
"""

import re
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from aperio import logger
from aperio.item_structures import FileFilter, RequestStructureParameter

type LuaValue = float | int | str | bool
type LuaType = type[float] | type[int] | type[str] | type[bool]

TRACK_STEPS = {"1": 0, "0.1": 1, "0.01": 2}
"""--track の移動単位 → 丸める小数点以下の桁数"""
DEFAULT_TRACK_STEP = "0.1"
MAX_INDEX = 3
"""track/check の番号の上限(AviUtl は 0~3)。超えても登録はするが警告する。"""
DEFAULT_COLOR = 0xFFFFFF

_DIRECTIVE = re.compile(r"--(?P<kind>track(?P<track>\d+)|check(?P<check>\d+)|param|color|file|dialog):(?P<body>.*)")
_INT_LITERAL = re.compile(r"[+-]?\d+")
_NUMBER_LITERAL = re.compile(r"[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?|[+-]?0[xX][0-9a-fA-F]+")


@dataclass
class Section:
    """1 つのジェネレーターになるスクリプトの範囲。"""

    path: Path
    display_name: str
    lines: list[str]
    """セクションの本文(``@表示名`` の行は含まない)"""
    first_line: int
    """``lines[0]`` のファイル内での行番号(1 始まり)"""
    has_header: bool
    """``@表示名`` で区切られたセクションかどうか"""


@dataclass
class ScriptParameter:
    """UI のパラメータ 1 つと、それに対応する Lua の変数。"""

    ui: RequestStructureParameter
    id: str
    lua_name: str
    lua_type: LuaType
    default: LuaValue
    """UI から値が来なかったときに Lua に渡す値"""
    convert: Callable[[Any], LuaValue]
    """UI の値(params.args[id])を Lua に渡す値に変換する"""


@dataclass
class ParsedParameters:
    parameters: list[ScriptParameter] = field(default_factory=list)
    prelude: list[str] = field(default_factory=list)
    """UI で編集できない値(テーブルなど)の代入文。毎回スクリプトの前に実行する。"""


def read_script(path: Path) -> str:
    """AviUtl のスクリプトは Shift_JIS が多いので、UTF-8 で読めなければ cp932 で読む。"""
    data = path.read_bytes()
    try:
        return data.decode("utf-8-sig")
    except UnicodeDecodeError:
        return data.decode("cp932", errors="replace")


def split_sections(path: Path) -> list[Section]:
    """ファイルを ``@表示名`` の行で区切る。``@`` の行が無ければファイル丸ごと 1 つのセクションにする。
    最初の ``@`` の行より前の部分は(AviUtl と同じく)無視する。"""
    lines = read_script(path).splitlines()
    sections: list[Section] = []
    current: Section | None = None
    for i, line in enumerate(lines):
        if line.startswith("@"):
            display_name = line[1:].strip()
            if not display_name:
                logger.warning(f"{path.name}:{i + 1}: '@' の後に表示名がありません。このセクションは無視されます。")
                current = None
                continue
            current = Section(path, display_name, [], i + 2, True)
            sections.append(current)
        elif current is not None:
            current.lines.append(line)

    if not sections and not any(line.startswith("@") for line in lines):
        sections.append(Section(path, path.stem, lines, 1, False))
    return sections


def parse_parameters(section: Section) -> ParsedParameters:
    """セクションの先頭から連続するパラメータ定義行を解析する。定義行でない行が来たらそこで終わる。"""
    parser = _ParameterParser(section)
    for offset, line in enumerate(section.lines):
        m = _DIRECTIVE.fullmatch(line.strip())
        if m is None:
            break
        parser.line_no = section.first_line + offset
        parser.handle(m)
    return parser.result


class _ParameterParser:
    def __init__(self, section: Section) -> None:
        self.section = section
        self.result = ParsedParameters()
        self.line_no = section.first_line
        self._ids: set[str] = set()
        self._lua_names: set[str] = set()

    def warn(self, message: str) -> None:
        logger.warning(f"{self.section.path.name}:{self.line_no} ({self.section.display_name}): {message}")

    def handle(self, m: re.Match[str]) -> None:
        body = m["body"].strip()
        if m["track"] is not None:
            self.track(int(m["track"]), body)
        elif m["check"] is not None:
            self.check(int(m["check"]), body)
        else:
            match m["kind"]:
                case "param":
                    self.param(body)
                case "color":
                    self.color(body)
                case "file":
                    self.file(body)
                case "dialog":
                    self.dialog(body)

    def add(self, param: ScriptParameter) -> None:
        if param.id in self._ids or param.lua_name in self._lua_names:
            self.warn(f"'{param.lua_name}' is already defined. This definition is ignored.")
            return
        self._ids.add(param.id)
        self._lua_names.add(param.lua_name)
        self.result.parameters.append(param)

    def check_index(self, kind: str, index: int) -> None:
        if index > MAX_INDEX:
            self.warn(f"--{kind}{index}: {kind} is out of range.")

    # --trackX:名前,最小値,最大値,デフォルト値,移動単位
    def track(self, index: int, body: str) -> None:
        self.check_index("track", index)
        parts = [p.strip() for p in body.split(",")]
        if len(parts) not in (4, 5):
            self.warn(f"--track{index}: invalid syntax")
            return
        title = parts[0]
        try:
            lo, hi, default = (float(p) for p in parts[1:4])
        except ValueError:
            self.warn(f"--track{index}: all values must be numbers.")
            return

        step = parts[4] if len(parts) == 5 and parts[4] else DEFAULT_TRACK_STEP
        if step not in TRACK_STEPS:
            self.warn(
                f"--track{index}: movement unit '{step}' is not in {', '.join(TRACK_STEPS)}."
                f"Using {DEFAULT_TRACK_STEP} instead."
            )
            step = DEFAULT_TRACK_STEP
        digits = TRACK_STEPS[step]

        id = f"track{index}"
        ui: RequestStructureParameter
        if digits == 0:
            ui = RequestStructureParameter.Int(id=id, title=title, default_value=round(default), min=round(lo), max=round(hi))
        else:
            ui = RequestStructureParameter.Float(id=id, title=title, default_value=default, min=lo, max=hi)
        self.add(
            ScriptParameter(
                ui=ui,
                id=id,
                lua_name=f"obj.track{index}",
                lua_type=float,
                default=round(default, digits),
                convert=lambda v: round(float(v), digits),
            )
        )

    # --checkX:名前,デフォルト値(0か1)
    def check(self, index: int, body: str) -> None:
        self.check_index("check", index)
        parts = [p.strip() for p in body.split(",")]
        if len(parts) != 2 or parts[1] not in ("0", "1"):
            self.warn(f"--check{index}: invalid syntax.")
            return
        id = f"check{index}"
        default = parts[1] == "1"
        self.add(
            ScriptParameter(
                ui=RequestStructureParameter.Bool(id=id, title=parts[0], default_value=default),
                id=id,
                lua_name=f"obj.check{index}",
                lua_type=bool,
                default=default,
                convert=bool,
            )
        )

    # --param:整数  または  --param:dx=デフォルト値;dy=デフォルト値;
    def param(self, body: str) -> None:
        if _INT_LITERAL.fullmatch(body):
            default = int(body)
            self.add(
                ScriptParameter(
                    ui=RequestStructureParameter.Int(id="param", title="パラメータ", default_value=default),
                    id="param",
                    lua_name="param",
                    lua_type=int,
                    default=default,
                    convert=int,
                )
            )
            return

        for item in _split_items(body):
            name, sep, expr = item.partition("=")
            name = name.strip()
            if not sep or not name:
                self.warn(f"--param: '{item}': invalid syntax.")
                continue
            self.value(f"param.{name}", name, name, expr.strip())

    # --color:10進数 または 0x付き16進数
    def color(self, body: str) -> None:
        default = self.parse_color(body, "--color")
        self.add(_color_parameter("color", "色", "color", default))

    # --file:
    def file(self, body: str) -> None:
        if body:
            self.warn(f"--file: unexpected argument '{body}'.")
        self.add(
            ScriptParameter(
                ui=RequestStructureParameter.File(
                    id="file",
                    title="ファイル",
                    multi_selections=False,
                    open_type="file",
                    filters=[FileFilter("すべてのファイル", ["*"])],
                ),
                id="file",
                lua_name="file",
                lua_type=str,
                default="",
                convert=lambda v: str(v[0]) if v else "",
            )
        )

    # --dialog:表示名,変数名=初期値;表示名/chk,変数名=初期値;...
    def dialog(self, body: str) -> None:
        for item in _split_items(body):
            label, sep, assign = item.partition(",")
            name, sep2, expr = assign.partition("=")
            label, name, expr = label.strip(), name.strip(), expr.strip()
            if not sep or not sep2 or not name:
                self.warn(f"--dialog: '{item}' : invalid syntax")
                continue

            id = f"dialog.{name}"
            if label.endswith("/chk"):
                # AviUtl と同じく、チェックボックスの値は Lua 上では 0/1 の数値
                default = expr not in ("", "0", "nil", "false")
                self.add(
                    ScriptParameter(
                        ui=RequestStructureParameter.Bool(id=id, title=label[:-4], default_value=default),
                        id=id,
                        lua_name=name,
                        lua_type=int,
                        default=int(default),
                        convert=lambda v: 1 if v else 0,
                    )
                )
            elif label.endswith("/col"):
                self.add(_color_parameter(id, label[:-4], name, self.parse_color(expr, f"--dialog({name})")))
            elif label.endswith("/fig"):
                # TODO: 図形選択ボタンは未実装。今は初期値(図形名の文字列)をそのまま渡す。
                self.value(id, label[:-4], name, expr)
            else:
                self.value(id, label, name, expr)

    def value(self, id: str, title: str, lua_name: str, expr: str) -> None:
        """リテラルの初期値から型を推定してパラメータを追加する。
        リテラルでない(テーブルや式など)場合は UI に出さず、代入文をスクリプトの前に実行する。"""
        literal = _parse_literal(expr)
        if literal is None:
            if lua_name in self._lua_names:
                self.warn(f"'{lua_name}' is already defined. Skipping.")
                return
            self.warn(f"'{lua_name}': cannnot determine type from '{expr}'. This value will be assigned in the prelude, and cannot be edited in the UI.")
            self._lua_names.add(lua_name)
            self.result.prelude.append(f"{lua_name} = {expr or 'nil'}")
            return

        ui: RequestStructureParameter
        lua_type: LuaType
        convert: Callable[[Any], LuaValue]
        match literal:
            case bool():
                ui, lua_type, convert = RequestStructureParameter.Bool(id=id, title=title, default_value=literal), bool, bool
            case int():
                ui, lua_type, convert = RequestStructureParameter.Int(id=id, title=title, default_value=literal), int, int
            case float():
                ui, lua_type, convert = RequestStructureParameter.Float(id=id, title=title, default_value=literal), float, float
            case str():
                ui, lua_type, convert = RequestStructureParameter.String(id=id, title=title, default_value=literal), str, str
        self.add(ScriptParameter(ui=ui, id=id, lua_name=lua_name, lua_type=lua_type, default=literal, convert=convert))

    def parse_color(self, text: str, where: str) -> int:
        try:
            value = int(text, 16) if text.lower().startswith("0x") else int(text, 10)
        except ValueError:
            self.warn(f"{where}: color '{text}' must be specified as a decimal or 0x-prefixed hexadecimal. Using 0x{DEFAULT_COLOR:06x} instead.")
            return DEFAULT_COLOR
        if not 0 <= value <= 0xFFFFFF:
            self.warn(f"{where}: color '{text}' is out of range. Using the lower 24 bits.")
        return value & 0xFFFFFF


def _color_parameter(id: str, title: str, lua_name: str, default: int) -> ScriptParameter:
    """色選択ボタン。Lua 上では 0xRRGGBB の数値。"""
    rgba = ((default >> 16 & 0xFF) / 255, (default >> 8 & 0xFF) / 255, (default & 0xFF) / 255, 1.0)
    return ScriptParameter(
        ui=RequestStructureParameter.Color(id=id, title=title, default_value=rgba, use_alpha=False),
        id=id,
        lua_name=lua_name,
        lua_type=int,
        default=default,
        convert=_rgba_to_int,
    )


def _rgba_to_int(rgba: Any) -> int:
    r, g, b = (max(0, min(255, round(float(c) * 255))) for c in rgba[:3])
    return r << 16 | g << 8 | b


def _parse_literal(expr: str) -> LuaValue | None:
    """Lua の単純なリテラル(数値・文字列・真偽値)を Python の値にする。それ以外は None。"""
    if expr in ("true", "false"):
        return expr == "true"
    if _NUMBER_LITERAL.fullmatch(expr):
        if expr.lstrip("+-")[:2].lower() == "0x":
            return int(expr, 16)
        if _INT_LITERAL.fullmatch(expr):
            return int(expr)
        return float(expr)
    if len(expr) >= 2 and expr[0] == expr[-1] and expr[0] in "\"'" and expr[0] not in expr[1:-1]:
        return expr[1:-1]
    return None


def _split_items(body: str) -> list[str]:
    """';' で区切る。文字列リテラルや括弧の中の ';' では区切らない。空の項目は捨てる。"""
    items: list[str] = []
    current: list[str] = []
    quote: str | None = None
    depth = 0
    for ch in body:
        if quote is not None:
            if ch == quote:
                quote = None
        elif ch in "\"'":
            quote = ch
        elif ch in "{([":
            depth += 1
        elif ch in "})]":
            depth = max(0, depth - 1)
        elif ch == ";" and depth == 0:
            items.append("".join(current))
            current = []
            continue
        current.append(ch)
    items.append("".join(current))
    return [item.strip() for item in items if item.strip()]
