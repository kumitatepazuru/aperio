"""Lua (LuaJIT) → CPU Lua + Slang compute kernels for Aperio.

```python
from lua_gpu import LuaGPU, CpuFunction, GpuFunction, PyFunction, Ext, Gen

gpu = LuaGPU([
    CpuFunction("obj.w", lambda c: "${w}", property=True, returns=["int"], externals=[Ext("w", "int")]),
    GpuFunction("obj.getpixel", gen_getpixel, params=["float", "float"], returns=["float"] * 4),
])
res = gpu.convert(source)              # ConvertResult | None (see gpu.last_diagnostics)
for k, params in res({"w": 1920}):     # dispatches in execution order
    builder = builder.add_shader(compiled[k], params, w, h)   # compiled[k] from res.shaders[k]
```
"""

from collections.abc import Callable, Mapping, Sequence
from os import PathLike
from typing import Any, Final, Literal, TypedDict, final

__version__: Final[str]

class LuaGpuError(Exception):
    """Base class of lua_gpu errors."""

class LuaGpuRuntimeError(LuaGpuError):
    """The converted Lua failed at run time (Lua error, bad params, inner loop limit, ...)."""

class LuaGpuArgError(LuaGpuError):
    """Invalid or missing external values were passed to run()."""

@final
class Gen:
    """Return value of a generator. A generator may also return a single `str` (one expression),
    a `list[str]` (one expression per return value) or `None` (not convertible -> conversion error)."""

    exprs: list[str]
    """One expression per return value."""
    stmts: str
    """Statements placed before the expressions."""
    defs: str
    """Top-level definitions (Slang functions/structs). Identical text is emitted once."""
    def __new__(cls, /, exprs: Sequence[str] | None = None, stmts: str = "", defs: str = "") -> Gen: ...

@final
class CallSite:
    """Information about one call site, passed to a generator (called once per call site, in source order)."""

    @property
    def name(self) -> str: ...
    @property
    def args(self) -> list[str]:
        """Argument expressions in the target language (parameter names `a1`, `a2`, ...)."""
    @property
    def consts(self) -> list[bool | float | str | None]:
        """Argument values known at convert time (None when not constant)."""
    @property
    def types(self) -> list[str]: ...
    @property
    def is_method(self) -> bool:
        """Called as `obj:name(...)` (self is not passed)."""
    @property
    def chunk(self) -> str: ...
    @property
    def line(self) -> int: ...
    @property
    def col(self) -> int: ...
    @property
    def context(self) -> Literal["cpu", "kernel"]:
        """"cpu": return Lua code. "kernel": return Slang code."""

@final
class Ext:
    """An external value (passed on every run). Reference it as `${name}` in generated code."""

    name: str
    type: str
    def __new__(cls, /, name: str, type: str) -> Ext: ...

Generator = Callable[[CallSite], str | Gen | Sequence[str] | None]

@final
class CpuFunction:
    """Called at convert time; returns **Lua** code for each call site."""

    def __new__(
        cls,
        /,
        name: str,
        generate: Generator,
        *,
        returns: Sequence[str] | None = None,
        externals: Sequence[Ext] | None = None,
        property: bool = False,
    ) -> CpuFunction: ...

@final
class GpuFunction:
    """Called at convert time; returns **Slang** code for each call site inside a GPU kernel.

    The Slang code can use `inputs.tex0` (+ `extra_inputs`), `res.outputTex` and `res.samp`."""

    def __new__(
        cls,
        /,
        name: str,
        generate: Generator,
        *,
        params: Sequence[str] | None = None,
        returns: Sequence[str] | None = None,
        externals: Sequence[Ext] | None = None,
        property: bool = False,
    ) -> GpuFunction: ...

@final
class PyFunction:
    """A host function called directly from Lua at run time (CPU only).

    Lua values are passed as None / bool / float / str / list / dict; a returned tuple is
    returned to Lua as multiple values."""

    def __new__(
        cls, /, name: str, func: Callable[..., Any], *, returns: Sequence[str] | None = None
    ) -> PyFunction: ...

class Diagnostic(TypedDict):
    severity: Literal["error", "warning"]
    code: str
    message: str
    chunk: str
    line: int
    col: int

Dispatch = tuple[int, bytes]

@final
class ConvertResult:
    @property
    def lua(self) -> str:
        """CPU Lua (layout header on line 1, bundled modules)."""
    @property
    def shaders(self) -> list[str]:
        """Slang source per kernel (index = kernel index of a dispatch)."""
    @property
    def externals(self) -> list[tuple[str, str]]: ...
    def __call__(
        self,
        externals: Mapping[str, Any] | None = None,
        on_dispatch: Callable[[int, bytes], object] | None = None,
    ) -> list[Dispatch]: ...

@final
class LuaGPU:
    def __new__(
        cls,
        /,
        functions: Sequence[CpuFunction | GpuFunction | PyFunction],
        *,
        package_path: str | None = None,
        max_loop_iterations: int = 1_000_000,
        max_values_per_iteration: int = 16,
        max_local_table_len: int = 64,
        extra_inputs: Sequence[str] | None = None,
    ) -> LuaGPU: ...
    def convert(
        self, source: str, chunk_name: str = "main.lua", base_dir: str | PathLike[str] | None = None
    ) -> ConvertResult | None: ...
    def run(
        self,
        lua: str,
        externals: Mapping[str, Any] | None = None,
        on_dispatch: Callable[[int, bytes], object] | None = None,
    ) -> list[Dispatch]:
        """Run CPU Lua; returns the dispatches in execution order (empty when on_dispatch is given)."""
    @property
    def last_diagnostics(self) -> list[Diagnostic]: ...
    @property
    def externals(self) -> list[tuple[str, str]]: ...
