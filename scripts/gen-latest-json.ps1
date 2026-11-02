# 生成并签名自动更新清单 latest.json（SYS-7）
# 用法：pwsh -File scripts/gen-latest-json.ps1 -Notes "本次更新说明"
#       先跑 scripts/package.ps1 产出 dist\airsnip-v{版本}-win-x64.exe
#
# 依赖：私钥 %USERPROFILE%\.airsnip\update.key（由 cargo run --example update-keygen 生成）
# 产物：dist\latest.json（与 zip / exe 一起上传到同一个 Release）
param([string]$Notes = "")
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$m = Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1
if (-not $m) { throw "无法从 Cargo.toml 读取版本号" }
$ver = $m.Matches[0].Groups[1].Value
$asset = "airsnip-v$ver-win-x64.exe"
$exe = Join-Path $root "dist\$asset"
if (-not (Test-Path $exe)) { throw "找不到 $exe，请先运行 scripts/package.ps1" }

$repo = "bd-dxg/airsnip"
$sha = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
Write-Host "版本：$ver"
Write-Host "安装包：$asset"
Write-Host "SHA256：$sha"

# 签名：与客户端 Manifest::payload() 的载荷格式保持一致（标签 + 版本 + 小写摘要）
Write-Host "调用 update-sign 生成签名 ..."
$sig = (cargo run --quiet --example update-sign -- --version $ver --sha256 $sha 2>&1 | Select-Object -Last 1)
if ($LASTEXITCODE -ne 0 -or -not $sig -or $sig.Length -ne 128) {
    throw "签名失败（检查私钥 %USERPROFILE%\.airsnip\update.key 是否存在）：$sig"
}
Write-Host "签名：$($sig.Substring(0,16))...(共 128 字符)"

# 清单里的 url 用 GitHub 规范地址：加速前缀由客户端按可用镜像套用，
# 换镜像域名时不需要重新发版（见 ADR 0018）。
$url = "https://github.com/$repo/releases/download/v$ver/$asset"
$pubDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")

$manifest = [ordered]@{
    version   = $ver
    pub_date  = $pubDate
    notes     = $Notes
    url       = $url
    sha256    = $sha
    signature = $sig.Trim()
}
$out = Join-Path $root "dist\latest.json"
$manifest | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 $out

# 自检：四项一致（清单版本 / 签名绑定版本 / 包文件名 / Cargo.toml 版本）
if ($manifest.version -ne $ver) { throw "清单版本与 Cargo.toml 不一致" }
if (-not $manifest.url.EndsWith($asset)) { throw "清单 url 与包文件名不一致：$($manifest.url)" }
if ($manifest.signature.Length -ne 128) { throw "签名长度异常：$($manifest.signature.Length)" }

Write-Host ""
Write-Host "产物：dist\latest.json"
Write-Host "下一步：gh release upload v$ver dist\latest.json dist\$asset --clobber"
