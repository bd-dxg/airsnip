# 打包发布（M5）：构建 release → 校验图标/体积 → 输出 dist\airsnip-v{版本}-win-x64.zip
# 用法：pwsh -File scripts/package.ps1            # 完整构建 + 打包
#       pwsh -File scripts/package.ps1 -SkipBuild  # 复用已有 release 产物
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$m = Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1
if (-not $m) { throw "无法从 Cargo.toml 读取版本号" }
$ver = $m.Matches[0].Groups[1].Value
$name = "airsnip-v$ver-win-x64"
Write-Host "版本：$ver"

if (-not $SkipBuild) {
    Write-Host "构建 release ..."
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release 失败" }
}

$exe = Join-Path $root "target\release\airsnip.exe"
if (-not (Test-Path $exe)) { throw "找不到 $exe" }

# 体积红线（PRD：≤5 MB）
$sizeMb = (Get-Item $exe).Length / 1MB
$verdict = if ($sizeMb -le 5) { "达标" } else { "超标" }
Write-Host ("release 体积：{0:N2} MB（红线 ≤5 MB，{1}）" -f $sizeMb, $verdict)

# 图标校验：资源段里嵌入的图标必须能取出来
Add-Type -AssemblyName System.Drawing
$icon = [System.Drawing.Icon]::ExtractAssociatedIcon($exe)
if (-not $icon) { throw "exe 未嵌入图标（app.rc 的 ICON 资源缺失？）" }
Write-Host ("嵌入图标：{0}×{1}" -f $icon.Width, $icon.Height)

# 组装发布目录
$dist = Join-Path $root "dist"
$pkg = Join-Path $dist $name
if (Test-Path $pkg) { Remove-Item $pkg -Recurse -Force }
New-Item -ItemType Directory -Path $pkg -Force | Out-Null

Copy-Item $exe $pkg
foreach ($f in @("LICENSE", "THIRD-PARTY.md", "README.md")) {
    if (-not (Test-Path $f)) { throw "缺少发布必需文件：$f" }
    Copy-Item $f $pkg
}

$zip = Join-Path $dist "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $pkg "*") -DestinationPath $zip -CompressionLevel Optimal

$zipMb = (Get-Item $zip).Length / 1MB
$sha = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
Write-Host ""
Write-Host "产物：dist\$name.zip"
Write-Host ("体积：{0:N2} MB" -f $zipMb)
Write-Host "SHA256：$sha"
Write-Host ""
Write-Host "下一步：gh release create v$ver `"dist\$name.zip`" --title `"AirSnip v$ver`" --notes-file <说明文件>"
