// SPDX-License-Identifier: GPL-3.0-only
//! HDR 输出枚举与画布拼装（CAP-1，issue #75）。
//!
//! 负责把 DXGI 输出与 DisplayConfig 的色彩状态对应起来，并提供把单个输出的帧
//! 贴进虚拟屏画布的小工具（负坐标布局、越界裁剪）。

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6};
use windows::core::Interface;

use super::super::hdr_display::{DisplayColorState, state_for_device};

/// 一个已连接且处于 HDR 模式的输出及其抓帧上下文。
pub struct HdrOutput {
    pub output: IDXGIOutput6,
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    /// 桌面矩形（物理像素，虚拟桌面坐标）。
    pub rect: windows::Win32::Foundation::RECT,
    pub color: DisplayColorState,
}

impl HdrOutput {
    /// 为已连接输出建 D3D11 设备与抓帧上下文。
    pub fn attach(out: &AttachedOutput) -> Result<Self, String> {
        let (device, context) = create_context(&out.output)?;
        Ok(Self {
            output: out.output.clone(),
            device,
            context,
            rect: out.rect,
            color: out.color,
        })
    }
}

/// 一个已连接输出（**不区分 HDR / SDR**）的矩形与色彩状态。
///
/// 虚拟屏拼装需要逐个输出处理：HDR 走 DDA，SDR 走 BitBlt 补齐（issue #156）。
pub struct AttachedOutput {
    pub output: IDXGIOutput6,
    /// 桌面矩形（物理像素，虚拟桌面坐标）。
    pub rect: windows::Win32::Foundation::RECT,
    pub color: DisplayColorState,
}

/// 单个输出的抓帧结果（RGBA8）。
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 枚举全部已连接输出（含 SDR，调用方自行决定谁走 DDA）。
pub fn attached_outputs(states: &[(String, DisplayColorState)]) -> Result<Vec<AttachedOutput>, String> {
    let mut found = Vec::new();
    visit_attached_outputs(states, |output6, desc, color| {
        let rect = unsafe {
            output6
                .GetDesc1()
                .map(|d| d.DesktopCoordinates)
                .unwrap_or(desc.DesktopCoordinates)
        };
        found.push(AttachedOutput {
            output: output6.clone(),
            rect,
            color,
        });
        Ok(true)
    })?;
    Ok(found)
}

/// 找与请求矩形相交的 HDR 输出（长截图区域抓取用）。
pub fn hdr_output_intersecting(
    states: &[(String, DisplayColorState)],
    x: i32,
    y: i32,
    w: u32,
    h: u32,
) -> Option<HdrOutput> {
    let target = windows::Win32::Foundation::RECT {
        left: x,
        top: y,
        right: x.saturating_add(w as i32),
        bottom: y.saturating_add(h as i32),
    };
    let mut picked = None;
    let _ = visit_attached_outputs(states, |output6, desc, color| {
        // 只有 HDR 输出才需要 DDA，SDR 区域交给 BitBlt 更快。
        if !color.hdr_enabled {
            return Ok(true);
        }
        let r = desc.DesktopCoordinates;
        let disjoint =
            r.right <= target.left || r.left >= target.right || r.bottom <= target.top || r.top >= target.bottom;
        if disjoint {
            return Ok(true); // 继续找
        }
        let Ok((device, context)) = create_context(output6) else {
            return Ok(true);
        };
        picked = Some(HdrOutput {
            output: output6.clone(),
            device,
            context,
            rect: r,
            color,
        });
        Ok(false) // 命中即停
    });
    picked
}

