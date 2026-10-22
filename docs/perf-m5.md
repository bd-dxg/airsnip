# M5 性能剖析报告

对照 PRD §6 性能红线，在真实目标机型（无独显）上实测并优化。

## 测试环境

| 项 | 值 |
|---|---|
| 屏幕 | 2560×1440 @ 100% DPI（单屏，3.69 MP） |
| GPU | 无独显，仅 CPU 核显（渲染全程 CPU） |
| 系统 | Windows 11 |
| 构建 | `cargo build --release`（`lto="fat"`、`opt-level="z"`、`strip=true`） |
| 二进制体积 | 0.98 MB（目标 ≤5 MB） |

## 测量方法

两个脚本（均在 `scripts/`）：

- `perf-capture.ps1`：启动程序 → 模拟 `WM_HOTKEY(101)` 触发 5 次截图 → 模拟框选与 20 次鼠标移动（产生多帧脏区重绘）→ `Esc` 退出 → 解析日志。全程无人工干预。
- `perf.ps1`：采样待机 CPU（`TotalProcessorTime` 增量）与 Private Working Set（性能计数器）。

埋点在 `src/perf.rs`，仅在 `JIETU_TIMING=1` 时记录，常态零开销（一次 `OnceLock` 读）。

## 结果

### 优化前后对比（release，5 轮平均）

| 指标 | 优化前 | 优化后 | 红线 | 判定 |
|---|---|---|---|---|
| 热键 → 覆盖层可见 | 134.9 ms（最大 176.7） | **123.9 ms**（冷启动 235.9） | ≤150 ms（1080p）/ ≤250 ms（4K） | ✅ |
| 交互每帧重绘（平均） | 6.8–8.7 ms | **4.6–9.1 ms** | ≤16.7 ms（60 FPS） | ✅ |
| 交互每帧重绘（最大，真实拖动 90 帧） | 17.6 ms | **12.6 ms** | ≤16.7 ms | ✅ |
| 待机 CPU | 0.00 % | **0.00 %** | 0 % | ✅ |
| 待机 Private Working Set | 1.2 MB | **1.2 MB** | ≤15 MB | ✅ |
| Peak Working Set | 11.6 MB | 11.6 MB | — | — |

当前屏幕为 1440p（3.69 MP），介于 1080p（2.07 MP）与 4K（8.29 MP）之间，按像素数插值红线约 **176 ms**，实测 124 ms 留有余量。

### 稳态耗时分解（release，单次截图）

| 阶段 | 耗时 | 说明 |
|---|---|---|
| `BitBlt`（DWM 表面 → DIB） | 49.6 ms | 14.7 MB @ ~296 MB/s，**不可优化项** |
| 拷贝 + R/B 交换（多线程） | 4.4 ms | DIB → `Vec` 并转 RGBA |
| 建窗 + 初始化 | 10.0 ms | 两次 14.7 MB clone（2.9 × 2）+ 暗化（1.75） |
| `SetWindowPos` 显示 | 5 ms | 冷启动首次 105 ms（DWM 分配全屏表面） |
| 首帧重绘 | 4.6 ms | 底图 + 工具栏 + 上屏 |
| `SetForegroundWindow` + `SetFocus` | 0.04 ms | 改为消息循环异步执行后 |

## 已做的优化

1. **首帧跳过冗余全屏回贴**：`display` 初始内容本就等于 `dimmed`，首帧的 `restore_region` 是纯浪费（一次 14.7 MB 拷贝）。首帧 11.7 ms → 4.6 ms。
2. **`SetForegroundWindow` 异步化**：窗口刚显示时同步调用会被前台锁定阻塞 ~45 ms，直接卡住「画面出现」时刻。改为 `PostMessage(WM_FOCUS_OVERLAY)` 在消息循环里执行。前台+焦点 45 ms → 0.04 ms，热键到可见省 39 ms。
3. **埋点体系**：`src/perf.rs` 提供热键→可见、每帧重绘（RAII 守卫，覆盖所有 return 路径）、覆盖层退出汇总；`capture` 与 `overlay` 内各有分段计时。

## 未做的优化（记录原因）

| 方向 | 结论 |
|---|---|
| 去掉 `CAPTUREBLT` | 实测更慢（60.7 ms vs 50.9 ms），且会漏抓分层窗口。已恢复 |
| DXGI Desktop Duplication 替代 BitBlt | 唯一可能突破 50 ms 的路径（GPU 纹理回读），需引入 D3D11/DXGI 依赖与设备管理，工作量远超 M5 打磨范围 |
| 启动时预热覆盖层窗口 | 可消除冷启动 105 ms 的 `SetWindowPos`，但预热需显示窗口（用户会看到闪一下），代价大于收益 |
| 减少 `Pixmap` 副本 | `original` / `dimmed` / `display` 三份是语义必需（底图、预生成暗图、可写显示缓冲），合并会牺牲脏区重绘的 O(局部) 特性 |

## 复现

```powershell
# 管理员 PowerShell（release 版 manifest 要求提权）
$env:JIETU_TIMING = "1"
.\scripts\perf-capture.ps1 -Rounds 5     # 热键延迟 + 帧耗时
.\scripts\perf.ps1 -Seconds 10           # 待机 CPU / 内存（需程序在运行）
```
