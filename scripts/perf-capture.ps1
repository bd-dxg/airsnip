# 性能采集（M5）：启动 jietu → 模拟 F1 截图 + 拖框绘制多帧 → 解析 JIETU_TIMING 日志。
# 用法（需与目标程序同权限；release 版要管理员）：
#   pwsh -File scripts/perf-capture.ps1                     # 默认 release
#   pwsh -File scripts/perf-capture.ps1 -Exe .\target\debug\jietu.exe -Rounds 3
# 结果对照 PRD §6：热键→覆盖层 ≤150ms(1080p)/≤250ms(4K)，交互 ≥60FPS（每帧 ≤16.7ms）。
param(
    [string]$Exe = ".\target\release\jietu.exe",
    [int]$Rounds = 5,
    [string]$LogPath = "$env:TEMP\jietu-perf.log"
)
$ErrorActionPreference = 'Stop'

Add-Type -Namespace P -Name W -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, IntPtr win);
[DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
[DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
'@

$WM_HOTKEY = 0x0312
$WM_LBUTTONDOWN = 0x0201
$WM_MOUSEMOVE = 0x0200
$WM_LBUTTONUP = 0x0202
$WM_KEYDOWN = 0x0100
$VK_ESC = 0x1B

function LP([int]$x, [int]$y) { [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)) }
function Find([string]$cls) { [P.W]::FindWindowW($cls, [IntPtr]::Zero) }

function Wait-Window([string]$cls, [int]$timeoutMs = 3000) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ((Get-Date) -lt $deadline) {
        $h = Find $cls
        if ($h -ne [IntPtr]::Zero) { return $h }
        Start-Sleep -Milliseconds 10
    }
    return [IntPtr]::Zero
}

function Wait-Gone([string]$cls, [int]$timeoutMs = 3000) {
    $deadline = (Get-Date).AddMilliseconds($timeoutMs)
    while ((Get-Date) -lt $deadline) {
        if ((Find $cls) -eq [IntPtr]::Zero) { return $true }
        Start-Sleep -Milliseconds 10
    }
    return $false
}

if (-not (Test-Path $Exe)) { throw "找不到可执行文件：$Exe" }
$Exe = (Resolve-Path $Exe).Path
$running = Get-Process jietu -ErrorAction SilentlyContinue
if ($running) { throw "jietu 已在运行（PID $($running.Id)），请先退出再采集" }

$env:JIETU_TIMING = "1"
if (Test-Path $LogPath) { Remove-Item $LogPath -Force }
Write-Host "启动 $Exe（日志：$LogPath）"
$proc = Start-Process -FilePath $Exe -RedirectStandardError $LogPath -PassThru
Start-Sleep -Seconds 2

$hidden = Wait-Window "jietu.hidden" 5000
if ($hidden -eq [IntPtr]::Zero) { Stop-Process -Id $proc.Id -Force; throw "未找到隐藏主窗口 jietu.hidden" }

$sw = [P.W]::GetSystemMetrics(0)
$sh = [P.W]::GetSystemMetrics(1)
# 选区与矩形按屏幕比例计算，避免越界
$sx1 = [int]($sw * 0.12); $sy1 = [int]($sh * 0.15)
$sx2 = [int]($sw * 0.78); $sy2 = [int]($sh * 0.80)
$rx1 = [int]($sw * 0.25); $ry1 = [int]($sh * 0.30)
$rx2 = [int]($sw * 0.55); $ry2 = [int]($sh * 0.60)

