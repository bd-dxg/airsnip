// SPDX-License-Identifier: GPL-3.0-only
//! 区域抓帧（LNG-2/3，issue #128）：长截图逐帧采集视口。
//!
//! HDR 输出走**复用**的抓帧会话（`hdr::RegionCapturer`），纯 SDR 走每帧 BitBlt。
//! 复用是为了避免 HDR 下每帧重建 DXGI —— 那会在 NVIDIA 驱动上累积内存泄漏。
//! 单次全屏抓屏（`capture_virtual_screen`）不受影响，仍是现建现用。

use super::capture_region_bitblt;
use super::hdr;

/// 区域抓帧器：跨帧复用 HDR 抓帧会话；SDR 时回退 BitBlt。
pub struct RegionCapture {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    /// HDR 会话；`None` 表示纯 SDR，走 BitBlt。
    hdr: Option<hdr::RegionCapturer>,
    /// 最近一帧（桌面无变化时沿用，让拼接看到「静止」）。
    last: Vec<u8>,
    /// 是否已取到过至少一帧（决定 HDR 会话能否把「无变化」当静止）。
    have_frame: bool,
}

impl RegionCapture {
    /// 建立区域抓帧器。HDR 会话建立失败时返回 `Err`（长截图会取消并提示）。
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Result<Self, String> {
        let hdr = match hdr::RegionCapturer::open(x, y, w, h) {
            None => None, // 纯 SDR
            Some(Ok(c)) => Some(c),
            Some(Err(e)) => return Err(e),
        };
        Ok(Self {
            x,
            y,
            w,
            h,
            hdr,
            last: Vec::new(),
            have_frame: false,
        })
    }

    /// 抓取当前区域帧，返回最近一帧像素（长度 = `w*h*4`）。
    pub fn grab(&mut self) -> Result<Vec<u8>, String> {
        if let Some(capturer) = self.hdr.as_mut() {
            match capturer.grab(!self.have_frame) {
                Ok(Some(rgba)) => {
                    self.last = rgba;
                    self.have_frame = true;
                }
                // 桌面无变化：沿用上一帧（拼接会看到位移 0 → 静止）。
                Ok(None) => {
                    if !self.have_frame {
                        return Err("桌面复制连续取不到有效帧".into());
                    }
                }
                Err(hdr::GrabError::Lost) => {
                    // 会话失效（分辨率切换 / 全屏 / 锁屏）：重建一次再继续。
                    match hdr::RegionCapturer::open(self.x, self.y, self.w, self.h) {
                        Some(Ok(c)) => {
                            self.hdr = Some(c);
                            self.have_frame = false;
                        }
                        Some(Err(e)) => return Err(e),
                        // 已不再有 HDR 输出：回退 BitBlt。
                        None => self.hdr = None,
                    }
                }
                Err(hdr::GrabError::Other(e)) => return Err(e),
            }
        } else {
            self.last = capture_region_bitblt(self.x, self.y, self.w, self.h)?.rgba;
        }
        Ok(self.last.clone())
    }
}
