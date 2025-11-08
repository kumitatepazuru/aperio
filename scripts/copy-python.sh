#!/bin/bash

# distディレクトリを作成
mkdir -p dist

# pythonの共有ライブラリのパスを取得
# --uvがつけられたらpythonをrunするときにuv runをつけるようにする
if [ "$1" = "--uv" ]; then
  PYTHON_CMD="uv run python"
else
  PYTHON_CMD="python3"
fi

PYTHON_PATH=$($PYTHON_CMD -c "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))")
PYTHON_LIB=$($PYTHON_CMD -c "import sys; print(f'libpython{sys.version_info.major}.{sys.version_info.minor}.so')")

if [ -z "$PYTHON_PATH" ]
then
  echo "Failed to get Python library path"
  exit 1
fi
echo "Python library path: $PYTHON_PATH"
echo "ls $PYTHON_PATH:"
ls -alh "$PYTHON_PATH"

PYTHON_LIB="$PYTHON_PATH/$PYTHON_LIB"

# ファイルがリンクの可能性があるので実体になるまでたどる
while [ -L "$PYTHON_LIB" ]; do
  LINK_TARGET=$(readlink "$PYTHON_LIB") # 例：libpython3.13.so.1.0 -> libpython3.13.so.1.0.1
  if [[ "$LINK_TARGET" = /* ]]; then # 絶対パスかどうか
    PYTHON_LIB="$LINK_TARGET"
  else
    PYTHON_LIB="$(dirname "$PYTHON_LIB")/$LINK_TARGET"
  fi
done
echo "Python shared library: $PYTHON_LIB"

# distにコピー
cp --verbose "$PYTHON_LIB" dist/