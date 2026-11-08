// SPDX-License-Identifier: GPL-3.0-only
//! DXGI Desktop Duplication 抓帧（CAP-1，issue #75）：HDR 桌面下取 FP16 scRGB 原始帧。
//!
//! 为什么不能用 GDI：HDR 桌面的合成面是 scRGB FP16，`BitBlt` 只能拿到被 DWM 压平的
//! 8bit 版本，高光硬截断（实测整屏 23% 像素爆成纯白）。DDA 的 `DuplicateOutput1`
//! 可以显式请求 `R16G16B16A16_FLOAT`，拿到完整动态范围后再色调映射回 SDR。
//!
//! ## 三条硬性前提（都是实测踩过的坑，详见 ADR 0024）
//!
//! 1. **进程必须 Per-Monitor V2 DPI aware**，否则 `DuplicateOutput1` 对**所有**格式
//!    （含 BGRA8）恒返回 `DXGI_ERROR_UNSUPPORTED`，看起来像"系统不支持 HDR 抓屏"。
//!    主程序 manifest 已声明；测试与 example 进程需自行设置。
//! 2. 同一个 output 同时只能有一个 duplication 会话，旧会话未释放会让新调用返回
//!    `E_ACCESSDENIED`。
//! 3. `AcquireNextFrame` 成功但 `LastPresentTime == 0` 时只有指针更新，帧内容未定义
//!    （通常是全黑），必须丢弃重试。
//!
//! 本模块把「建会话」「取一帧」拆成两个函数：单次截图现建现用（`grab_frame`），
//! 长截图逐帧复用同一会话（`super::session::RegionCapturer`，issue #128）。

use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_USAGE_STAGING, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_FRAME_INFO, IDXGIOutput5, IDXGIOutputDuplication, IDXGIResource};
use windows::core::Interface;

use super::super::tonemap::{bgra_to_rgba, tonemap};
use super::output::{Frame, HdrOutput};

/// `AcquireNextFrame` 等待新帧的超时（毫秒）。静态桌面下靠多次重试凑满一帧。
const ACQUIRE_TIMEOUT_MS: u32 = 300;
/// 空帧 / 超时的最大重试次数。
const MAX_ACQUIRE_ATTEMPTS: u32 = 20;
/// `DXGI_ERROR_WAIT_TIMEOUT`：桌面自上次抓帧后没有变化。
const DXGI_ERROR_WAIT_TIMEOUT: i32 = 0x887A0027u32 as i32;
/// `DXGI_ERROR_ACCESS_LOST`：duplication 会话失效（分辨率切换 / 全屏独占 / 锁屏等）。
const DXGI_ERROR_ACCESS_LOST: i32 = 0x887A0026u32 as i32;

/// 抓帧失败：区分「会话失效需重建」与一般错误。
#[derive(Debug)]
pub enum GrabError {
    /// 会话失效，复用会话时必须重建后再抓（见 `super::session`）。
    Lost,
    /// 其他错误，附文本。
    Other(String),
}

/// 为一个输出建立桌面复制会话（`DuplicateOutput1`，请求 FP16 + BGRA8 兜底）。
pub fn open_duplication(out: &HdrOutput) -> Result<IDXGIOutputDuplication, String> {
    unsafe {
        let device_iunk = out
            .device
            .cast::<windows::core::IUnknown>()
            .map_err(|e| format!("取 IUnknown 失败：{e}"))?;
        let output5 = out
            .output
            .cast::<IDXGIOutput5>()
            .map_err(|e| format!("取 IDXGIOutput5 失败：{e}"))?;
        output5
            .DuplicateOutput1(
                &device_iunk,
                0,
                &[DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_B8G8R8A8_UNORM],
            )
            .map_err(|e| format!("DuplicateOutput1 失败：{e}（需进程为 Per-Monitor V2 DPI aware，且无其他抓屏占用）"))
    }
}

