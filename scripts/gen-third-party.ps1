# 生成 THIRD-PARTY.md（第三方组件声明）
# 用法：pwsh -File scripts/gen-third-party.ps1（需 PowerShell 7；Windows PowerShell 5.1 会把无 BOM 的中文读成乱码）
# 依赖变更后重跑，并在 PR 中一并提交；许可证白名单见根目录 deny.toml。
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$meta = Join-Path $env:TEMP "airsnip-meta.json"
cargo metadata --format-version 1 --locked | Out-File -Encoding utf8 $meta
$m = Get-Content $meta -Raw | ConvertFrom-Json
$pkgs = $m.packages | Where-Object { $_.source -ne $null } | Sort-Object name

$sb = [System.Text.StringBuilder]::new()
[void]$sb.AppendLine("# 第三方组件声明")
[void]$sb.AppendLine()
[void]$sb.AppendLine("AirSnip 自身以 **GPL-3.0-only** 发布。以下是随程序一同分发的第三方开源组件（共 $($pkgs.Count) 个 crate，均由 crates.io 获取）。")
[void]$sb.AppendLine()
[void]$sb.AppendLine("所有组件均使用宽松许可证，与 GPL-3.0-only 兼容（无 GPL-2.0-only 等不兼容条款）。清单包含运行期与构建期依赖（如 ``embed-resource``）。")
[void]$sb.AppendLine()
[void]$sb.AppendLine("自动校验：仓库根目录的 ``deny.toml`` + ``cargo deny check``（许可证 / 安全公告 / 来源 / 重复版本），CI 与发布前均需通过。")
[void]$sb.AppendLine()
[void]$sb.AppendLine("## 按许可证分组")
[void]$sb.AppendLine()
foreach ($g in ($pkgs | Group-Object license | Sort-Object Name)) {
    [void]$sb.AppendLine("### $($g.Name)")
    [void]$sb.AppendLine()
    foreach ($p in $g.Group) { [void]$sb.AppendLine("- $($p.name) $($p.version)") }
    [void]$sb.AppendLine()
}
[void]$sb.AppendLine("## 许可证全文")
[void]$sb.AppendLine()
foreach ($lic in @(
        @("MIT", "https://opensource.org/license/mit"),
        @("Apache-2.0", "https://www.apache.org/licenses/LICENSE-2.0"),
        @("BSD-3-Clause", "https://opensource.org/license/bsd-3-clause"),
        @("BSD-2-Clause", "https://opensource.org/license/bsd-2-clause"),
        @("Zlib", "https://opensource.org/license/zlib"),
        @("0BSD", "https://opensource.org/license/0bsd"),
        @("Unlicense", "https://unlicense.org/"),
        @("Unicode-3.0", "https://www.unicode.org/license.txt"),
        @("IJG", "https://www.ijg.org/"),
        @("ISC", "https://opensource.org/license/isc-license-txt")
    )) {
    [void]$sb.AppendLine("- $($lic[0]): $($lic[1])")
}

$out = Join-Path $root "THIRD-PARTY.md"
# 统一为 LF（.gitattributes 要求），避免 CRLF 干扰
$text = $sb.ToString() -replace "`r`n", "`n"
[System.IO.File]::WriteAllText($out, $text, [System.Text.UTF8Encoding]::new($false))
Write-Host "已写入 $out（$($pkgs.Count) 个 crate）"
