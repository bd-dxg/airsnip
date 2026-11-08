// SPDX-License-Identifier: GPL-3.0-only
//! HDR 区域抓帧会话（LNG-2/3，issue #128）：长截图逐帧采集视口时复用同一套
//! D3D11 设备与桌面复制（duplication）会话。
//!
//! 为什么必须复用：重建一次 DXGI（释放设备与 duplication 后再 `D3D11CreateDevice` 重来）
//! 在 NVIDIA 驱动上会累积内存泄漏 —— 长截图每 ~90ms 一拍，几百拍就泄漏数 GB
//! （参见 issue #128）。单次截图仍走 `frame::grab_frame` 的「现建现用」路径（ADR 0024）。
//!
//! `open` 返回 `None` 表示请求区域不在 HDR 输出上（纯 SDR，调用方走 BitBlt）。

use windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication;

use super::frame::{self, GrabError};
use super::output::{HdrOutput, blit_into, hdr_output_intersecting};

/// 一个 HDR 输出的区域抓帧会话：持有设备与 duplication，跨帧复用。
pub struct RegionCapturer {
    out: HdrOutput,
    dup: IDXGIOutputDuplication,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

impl RegionCapturer {
    /// 若请求区域落在 HDR 输出上则建立会话；非 HDR 返回 `None`（调用方走 BitBlt）。
    pub fn open(x: i32, y: i32, w: u32, h: u32) -> Option<Result<Self, String>> {
        let states = crate::capture::hdr_display::active_display_states();
        let out = hdr_output_intersecting(&states, x, y, w, h)?;
        match frame::open_duplication(&out) {
            Ok(dup) => Some(Ok(Self { out, dup, x, y, w, h })),
            Err(e) => Some(Err(e)),
        }
    }

    /// 抓取当前区域帧（RGBA，长度 = `w*h*4`）。
    ///
    /// - `Ok(Some(rgba))`：新帧；
    /// - `Ok(None)`：桌面无变化（复用会话时），上层当作「静止」；
    /// - `Err(Lost)`：会话失效，调用方应重建后再抓。
    pub fn grab(&self, need_frame: bool) -> Result<Option<Vec<u8>>, GrabError> {
        let Some(frame) = frame::acquire(&self.out, &self.dup, need_frame)? else {
            return Ok(None);
        };
        let mut rgba = vec![0u8; (self.w as usize) * (self.h as usize) * 4];
        let dx = self.out.rect.left - self.x;
        let dy = self.out.rect.top - self.y;
        blit_into(&mut rgba, self.w, self.h, dx, dy, &frame).map_err(GrabError::Other)?;
        Ok(Some(rgba))
    }
}