/// 遍历所有已连接输出；回调返回 `Ok(true)` 继续、`Ok(false)` 停止、`Err` 中断。
///
/// 回调参数：输出接口、`DXGI_OUTPUT_DESC`、该输出的色彩状态。
fn visit_attached_outputs<F>(states: &[(String, DisplayColorState)], mut visit: F) -> Result<(), String>
where
    F: FnMut(
        &IDXGIOutput6,
        &windows::Win32::Graphics::Dxgi::DXGI_OUTPUT_DESC,
        DisplayColorState,
    ) -> Result<bool, String>,
{
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| format!("CreateDXGIFactory1 失败：{e}"))?;
        let mut adapter_index = 0;
        loop {
            let Ok(adapter) = factory.EnumAdapters1(adapter_index) else {
                break;
            };
            adapter_index += 1;
            let mut out_index = 0;
            loop {
                let Some(output) = adapter.EnumOutputs(out_index).ok() else {
                    break;
                };
                out_index += 1;
                let Some(output6) = output.cast::<IDXGIOutput6>().ok() else {
                    continue;
                };
                let Ok(desc) = output6.GetDesc() else { continue };
                if !desc.AttachedToDesktop.as_bool() {
                    continue;
                }
                let name = String::from_utf16_lossy(
                    &desc
                        .DeviceName
                        .iter()
                        .take_while(|c| **c != 0)
                        .copied()
                        .collect::<Vec<_>>(),
                );
                let color = state_for_device(states, &name);
                if !visit(&output6, &desc, color)? {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

/// 为一个输出建 D3D11 设备与立即上下文。
fn create_context(output: &IDXGIOutput6) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    unsafe {
        let adapter = output
            .GetParent::<windows::Win32::Graphics::Dxgi::IDXGIAdapter>()
            .map_err(|e| format!("取 IDXGIAdapter 失败：{e}"))?;
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        // BGRA_SUPPORT 是 DDA 的硬性要求；10_0 兜底老显卡。
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .map_err(|e| format!("D3D11CreateDevice 失败：{e}"))?;
        match (device, context) {
            (Some(d), Some(c)) => Ok((d, c)),
            _ => Err("D3D11CreateDevice 未返回设备".into()),
        }
    }
}

/// 把输出帧贴进目标画布，裁掉超出画布的部分。
///
/// `dst_x`/`dst_y` 是帧左上角在画布中的坐标（可为负，对应虚拟屏负坐标布局）。
pub fn blit_into(canvas: &mut [u8], cw: u32, ch: u32, dst_x: i32, dst_y: i32, frame: &Frame) -> Result<(), String> {
    let (cw, ch) = (cw as i32, ch as i32);
    // 求交集，源与目标都要相应偏移
    let x0 = dst_x.max(0);
    let y0 = dst_y.max(0);
    let x1 = (dst_x + frame.width as i32).min(cw);
    let y1 = (dst_y + frame.height as i32).min(ch);
    if x1 <= x0 || y1 <= y0 {
        return Ok(()); // 完全在画布外
    }
    let (sx0, sy0) = (x0 - dst_x, y0 - dst_y);
    let row_bytes = ((x1 - x0) as usize) * 4;
    for y in 0..(y1 - y0) {
        let src_off = ((sy0 + y) as usize * frame.width as usize + sx0 as usize) * 4;
        let dst_off = ((y0 + y) as usize * cw as usize + x0 as usize) * 4;
        if src_off + row_bytes > frame.rgba.len() || dst_off + row_bytes > canvas.len() {
            return Err("贴图层越界".into());
        }
        canvas[dst_off..dst_off + row_bytes].copy_from_slice(&frame.rgba[src_off..src_off + row_bytes]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, value: u8) -> Frame {
        Frame {
            width: w,
            height: h,
            rgba: vec![value; (w * h * 4) as usize],
        }
    }

    #[test]
    fn blit_places_frame_at_offset() {
        let mut canvas = vec![0u8; 4 * 3 * 4];
        let f = frame(2, 2, 9);
        blit_into(&mut canvas, 4, 3, 1, 1, &f).unwrap();
        let at = |x: usize, y: usize| canvas[(y * 4 + x) * 4];
        assert_eq!(at(1, 1), 9);
        assert_eq!(at(2, 1), 9);
        assert_eq!(at(1, 2), 9);
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(3, 2), 0);
    }

    #[test]
    fn blit_clips_negative_origin() {
        let mut canvas = vec![0u8; 2 * 2 * 4];
        let f = frame(2, 2, 7);
        // 帧左上角在画布 (-1,-1)，只贴进右下 1x1
        blit_into(&mut canvas, 2, 2, -1, -1, &f).unwrap();
        let at = |x: usize, y: usize| canvas[(y * 2 + x) * 4];
        assert_eq!(at(0, 0), 7);
        assert_eq!(at(1, 1), 0);
    }

    #[test]
    fn blit_ignores_fully_outside_frame() {
        let mut canvas = vec![1u8; 2 * 2 * 4];
        let f = frame(2, 2, 9);
        blit_into(&mut canvas, 2, 2, 10, 10, &f).unwrap();
        assert!(canvas.iter().all(|v| *v == 1), "画布不应被改动");
    }
}
