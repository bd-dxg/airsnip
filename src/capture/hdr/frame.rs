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

use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_USAGE_STAGING, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_MODE_ROTATION,
};
use windows::Win32::Graphics::Dxgi::{DXGI_OUTDUPL_FRAME_INFO, IDXGIOutput5, IDXGIResource};
use windows::core::Interface;

use super::super::tonemap::{bgra_to_rgba, tonemap};
use super::output::{Frame, HdrOutput};

/// `AcquireNextFrame` 等待新帧的超时（毫秒）。静态桌面下靠多次重试凑满一帧。
const ACQUIRE_TIMEOUT_MS: u32 = 300;
/// 空帧 / 超时的最大重试次数。
const MAX_ACQUIRE_ATTEMPTS: u32 = 20;
/// `DXGI_ERROR_WAIT_TIMEOUT`：桌面自上次抓帧后没有变化。
const DXGI_ERROR_WAIT_TIMEOUT: i32 = 0x887A0027u32 as i32;

/// 把 DDA 返回的未旋转帧转到桌面朝向（DXGI_OUTDUPL_DESC.Rotation）。
///
/// DDA 契约：帧是面板**原生**朝向（未旋转），旋转显示器的帧尺寸与桌面矩形
/// 宽高互换，必须按 `Rotation` 转正后才能贴进虚拟屏画布——否则旋转屏的内容
/// 会横着铺进相邻屏幕（多屏实测缺陷，2026-11）。
/// 返回转正后的 (RGBA, 宽, 高)；IDENTITY/UNSPECIFIED 原样返回。
pub fn rotate_to_desktop(rgba: Vec<u8>, w: u32, h: u32, rotation: DXGI_MODE_ROTATION) -> (Vec<u8>, u32, u32) {
    match rotation.0 {
        2 => rotate90(&rgba, w as usize, h as usize),
        3 => rotate180(&rgba, w as usize, h as usize),
        4 => rotate270(&rgba, w as usize, h as usize),
        _ => (rgba, w, h),
    }
}

