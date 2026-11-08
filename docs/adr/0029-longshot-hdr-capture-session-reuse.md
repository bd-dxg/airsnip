# ADR 0029：长截图 HDR 抓屏复用 DXGI 会话

- 状态：已采纳（v0.9.x 开发期）
- 影响范围：新增 `src/capture/region.rs`、`src/capture/hdr/session.rs`；`src/capture/hdr/frame.rs`
  拆出 `open_duplication` / `acquire`；`src/longshot/session.rs` 的 `ScreenSource` 改持会话
- 相关：issue #128、ADR 0024（HDR 抓屏 DDA）、ADR 0001（纯 CPU 渲染）

## 背景

issue #128：v0.9.0 正式版用户反馈，长截图滚动较长后内存占用飙到 ~15GB（报告者显卡 RTX 5060 Ti）。

排查：长截图每 ~90ms 抓一帧视口。HDR 桌面下走 DXGI Desktop Duplication（issue #75 引入），
而原实现（ADR 0024）为「待机零开销」**每次抓屏都现建 D3D11 设备 + duplication 会话，抓完即释放**——
即长截图期间在**每 90ms 重建一次 DXGI 子系统**。

NVIDIA 驱动在反复重建 DXGI（释放设备与 duplication 后 `D3D11CreateDevice` 重来）时会累积内存泄漏：
NVIDIA 开发者论坛有同症状报告——反复重建约 100 次后「泄漏数 GB 内存」，AMD 卡不泄漏。
与报告者的「NVIDIA 卡 + 越滚占用越大」高度吻合；本地 SDR 环境走 BitBlt，复现不出。

## 决策

**长截图逐帧抓取改为复用同一套 D3D11 设备与 duplication 会话**，会话结束时统一释放。
单次截图（`capture_virtual_screen`）与贴图编辑的区域抓取（`capture::capture_region`）
**保持现建现用**——ADR 0024 的取舍对一次性操作仍然成立。

- HDR 会话落在 `hdr::RegionCapturer`，由 `capture::RegionCapture` 封装（HDR 复用 / SDR 回退 BitBlt）。
- 复用会话的取帧语义：首帧必须拿到有效画面（`LastPresentTime != 0`）；后续帧桌面无变化
  （`WAIT_TIMEOUT` / 仅指针更新）立即返回「无变化」，长截图当作「静止」（位移 0，不计入拼接失败）。
- `DXGI_ERROR_ACCESS_LOST`（分辨率切换 / 全屏独占 / 锁屏）→ 重建一次会话后继续。

## 影响

- 长截图期间设备常驻（约几十 MB），会话结束即释放；**待机（无长截图）仍不持有设备**，
  不破坏「待机 ≈0 开销」红线。
- 省掉每帧建 D3D11 设备的 ≈40ms（ADR 0024 实测），长截图更流畅。
- 修复有效性依赖 NVIDIA + HDR 环境，本仓库无法自测，交给报告者复测。
