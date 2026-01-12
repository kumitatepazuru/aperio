param(
    [string]$Flag
)

# dist ディレクトリ作成
New-Item -ItemType Directory -Force -Path "dist" | Out-Null

function Invoke-Python {
    param(
        [string]$Code
    )

    if ($Flag -eq "--uv") {
        # uv 経由で python 実行
        & uv run python -c $Code
    } else {
        & python -c $Code
    }
}

# Python から最低限の情報だけ取得:
# 1行目: sys.executable
# 2行目: major
# 3行目: minor
$pyOutput = Invoke-Python "import sys;import sysconfig; print(sysconfig.get_config_var('LIBDIR')); print(sys.version_info.major); print(sys.version_info.minor)"

if ($LASTEXITCODE -ne 0 -or -not $pyOutput) {
    Write-Host "Failed to run Python to get version info."
    exit 1
}

# 複数行として扱う
$lines = @($pyOutput)
if ($lines.Count -lt 3) {
    Write-Host "Unexpected Python output: $pyOutput"
    exit 1
}

$exePath = $lines[0].Trim()
$major   = [int]$lines[1]
$minor   = [int]$lines[2]

$dllName = "python{0}{1}.dll" -f $major, $minor

Write-Host "Python executable: $exePath"
Write-Host "Python version   : $major.$minor"
Write-Host "DLL name         : $dllName"

# exe があるディレクトリ
$exeDir   = Split-Path -Parent $exePath
$parent   = Split-Path -Parent $exeDir
$libsDir  = Join-Path $exeDir   "libs"
$libsDir2 = Join-Path $parent   "libs"

# 探索候補ディレクトリ
$candidateDirs = @(
    $exeDir,
    $parent,
    $libsDir,
    $libsDir2
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique

$dllPath = $null

foreach ($dir in $candidateDirs) {
    $candidate = Join-Path $dir $dllName
    if (Test-Path $candidate) {
        $dllPath = (Resolve-Path $candidate).Path
        break
    }
}

if (-not $dllPath) {
    Write-Host "Error: $dllName not found in:"
    $candidateDirs | ForEach-Object { Write-Host "  $_" }
    exit 1
}

$dllDir = Split-Path -Parent $dllPath

Write-Host "Python shared library directory: $dllDir"
Write-Host "Listing $dllDir :"
Get-ChildItem -Force $dllDir

Write-Host "Python shared library: $dllPath"

# dist にコピー
Copy-Item -Verbose -Path $dllPath -Destination "dist"

# 追加: python3.dll も同じディレクトリにあればコピー
$python3Dll = Join-Path $dllDir "python3.dll"
if (Test-Path $python3Dll) {
    Write-Host "Found python3.dll: $python3Dll"
    Copy-Item -Verbose -Path $python3Dll -Destination "dist"
} else {
    Write-Host "python3.dll not found in $dllDir"
}