/// 从已有复制会话取一帧并转成 RGBA8。
///
/// `need_frame`：会话首帧（或一次性抓屏）为 `true` —— 必须拿到一帧带真实桌面内容的画面
/// （`LastPresentTime != 0`），取不到就重试到上限后返回 `None`；复用会话的后续帧为 `false` ——
/// 桌面无变化（`WAIT_TIMEOUT` 或只有指针更新）时立即返回 `None`，交给上层当作「静止」。
pub fn acquire(out: &HdrOutput, dup: &IDXGIOutputDuplication, need_frame: bool) -> Result<Option<Frame>, GrabError> {
    let timing = std::env::var_os("AIRSNIP_TIMING").is_some();
    let t_all = std::time::Instant::now();
    unsafe {
        let mut attempts = 0;
        let resource: IDXGIResource = loop {
            if attempts >= MAX_ACQUIRE_ATTEMPTS {
                return Ok(None);
            }
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res: Option<IDXGIResource> = None;
            match dup.AcquireNextFrame(ACQUIRE_TIMEOUT_MS, &mut info, &mut res) {
                Ok(()) => {
                    if info.LastPresentTime != 0 {
                        break res.ok_or_else(|| GrabError::Other("桌面资源为空".into()))?;
                    }
                    // 只有指针更新，内容未定义：丢弃。
                    let _ = dup.ReleaseFrame();
                    if !need_frame {
                        return Ok(None);
                    }
                    attempts += 1;
                }
                Err(e) if e.code().0 == DXGI_ERROR_WAIT_TIMEOUT => {
                    if !need_frame {
                        return Ok(None);
                    }
                    attempts += 1;
                }
                Err(e) if e.code().0 == DXGI_ERROR_ACCESS_LOST => return Err(GrabError::Lost),
                Err(e) => return Err(GrabError::Other(format!("AcquireNextFrame 失败：{e}"))),
            }
        };

        let texture: ID3D11Texture2D = resource
            .cast()
            .map_err(|e| GrabError::Other(format!("取纹理失败：{e}")))?;
        let mut desc = Default::default();
        texture.GetDesc(&mut desc);

        // staging 纹理：GPU 侧可见资源不能直接 Map，必须先 CopyResource 到 CPU 可读副本。
        let mut staging_desc = desc;
        staging_desc.Usage = D3D11_USAGE_STAGING;
        staging_desc.BindFlags = 0;
        staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        staging_desc.MiscFlags = 0;
        let mut staging: Option<ID3D11Texture2D> = None;
        out.device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging))
            .map_err(|e| GrabError::Other(format!("CreateTexture2D（staging）失败：{e}")))?;
        let staging = staging.ok_or_else(|| GrabError::Other("staging 纹理为空".into()))?;
        out.context.CopyResource(&staging, &texture);
        let _ = dup.ReleaseFrame();

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        out.context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| GrabError::Other(format!("Map 失败：{e}")))?;
        let pitch = mapped.RowPitch as usize;
        let len = pitch * desc.Height as usize;
        let data = std::slice::from_raw_parts(mapped.pData as *const u8, len);
        let (w, h) = (desc.Width, desc.Height);
        let is_fp16 = desc.Format == DXGI_FORMAT_R16G16B16A16_FLOAT;

        let rgba = if is_fp16 {
            let t = std::time::Instant::now();
            let v = tonemap(data, pitch, w, h, out.color.sdr_white_nits);
            if timing {
                eprintln!("[airsnip] HDR 色调映射 {:?}", t.elapsed());
            }
            v
        } else {
            bgra_to_rgba(data, pitch, w, h)
        };
        out.context.Unmap(&staging, 0);
        let _ = dup.ReleaseFrame();
        if timing {
            eprintln!("[airsnip] HDR 抓帧总耗时 {:?}", t_all.elapsed());
        }

        Ok(Some(Frame {
            width: w,
            height: h,
            rgba,
        }))
    }
}

/// 一次性抓一帧（单次截图用）：现建会话 → 取帧 → 释放会话。
///
/// 长截图请改用 `super::session::RegionCapturer` 复用会话 —— 每帧重建 DXGI 会在
/// NVIDIA 驱动上累积泄漏（issue #128）。
pub fn grab_frame(out: &HdrOutput) -> Result<Frame, String> {
    let dup = open_duplication(out)?;
    match acquire(out, &dup, true) {
        Ok(Some(f)) => Ok(f),
        Ok(None) => Err("桌面复制连续取不到有效帧".into()),
        Err(GrabError::Lost) => Err("桌面复制会话失效".into()),
        Err(GrabError::Other(e)) => Err(e),
    }
}
