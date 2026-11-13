# 打包发布（M5）：构建 release → 校验图标/体积 → 输出 dist\airsnip-v{版本}-win-x64.zip
# 用法：pwsh -File scripts/package.ps1                    # 完整构建 + 打包
#       pwsh -File scripts/package.ps1 -SkipBuild        # 复用已有 release 产物
#       pwsh -File scripts/package.ps1 -Suffix 20261113  # 加后缀区分测试包（不影响版本号）
param([switch]$SkipBuild, [string]$Suffix = "")

$ErrorActionPreference = 'Stop'

# 读取 PE 导入表（仅支持 PE32+ / x64），返回 exe 直接依赖的 DLL 名。
# 用途：打包前守门，确认 exe 没有动态依赖 VC 运行时（ADR 0021）。
function Get-PeImportedDlls([string]$Path) {
    $fs = [IO.File]::OpenRead($Path)
    $br = New-Object IO.BinaryReader($fs)
    try {
        if ($br.ReadUInt16() -ne 0x5A4D) { throw "不是 PE 文件：$Path" }
        $fs.Position = 0x3C
        $fs.Position = $br.ReadUInt32()
        if ($br.ReadUInt32() -ne 0x00004550) { throw "PE 签名无效：$Path" }
        $null = $br.ReadUInt16()                    # Machine
        $sectionCount = $br.ReadUInt16()
        $null = $br.ReadUInt32(); $null = $br.ReadUInt32(); $null = $br.ReadUInt32()
        $optSize = $br.ReadUInt16()
        $null = $br.ReadUInt16()                    # Characteristics

        $optStart = $fs.Position
        if ($br.ReadUInt16() -ne 0x20B) { throw "只支持 PE32+（x64）：$Path" }
        $fs.Position = $optStart + 112 + 8          # DataDirectory[1] = Import Table
        $importRva = $br.ReadUInt32()
        $null = $br.ReadUInt32()                    # Import Table 字节数

        $fs.Position = $optStart + $optSize         # 节表紧随 OptionalHeader
        $sections = for ($i = 0; $i -lt $sectionCount; $i++) {
            $null = $br.ReadBytes(8)                # Name
            $vsize = $br.ReadUInt32(); $vaddr = $br.ReadUInt32()
            $rawSize = $br.ReadUInt32(); $rawPtr = $br.ReadUInt32()
            $null = $br.ReadBytes(16)
            [pscustomobject]@{ VAddr = $vaddr; VSize = $vsize; RawPtr = $rawPtr; RawSize = $rawSize }
        }

        # RVA → 文件偏移（导入表按节映射）
        $rvaToOffset = {
            param($rva)
            foreach ($s in $sections) {
                $span = [Math]::Max([int64]$s.VSize, [int64]$s.RawSize)
                if ($rva -ge $s.VAddr -and $rva -lt ($s.VAddr + $span)) {
                    return [int64]$s.RawPtr + ($rva - $s.VAddr)
                }
            }
            return [int64]-1
        }

        $names = @()
        $offset = & $rvaToOffset $importRva
        while ($offset -ge 0) {
            $fs.Position = $offset
            $desc = $br.ReadBytes(20)
            if ($desc.Length -lt 20) { break }
            $nameRva = [BitConverter]::ToUInt32($desc, 12)   # IMAGE_IMPORT_DESCRIPTOR.Name
            if ($nameRva -eq 0) { break }
            $nameOffset = & $rvaToOffset $nameRva
            if ($nameOffset -lt 0) { break }
            $fs.Position = $nameOffset
            $bytes = New-Object System.Collections.Generic.List[byte]
            while ($bytes.Count -lt 256) {
                $b = $br.ReadByte()
                if ($b -eq 0) { break }
                $bytes.Add($b)
            }
            $names += [Text.Encoding]::ASCII.GetString($bytes.ToArray())
            $offset += 20
        }
        return $names
    }
    finally {
        $br.Dispose()
        $fs.Dispose()
    }
}

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$m = Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1
if (-not $m) { throw "无法从 Cargo.toml 读取版本号" }
$ver = $m.Matches[0].Groups[1].Value
$name = "airsnip-v$ver-win-x64"
# 测试包用后缀区分（issue 验证包）：版本号仍是当前 Cargo.toml 的，不会被自动更新覆盖
if ($Suffix) { $name = "$name-$Suffix" }
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

# CRT 依赖守门（ADR 0021）：exe 必须静态链接 VC 运行时，否则干净机器上
# 双击会报「找不到 VCRUNTIME140.dll」。.cargo/config.toml 漏配或失效时在这里拦下。
$imports = @(Get-PeImportedDlls $exe)
Write-Host ("导入的 DLL：{0}" -f ($imports -join ', '))
$crtDeps = $imports | Where-Object { $_ -match '^(VCRUNTIME|MSVCP|CONCRT|api-ms-win-crt|ucrtbase|msvcrt)' }
if ($crtDeps) {
    throw "exe 仍动态依赖 VC 运行时（$($crtDeps -join ', ')）：检查 .cargo/config.toml 的 +crt-static 是否生效"
}

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

# 自动更新包（SYS-7）：裸 exe（自动更新只需替换 exe，不用解压 zip）
$updExe = Join-Path $dist "$name.exe"
Copy-Item $exe $updExe -Force

$zipMb = (Get-Item $zip).Length / 1MB
$sha = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
$exeSha = (Get-FileHash $updExe -Algorithm SHA256).Hash.ToLower()
Write-Host ""
Write-Host "产物：dist\$name.zip（手动下载）"
Write-Host "      dist\$name.exe（自动更新）"
Write-Host ("体积：{0:N2} MB" -f $zipMb)
Write-Host "zip SHA256：$sha"
Write-Host "exe SHA256：$exeSha"
Write-Host ""
Write-Host "下一步："
Write-Host "  1. gh release create v$ver `"dist\$name.zip`" `"dist\$name.exe`" --title `"AirSnip v$ver`" --notes-file <说明文件>"
Write-Host "  2. pwsh -File scripts/gen-latest-json.ps1   # 生成并签名 latest.json，再上传到同一 Release"
