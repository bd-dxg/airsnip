# 长截图：已知问题与抓屏排查记录

记录 v0.11.x 期间排查「长截图拼不出来」的过程，避免下一轮重复实验。
排查用的临时代码已全部删除（重建方法见末节）。

## 现象

1. **目标窗口是 Windows Terminal 时拼不出来**（v0.11.0 复现，与 #152 的贴图改动无关）。
   状态条正常出现、滚轮滚下去终端内容可见地上移，但「已拼 N px」不增长。
   - 记事本、资源管理器、Chrome 都正常。
   - `AIRSNIP_DEBUG=1` 的 `%TEMP%\airsnip-longshot.log` 里连续 20+ 帧
     `capture#N hash=` **完全相同**，`push: offset=0 confident=true` —— 抓到的画面没变。
2. **视口内有动态内容**（动画徽章、光标闪烁、进度条）时拼接也会失败。用户观察，未系统复现。
   现有 `longshot/stitch.rs` 是**全帧定位**，局部动态元素会让整帧匹配失败。

## 已试过的抓屏后端（2026-11-12，单屏 2560×1440 / Windows 11 / 100%）

| 后端 | 终端 | 记事本 |
|---|---|---|
| GDI `BitBlt`（现状） | ✗ 帧不变 | ✓ |
| `BitBlt` + `CAPTUREBLT` | ✗ | ✗ |
| DXGI 桌面复制（DDA，SDR 下强制启用） | ✗ 帧不变 | ✓ |
| `PrintWindow(PW_RENDERFULLCONTENT)` + 裁剪 | ✗ 帧不变 | ✓ |

四种后端都拿不到终端的更新帧，而同机 GDI 应用全部正常。

**已排除**：不是我们自己的置顶窗口（边框 / 状态条 / 预览）遮挡导致。用一个隔离探针
（不起 AirSnip、直接抓前台窗口两帧比对）测过，差异像素为 0，结论一致。

## 未验证的方向

- 终端一类 GPU / DirectComposition 渲染窗口的内容是否走了 MPO（hardware overlay），
  导致重定向表面（GDI）与 DDA 都读不到更新。
- 拼接算法要能容忍**局部动态区域**：现有实现用全帧行匹配定位，任何持续变化的元素都会失败；
  可选做法是定位时排除"每帧都在变"的行/块。
- 更彻底的路线：按**窗口**抓而不是按屏幕矩形（`PrintWindow` 全窗抓再裁、或
  `Windows.Graphics.Capture`（WinRT，需要一次授权 UI））。

## 需要时怎么重造探针

- `capture/region.rs` 的 `RegionCapture::new` 里用环境变量 `AIRSNIP_LONGSHOT_CAPTURE`
  切后端：`bitblt`（默认）/ `captureblt` / `dda` / `printwindow`。
- `capture/mod.rs`：`capture_region_bitblt_opts(x, y, w, h, capture_layered)`、
  `capture_region_printwindow(x, y, w, h)`（`PrintWindow` 在 `windows` crate 里挂在
  `Win32_Storage_Xps` 特性下，探针用 `#[link(name = "user32")]` 直接声明，避免为一个函数加特性）。
- `capture/hdr/output.rs` 的 `output_intersecting_any` + `hdr/session.rs` 的
  `HdrRegionSession::open_any`：让 DDA 不要求 HDR 输出。
- `#[ignore]` 测试 `capture::tests::manual_grab_probe`：抓前台窗口两帧、打印差异像素数并存 PNG
  （`cargo test manual_grab_probe -- --ignored --nocapture`）。
