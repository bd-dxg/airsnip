# SPDX-License-Identifier: GPL-3.0-only
# 用户可见硬编码文案扫描（I18N-4，issue #151 / ADR 0047）。
#
# 目的：抽完 i18n 三个月后别再长回来。做法是「已迁移范围报错、未迁移范围只统计」——
# 随批次推进，把新迁完的路径加进 $MIGRATED，脚本就自动变成那一块的守门器。
#
# 判定规则（启发式，宁可漏报也不误报到没人看）：
#   - 只认**字符串字面量里**的 CJK 字符（中日文），注释里的中文不算；
#   - 跳过整个 `#[cfg(test)]` + `mod tests` 块（测试断言不翻译）；
#   - 跳过日志（eprintln! / println! / dbg!）与断言（assert / expect / panic）；
#   - 行尾写 `// i18n:keep` 的视为有意保留（技术标识、外部原文等）。
#
# 已知局限：不支持跨行字符串与 raw string 的完整解析，按行匹配。
# 用法：pwsh -File scripts/check-i18n.ps1 [-Verbose]

param([switch]$Verbose)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# 已迁移完毕、应当零硬编码的路径（前缀匹配，小写比较）。B1 设置面板 + B2 覆盖层/贴图。
$MIGRATED = @(
    'src\settings\',
    'src\update\state.rs',
    'src\output\quick.rs',
    'src\overlay\toolbar\tip.rs',
    'src\overlay\cancel_hint.rs',
    'src\overlay\toast.rs',
    'src\overlay\quicksave.rs',
    'src\overlay\setup.rs',
    'src\overlay\pin_edit.rs',
    'src\overlay\annotate\output.rs',
    'src\pin\menu.rs',
    'src\pin\mod.rs',
    'src\pin\window.rs',
    'src\pin\edit.rs',
    'src\pin\clipboard.rs',
    'src\i18n\'
)

$cjk = '[\u3400-\u4dbf\u4e00-\u9fff\u3040-\u30ff]'
$literal = '"[^"]*' + $cjk + '[^"]*"'

function Test-Skippable([string]$line) {
    if ($line -match '^\s*//') { return $true }
    if ($line -match 'eprintln!|println!|dbg!|panic!|assert!|assert_eq!|assert_ne!|expect\(|unwrap_err') {
        return $true
    }
    if ($line -match '//\s*i18n:keep') { return $true }
    return $false
}

function Test-Migrated([string]$rel) {
    $lower = $rel.ToLowerInvariant()
    foreach ($prefix in $MIGRATED) {
        if ($lower.StartsWith($prefix.ToLowerInvariant())) { return $true }
    }
    return $false
}

$problems = New-Object System.Collections.Generic.List[string]
$pendingHits = New-Object System.Collections.Generic.List[string]
$files = Get-ChildItem -Path src -Recurse -Filter *.rs

foreach ($file in $files) {
    $rel = $file.FullName.Substring($root.Length + 1)
    # 语言表本身就是文案，不参与扫描
    if ($rel.ToLowerInvariant().StartsWith('src\i18n\')) { continue }
    # 纯测试文件（`render_tests.rs` 这类）整份跳过：断言的说明文字不翻译
    if ($file.Name -match '(_tests|tests)\.rs$') { continue }
    # 纯测试文件（
    if ($file.Name -match '(_tests|tests)\.rs$') { continue }
    $lines = Get-Content -LiteralPath $file.FullName -Encoding UTF8
    $skipRest = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $line = $lines[$i]
        # `#[cfg(test)]` 后面紧跟 `mod tests` → 文件剩余部分是测试模块
        if ($line -match '#\[cfg\(test\)\]') {
            for ($j = $i + 1; $j -lt [Math]::Min($i + 4, $lines.Count); $j++) {
                if ($lines[$j] -match '^\s*mod\s+tests') { $skipRest = $true; break }
                if ($lines[$j].Trim() -ne '') { break }
            }
        }
        if ($skipRest) { break }
        if (Test-Skippable $line) { continue }

        # 去掉行内注释后再找字面量（`//` 之后不再看；含 `//` 的字符串会被截断，
        # 但那类字符串不含 CJK，不影响判定）
        $code = $line
        $slash = $line.IndexOf('//')
        if ($slash -ge 0) { $code = $line.Substring(0, $slash) }

        foreach ($hit in [regex]::Matches($code, $literal)) {
            $msg = "{0}:{1}: {2}" -f $rel, ($i + 1), $hit.Value
            if (Test-Migrated $rel) { $problems.Add($msg) } else { $pendingHits.Add($msg) }
        }
    }
}

if ($Verbose) {
    Write-Host ""
    Write-Host "未迁移批次的硬编码（只统计，不失败）：" -ForegroundColor DarkGray
    $pendingHits | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
}

Write-Host ("已迁移范围内剩余硬编码：{0}" -f $problems.Count)
Write-Host ("未迁移批次范围内硬编码：{0}（B2 覆盖层 / B3 长截图 / B4 app 系统串分批处理）" -f $pendingHits.Count)

if ($problems.Count -gt 0) {
    Write-Host ""
    Write-Host "以下文案必须移入 src/i18n 语言表（或行尾加 // i18n:keep 并说明理由）：" -ForegroundColor Red
    $problems | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    exit 1
}
Write-Host "OK：已迁移范围内没有用户可见硬编码。" -ForegroundColor Green
exit 0
