# ADR 0024：HDR 桌面抓屏改走 DXGI Desktop Duplication

- 状态：已采纳（v0.8.0 开发期）
- 影响范围：新增 `src/capture/hdr.rs`、`hdr_display.rs`、`tonemap.rs`、`src/capture/mod.rs` 分流、
  `Cargo.toml`（4 个 windows features）
- 相关：issue #75、ADR 0001（纯 CPU 渲染）、ADR 0003（管理员权限 manifest）

## 背景

issue #75：HDR 显示器上截图出现过曝、亮部细节丢失、发白。报告者列出详细的对照实验
（Chrome/Edge × 哔哩哔哩/YouTube × SDR/HDR 片源），并确认「Win 官方自带截图工具也一样过曝」。

复现确认（本机：Windows 11、HDR 已开启、SDR 白点 280 nits、显示器峰值 456 nits）：

| 抓屏路径 | 平均亮度 | 纯白（255）像素占比 |
| --- | --- | --- |
| GDI `BitBlt`（现有 `capture`） | 142.1 | **23.37%** |
| DDA + 色调映射（本 ADR） | 94.3 | 0.06% |

整屏近四分之一像素爆成纯白——这就是用户看到的「整个区域一起过曝」。
`Win+Shift+S` 同为 GDI/DWM 8bit 路径，因此表现一致，说明**不是 AirSnip 独有缺陷**，
而是 Windows 传统抓屏在 HDR 桌面下的通病。

### 根因

HDR 开启后 DWM 的合成面是 **scRGB FP16**（线性，1.0 == 80 nits），HDR 高光可达数千 nits。
GDI 只能拿 8bit 降级帧，高光被压平截断。信息一旦截断不可逆，无法靠后处理还原。

## 决策

**HDR 激活时改走 DXGI Desktop Duplication，抓 `R16G16B16A16_FLOAT` 原始帧后色调映射回 SDR；
纯 SDR 环境仍走 `BitBlt`。**

分流点集中在 `capture::capture_virtual_screen`：`hdr::capture_virtual_screen()` 返回 `None`
表示「无需介入」，继续走原有 BitBlt 快路径。调用点（`app/mod.rs`、`longshot/session.rs`）无需改动。

### 色调映射策略

```
sRGB_out = encode( clamp( scRGB / (SDRWhiteLevel / 80), 0, 1 ) )
```

即除以 SDR 白点后硬截断，与 OBS 屏幕录制、`shuakami/ShareX-HDR` 的输出一致。
SDR 内容精确落回 `[0,1]` 不受影响，HDR 高光压到白。

**实机 A/B 选型**：同时生成三种映射供与原屏幕手机拍照比对——
`÷白点+clip`、`BT.2390 EETF → SDR 白点`、`BT.2390 EETF → 显示器峰值`。
用户确认 **`÷白点+clip` 与屏幕观感最接近**。BT.2390 在目标峰值取 SDR 白点时与 clip 数值相同
（本场景源峰值未越过阈值），取显示器峰值时整体偏暗，故不采用。
若后续出现高光细节不足的反馈，可在此处替换为 EETF（常数已在代码中留痕）。

## 关键坑（三条，均为实测踩出）

### 1. `DuplicateOutput1` 要求进程 Per-Monitor V2 DPI aware

不满足时对**所有**格式（含 `B8G8R8A8_UNORM`）恒返回 `DXGI_ERROR_UNSUPPORTED (0x887A0004)`，
而 v0 的 `DuplicateOutput` 正常——极易误判为「系统不支持 HDR 抓屏」而放弃。

主程序 manifest 已声明 Per-Monitor V2（CAP-5），因此运行时无需额外处理；
但**测试与 example 进程没有 manifest**，必须显式
`SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`，
否则 `hdr_capture_works` 会误报失败。

### 2. 同一 output 只能有一个 duplication 会话

旧会话未释放会让新调用返回 `E_ACCESSDENIED (0x80070005)`，并发创建甚至会返 `E_INVALIDARG`。
每次抓帧后显式 `ReleaseFrame`，会话随 `IDXGIOutputDuplication` 的 drop 释放。
**副作用：`hdr_*` 系列集成测试必须串行跑**（用静态 `Mutex` 守门），否则并行时互相抢会话而假失败。

### 3. `LastPresentTime == 0` 的帧内容未定义

`AcquireNextFrame` 会成功返回，但此时只有指针更新，纹理通常是全黑。
必须丢弃并重试（实测首次抓取常需要 1–8 次），否则截图全黑。

## 未采纳方案

- **Windows.Graphics.Capture（WGC）**：微软推荐的新方案，帧池持续供帧、能正确处理 HDR，
  但要引入 WinRT 异步帧池、`IsBorderRequired` 关黄框、跨屏尺寸语义，复杂度与
  `Graphics_Capture` / `Graphics_DirectX*` 三个 features 都更高。DDA 已能解决本问题且为同步 API，
  更贴合「一次抓屏」的使用形态。
- **抓 8bit 后反推色调映射**：信息已截断，不可逆。实测 BitBlt 帧 23% 像素为纯白，无从还原。
- **检测到 HDR 就弹提示让用户关 HDR**：把系统设置问题转嫁给用户，与「触发即用」定位冲突。

## 影响

- **性能**（红线：热键→可见 150ms / 4K 250ms）：debug 下 2560×1440 实测 **80–103 ms**
  （色调映射 38–65 ms + 取帧/建设备 ≈40 ms）。

  初版逐像素做「除白点 + clamp + sRGB 查表」，色调映射要 70–100 ms、总耗时 127–173 ms，
  已贴着红线。改为把「拆位 → 除白点 → clamp → sRGB 编码」合成一张 64 KB 的
  `encode_lut`（half 位模式 → 最终 8bit），逐像素只剩 3 次数组访问，耗时降为 38–65 ms。
  按 4K 是 1440p 的 2.25 倍估算，仍在 250 ms 预算内；release 构建未实测（需提权运行）。

  **缓存 duplication 会话能省掉建 D3D11 设备的 ≈40 ms，但会让设备常驻，与「待机 ≈0 开销」冲突，
  故每次现建。** 若后续真要提速，优先优化是开 GPU 色调映射（写 shader），但那会把抓屏拉进
  GPU 渲染流程，与 ADR 0001 的纯 CPU 渲染原则抵赖，暂不做。
- **待机开销不变**：抓屏只在按键时发生，无轮询、无定时器。
- **多屏**：逐 output 抓取后按 `DesktopCoordinates` 贴进虚拟屏画布，负坐标布局与 BitBlt 路径语义一致。
  但**混合 HDR + SDR 多屏未实测**（当前无多屏环境）。
- **许可证**：仅使用系统 API 与既有 `windows` crate，无新依赖；`cargo deny check` 配置不变。
- **已知限制**：`DuplicateOutput1` 在个别驱动（如本机 Intel Iris Xe）以外的组合下行为需实测；
  失败时 `capture_virtual_screen` 返回 `Err`，用户会看到「抓屏失败」提示而非静默劣化。
