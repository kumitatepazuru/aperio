-- 遅延プロシージャのキュー。LuaRuntime ごとに、初回の遅延プロシージャ登録時に 1 回だけ読み込む。
--
-- 1 呼び出し = [proc_id, 引数...] を aperio_lua_slot の配列に順に積み、スクリプト終了後に
-- Rust が state() で得たアドレスから配列を読む(Rust 側の Slot と同じレイアウト)。
--
-- Q.reserve(k): k スロット分を確保して (buf, 先頭インデックス) を返す。足りなければ倍の配列に作り直す。
-- Q.str(s):     文字列引数を strs に入れて GC から守り、そのインデックス(1 始まり)を返す。
--               同じ文字列は同じインデックスにまとめ、Rust 側で読む文字列の数を減らす。
-- Q.state():    (buf のアドレス, 使用スロット数, strs, strs の要素数) を返す。
-- Q.reset():    キューを空にする。

local ffi = require("ffi")
ffi.cdef("typedef union { double f; int64_t i; } aperio_lua_slot;")
local slot_array = ffi.typeof("aperio_lua_slot[?]")
local slot_size = ffi.sizeof("aperio_lua_slot")

local cap = 256
local buf = slot_array(cap)
local n = 0
local strs = {}
local str_index = {}
local nstr = 0

local Q = {}

function Q.reserve(k)
  local start = n
  local need = n + k
  if need > cap then
    local ncap = cap * 2
    while ncap < need do ncap = ncap * 2 end
    local nbuf = slot_array(ncap)
    ffi.copy(nbuf, buf, n * slot_size)
    buf, cap = nbuf, ncap
  end
  n = need
  return buf, start
end

function Q.str(s)
  local i = str_index[s]
  if i == nil then
    nstr = nstr + 1
    strs[nstr] = s
    str_index[s] = nstr
    i = nstr
  end
  return i
end

function Q.state()
  return tonumber(ffi.cast("uintptr_t", buf)), n, strs, nstr
end

function Q.reset()
  n = 0
  if nstr > 0 then
    strs = {}
    str_index = {}
    nstr = 0
  end
end

return Q
