# 待机性能采集（M5）：Private Working Set 与 CPU 占用，对照 PRD §6 性能红线。
# 用法（程序已启动、托盘常驻、不做任何操作）：
#   pwsh -File scripts/perf.ps1
#   pwsh -File scripts/perf.ps1 -Seconds 10
param([int]$Seconds = 5)

$ErrorActionPreference = "Stop"
$p = Get-Process airsnip -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) {
    Write-Host "未找到 AirSnip 进程：请先启动程序（cargo run 或 release 版）" -ForegroundColor Yellow
    exit 1
}

Write-Host "进程：$($p.ProcessName) (PID $($p.Id))  采样 ${Seconds}s（期间请勿操作程序）"
$t1 = $p.TotalProcessorTime
Start-Sleep -Seconds $Seconds
$p.Refresh()
$t2 = $p.TotalProcessorTime

$cpuMs = ($t2 - $t1).TotalMilliseconds
$cpuPct = $cpuMs / ($Seconds * 1000) / [Environment]::ProcessorCount * 100

# Private Working Set 需性能计数器；不可用时回退到工作集
$privateWs = $null
try {
    $sample = (Get-Counter "\Process($($p.ProcessName))\Working Set - Private" -ErrorAction Stop).CounterSamples[0]
    $privateWs = [int64]$sample.CookedValue
} catch {
    Write-Host "（性能计数器不可用，回退为 WorkingSet64）" -ForegroundColor DarkGray
}

$ws = if ($privateWs) { $privateWs } else { $p.WorkingSet64 }
$peak = $p.PeakWorkingSet64

function Verdict([double]$value, [double]$limit, [string]$unit) {
    if ($value -le $limit) { "达标" } else { "超标" }
}

Write-Host ""
Write-Host ("Private Working Set : {0,8:N1} MB   （红线 ≤ 15 MB）{1}" -f ($ws / 1MB), (Verdict ($ws / 1MB) 15 ""))
Write-Host ("Peak Working Set    : {0,8:N1} MB" -f ($peak / 1MB))
Write-Host ("待机 CPU            : {0,8:N2} %    （红线 0%）{1}" -f $cpuPct, (Verdict $cpuPct 0.05 ""))
Write-Host ("CPU 时间增量        : {0,8:N1} ms / ${Seconds}s" -f $cpuMs)
Write-Host ""
Write-Host "说明：待机 CPU 红线为「无轮询、无定时器」；采样期内任何 >0 的持续占用都需排查。"