for ($i = 1; $i -le $Rounds; $i++) {
    [void][P.W]::PostMessageW($hidden, $WM_HOTKEY, [IntPtr]101, [IntPtr]0)
    $ov = Wait-Window "jietu.overlay" 5000
    if ($ov -eq [IntPtr]::Zero) { Write-Warning "第 $i 轮：覆盖层未出现"; continue }
    Start-Sleep -Milliseconds 250

    # 框选
    [void][P.W]::PostMessageW($ov, $WM_LBUTTONDOWN, [IntPtr]1, (LP $sx1 $sy1))
    [void][P.W]::PostMessageW($ov, $WM_MOUSEMOVE, [IntPtr]1, (LP (($sx1 + $sx2) / 2) (($sy1 + $sy2) / 2)))
    [void][P.W]::PostMessageW($ov, $WM_MOUSEMOVE, [IntPtr]1, (LP $sx2 $sy2))
    [void][P.W]::PostMessageW($ov, $WM_LBUTTONUP, [IntPtr]0, (LP $sx2 $sy2))
    Start-Sleep -Milliseconds 100

    # 画矩形（多次 move 触发多帧脏区重绘）
    [void][P.W]::PostMessageW($ov, $WM_LBUTTONDOWN, [IntPtr]1, (LP $rx1 $ry1))
    for ($k = 1; $k -le 20; $k++) {
        $mx = $rx1 + [int](($rx2 - $rx1) * $k / 20)
        $my = $ry1 + [int](($ry2 - $ry1) * $k / 20)
        [void][P.W]::PostMessageW($ov, $WM_MOUSEMOVE, [IntPtr]1, (LP $mx $my))
        Start-Sleep -Milliseconds 8
    }
    [void][P.W]::PostMessageW($ov, $WM_LBUTTONUP, [IntPtr]0, (LP $rx2 $ry2))
    Start-Sleep -Milliseconds 100

    [void][P.W]::PostMessageW($ov, $WM_KEYDOWN, [IntPtr]$VK_ESC, [IntPtr]0)
    if (-not (Wait-Gone "jietu.overlay" 3000)) { Write-Warning "第 $i 轮：Esc 后覆盖层未关闭" }
    Start-Sleep -Milliseconds 200
}

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300
if (-not (Test-Path $LogPath)) { throw "没有生成日志：$LogPath" }
$lines = Get-Content $LogPath

# Duration 的 Debug 输出带单位（如 "845.2µs" / "12.3ms" / "1.2s"），统一换算为 ms
function Parse-Ms([string]$line) {
    if ($line -match '([\d.]+)(ns|µs|us|ms|s)') {
        $v = [double]$matches[1]
        switch ($matches[2]) {
            'ns' { return $v / 1e6 }
            'µs' { return $v / 1e3 }
            'us' { return $v / 1e3 }
            'ms' { return $v }
            's' { return $v * 1000 }
        }
    }
    return $null
}

$capture = @($lines | Select-String '抓屏耗时' | ForEach-Object { Parse-Ms $_.Line } | Where-Object { $_ -ne $null })
$shown = @($lines | Select-String '热键到覆盖层显示' | ForEach-Object { Parse-Ms $_.Line } | Where-Object { $_ -ne $null })
$frames = @($lines | Select-String '重绘 \d+ 帧：平均 ([\d.]+)ms，最大 ([\d.]+)ms')

function Avg([double[]]$v) { if ($v.Count -eq 0) { 0 } else { ($v | Measure-Object -Average).Average } }
function MaxOf([double[]]$v) { if ($v.Count -eq 0) { 0 } else { ($v | Measure-Object -Maximum).Maximum } }

Write-Host ""
Write-Host "==== 采集结果（$($shown.Count) 次截图）===="
Write-Host ("抓屏耗时            : 平均 {0,6:N1} ms" -f (Avg $capture))
Write-Host ("热键 → 覆盖层可见    : 平均 {0,6:N1} ms   最大 {1,6:N1} ms" -f (Avg $shown), (MaxOf $shown))
Write-Host ("帧统计              : {0} 条" -f $frames.Count)
foreach ($f in $frames) { Write-Host ("  " + $f.Line.Replace('[jietu] ', '')) }
Write-Host ""
Write-Host "红线：热键→覆盖层 ≤150ms（1080p）/ ≤250ms（4K）；交互 ≥60FPS = 每帧 ≤16.7ms"
Write-Host ""
Write-Host "待机测试需程序在运行：先启动程序，再跑 pwsh -File scripts/perf.ps1 -Seconds 10"