/// 顺时针 90°：输出 h×w（宽高互换），dst(x', y') = src(y, h-1-x) 其中 x'=y 列……
/// 即新图第 y' 行第 x' 列取原图第 x' 行第 h-1-y' 列（y'∈[0,w)）。
fn rotate90(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, u32, u32) {
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h {
        for x in 0..w {
            let src = (y * w + x) * 4;
            let dst = (x * h + (h - 1 - y)) * 4;
            out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    (out, h as u32, w as u32)
}

/// 180°：宽高不变，中心对称。
fn rotate180(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, u32, u32) {
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h {
        for x in 0..w {
            let src = (y * w + x) * 4;
            let dst = ((h - 1 - y) * w + (w - 1 - x)) * 4;
            out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    (out, w as u32, h as u32)
}

/// 逆时针 90°：输出 h×w。
fn rotate270(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, u32, u32) {
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h {
        for x in 0..w {
            let src = (y * w + x) * 4;
            let dst = ((w - 1 - x) * h + y) * 4;
            out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    (out, h as u32, w as u32)
}

/// 抓一帧并转换成 RGBA8。
pub fn grab_frame(out: &HdrOutput) -> Result<Frame, String> {
    let timing = std::env::var_os("AIRSNIP_TIMING").is_some();
    let t_all = std::time::Instant::now();
    unsafe {
        let device_iunk = out
            .device
            .cast::<windows::core::IUnknown>()
            .map_err(|e| format!("取 IUnknown 失败：{e}"))?;
        let output5 = out
            .output
            .cast::<IDXGIOutput5>()
            .map_err(|e| format!("取 IDXGIOutput5 失败：{e}"))?;

        // 请求 FP16；列表里带上 BGRA8 兜底，避免个别驱动拒绝单一格式。
        let dup = output5
            .DuplicateOutput1(
                &device_iunk,
                0,
                &[DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_B8G8R8A8_UNORM],
            )
            .map_err(|e| {
                format!("DuplicateOutput1 失败：{e}（需进程为 Per-Monitor V2 DPI aware，且无其他抓屏占用）")
            })?;

        // 找一帧带真实桌面内容的：LastPresentTime == 0 表示只有指针更新，内容未定义。
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let mut attempts = 0;
        loop {
            if attempts >= MAX_ACQUIRE_ATTEMPTS {
                return Err("桌面复制连续取不到有效帧".into());
            }
            match dup.AcquireNextFrame(ACQUIRE_TIMEOUT_MS, &mut info, &mut resource) {
                Ok(()) => {
                    if info.LastPresentTime != 0 {
                        break;
                    }
                    let _ = dup.ReleaseFrame();
                    attempts += 1;
                }
                Err(e) if e.code().0 == DXGI_ERROR_WAIT_TIMEOUT => attempts += 1,
                Err(e) => return Err(format!("AcquireNextFrame 失败：{e}")),
            }
        }

        let texture: ID3D11Texture2D = resource
            .as_ref()
            .ok_or("桌面资源为空")?
            .cast()
            .map_err(|e| format!("取纹理失败：{e}"))?;
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
            .map_err(|e| format!("CreateTexture2D（staging）失败：{e}"))?;
        let staging = staging.ok_or("staging 纹理为空")?;
        out.context.CopyResource(&staging, &texture);
        let _ = dup.ReleaseFrame();

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        out.context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| format!("Map 失败：{e}"))?;
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
        // 旋转屏：DDA 帧是面板原生朝向，按 Rotation 转正后尺寸与桌面矩形一致
        let (rgba, w, h) = rotate_to_desktop(rgba, w, h, out.rotation);
        let _ = dup.ReleaseFrame();
        if timing {
            eprintln!("[airsnip] HDR 抓帧总耗时 {:?}", t_all.elapsed());
        }

        Ok(Frame {
            width: w,
            height: h,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use windows::Win32::Graphics::Dxgi::Common::DXGI_MODE_ROTATION;

    /// 2×2 测试图：四个像素各有独立颜色，验证旋转后角点映射。
    fn sample() -> (Vec<u8>, u32, u32) {
        // [A B / C D]，行优先
        (vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255], 2, 2)
    }

    #[test]
    fn rotate180_flips_both_axes() {
        let (rgba, w, h) = sample();
        let (out, w2, h2) = rotate_to_desktop(rgba, w, h, DXGI_MODE_ROTATION(3));
        assert_eq!((w2, h2), (2, 2));
        // 中心对称：A(0,0) ↔ D(1,1)，B(1,0) ↔ C(0,1)
        assert_eq!(out[0], 40);
        assert_eq!(out[12], 10);
        assert_eq!(out[4], 30);
        assert_eq!(out[8], 20);
    }

    #[test]
    fn rotate90_swaps_dimensions() {
        // 2×3 图（宽2 高3），ROTATE90 后应为 3×2
        let mut rgba = Vec::new();
        for v in 0..6u8 {
            rgba.extend_from_slice(&[v, 0, 0, 255]);
        }
        let (out, w2, h2) = rotate_to_desktop(rgba, 2, 3, DXGI_MODE_ROTATION(2));
        assert_eq!((w2, h2), (3, 2));
        assert_eq!(out.len(), 6 * 4);
        // 顺时针：原图第一行 [A B] 落到新图最右列，A 在右上角
        let px = |out: &[u8], x: usize, y: usize, w2: usize| out[(y * w2 + x) * 4];
        // 原图 (x=0,y=0)=A(0) → dst(x'=h-1-0=2, y'=0) 右上角
        assert_eq!(px(&out, 2, 0, 3), 0);
        // 原图 (x=1,y=2)=F(5) → dst(x'=h-1-2=0, y'=1) 左下角
        assert_eq!(px(&out, 0, 1, 3), 5);
        // 原图 (x=1,y=0)=B(1) → dst(x'=2, y'=1) 右下角
        assert_eq!(px(&out, 2, 1, 3), 1);
    }

    #[test]
    fn rotate270_swaps_dimensions_other_way() {
        let mut rgba = Vec::new();
        for v in 0..6u8 {
            rgba.extend_from_slice(&[v, 0, 0, 255]);
        }
        let (out, w2, h2) = rotate_to_desktop(rgba, 2, 3, DXGI_MODE_ROTATION(4));
        assert_eq!((w2, h2), (3, 2));
        let px = |out: &[u8], x: usize, y: usize, w2: usize| out[(y * w2 + x) * 4];
        // 逆时针：原图 (x=0,y=0)=A(0) → dst(x'=y=0, y'=w-1-x=1) 左下角
        assert_eq!(px(&out, 0, 1, 3), 0);
        // 原图 (x=1,y=2)=F(5) → dst(x'=2, y'=0) 右上角
        assert_eq!(px(&out, 2, 0, 3), 5);
        // 原图第一行 [A B] 落到新图最左列：dst(0,0)=B(1)
        assert_eq!(px(&out, 0, 0, 3), 1);
    }

    #[test]
    fn identity_keeps_frame_untouched() {
        let (rgba, w, h) = sample();
        let (out, w2, h2) = rotate_to_desktop(rgba.clone(), w, h, DXGI_MODE_ROTATION(1));
        assert_eq!((w2, h2), (2, 2));
        assert_eq!(out, rgba);
    }
}
