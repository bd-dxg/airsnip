// SPDX-License-Identifier: GPL-3.0-only
//! HDR 抓屏入口（CAP-1，issue #75）。
//!
//! 两个公开函数与 `capture` 模块的同名 GDI 实现一一对应：返回 `None` 表示
//! 「当前不是 HDR 环境、无需介入」，调用方继续走 `BitBlt` 快路径。
//!
//! 之所以用 `Option<Result<..>>` 而不是直接返回 `Result`，是为了让分流逻辑留在
//! `capture` 顶层：HDR 模块只负责它擅长的部分，SDR 行为完全不受影响。

mod frame;
mod output;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, ReleaseDC, SRCCOPY, SelectObject,
};

use super::CapturedScreen;
use super::hdr_display::active_display_states;
use super::tonemap::bgra_to_rgba;
use frame::grab_frame;
use output::{Frame, all_hdr_outputs, all_sdr_rects, blit_into, hdr_output_intersecting};

/// 抓取整个虚拟屏幕。非 HDR 环境返回 `None`。
pub fn capture_virtual_screen() -> Option<Result<CapturedScreen, String>> {
    let states = active_display_states();
    let outputs = match all_hdr_outputs(&states) {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    if outputs.is_empty() {
        return None; // 纯 SDR，交给 BitBlt
    }

    // 与 GDI 版本保持一致：画布覆盖整个虚拟屏幕，负坐标布局平移到原点。
    let (vx, vy, vw, vh) = unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        };
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
            GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
        )
    };
    if vw == 0 || vh == 0 {
        return Some(Err("虚拟屏幕尺寸为 0，无法抓屏".into()));
    }

    let mut canvas = vec![0u8; (vw as usize) * (vh as usize) * 4];
    for out in &outputs {
        let result = grab_frame(out);
        let frame = match result {
            Ok(f) => f,
            Err(e) => return Some(Err(e)),
        };
        // 把该输出的帧贴进虚拟屏画布（裁掉超出画布的部分）
        if let Err(e) = blit_into(&mut canvas, vw, vh, out.rect.left - vx, out.rect.top - vy, &frame) {
            return Some(Err(e));
        }
    }
    // 混合 HDR/SDR 桌面：SDR 显示器不走 DDA（all_hdr_outputs 已按 hdr_enabled 过滤），
    // 其区域用 GDI BitBlt 补进画布，否则 SDR 屏在冻结画面里是黑的
    // （多屏实测缺陷：主屏 SDR + 副屏 HDR 时主屏全黑，2026-11）。
    let sdr_rects = match all_sdr_rects(&states) {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    for rect in &sdr_rects {
        let frame = match gdi_sdr_frame(rect) {
            Ok(f) => f,
            Err(e) => return Some(Err(e)),
        };
        if let Err(e) = blit_into(&mut canvas, vw, vh, rect.left - vx, rect.top - vy, &frame) {
            return Some(Err(e));
        }
    }

    Some(Ok(CapturedScreen {
        width: vw,
        height: vh,
        origin_x: vx,
        origin_y: vy,
        rgba: canvas,
    }))
}

/// GDI BitBlt 抓单个 SDR 输出矩形为 RGBA 帧（内容与画布同坐标系，物理像素）。
fn gdi_sdr_frame(rect: &RECT) -> Result<Frame, String> {
    let w = (rect.right - rect.left).max(0) as usize;
    let h = (rect.bottom - rect.top).max(0) as usize;
    if w == 0 || h == 0 {
        return Ok(Frame {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        });
    }
    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.0.is_null() {
            return Err("GetDC 失败".into());
        }
        let hdc_mem = CreateCompatibleDC(None);
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32); // 自上而下
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbmp = match CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(e) => {
                let _ = ReleaseDC(None, hdc_screen);
                return Err(format!("CreateDIBSection（SDR 补齐）失败：{e}"));
            }
        };
        let old = SelectObject(hdc_mem, hbmp.into());
        let blit = BitBlt(
            hdc_mem,
            0,
            0,
            w as i32,
            h as i32,
            Some(hdc_screen),
            rect.left,
            rect.top,
            SRCCOPY,
        );
        let data = std::slice::from_raw_parts(bits as *const u8, w * h * 4);
        let rgba = if blit.is_ok() {
            bgra_to_rgba(data, w * 4, w as u32, h as u32)
        } else {
            Vec::new()
        };
        let _ = SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbmp.into());
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);
        if !blit.is_ok() {
            return Err("BitBlt（SDR 补齐）失败".into());
        }
        Ok(Frame {
            width: w as u32,
            height: h as u32,
            rgba,
        })
    }
}

/// 抓取屏幕指定区域（长截图逐帧采集视口用，LNG-2/3）。非 HDR 环境返回 `None`。
///
/// DDA 只能整屏抓，这里抓完再按请求矩形裁剪；越界部分留黑（与 BitBlt 行为一致）。
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> Option<Result<CapturedScreen, String>> {
    let states = active_display_states();
    let out = hdr_output_intersecting(&states, x, y, w, h)?;

    let result = grab_frame(&out);
    let frame = match result {
        Ok(f) => f,
        Err(e) => return Some(Err(e)),
    };
    // 帧的左上是 out.rect.left/top，换算成相对请求原点的偏移（可为负）
    let dx = out.rect.left - x;
    let dy = out.rect.top - y;
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    if let Err(e) = blit_into(&mut rgba, w, h, dx, dy, &frame) {
        return Some(Err(e));
    }
    Some(Ok(CapturedScreen {
        width: w,
        height: h,
        origin_x: x,
        origin_y: y,
        rgba,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 同一个 output 同时只能有一个 duplication 会话，并行跑会互相抢到 E_INVALIDARG / E_ACCESSDENIED。
    static CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 设置 Per-Monitor V2（`DuplicateOutput1` 的硬性前提；测试进程没有 manifest）。
    fn make_per_monitor_v2() {
        unsafe {
            use windows::Win32::UI::HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
            };
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    /// 本机若处于 HDR 模式，抓屏应成功且与虚拟屏幕尺寸一致。
    ///
    /// 默认 `--ignored`：需要真实桌面会话，无头环境会失败。
    #[test]
    #[ignore]
    fn hdr_capture_works() {
        let _guard = CAPTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        make_per_monitor_v2();
        match capture_virtual_screen() {
            None => println!("当前非 HDR 环境，走 BitBlt 路径"),
            Some(Ok(s)) => {
                assert!(s.width > 0 && s.height > 0);
                assert_eq!(s.rgba.len(), (s.width * s.height * 4) as usize);
                assert!(s.rgba.chunks_exact(4).all(|p| p[3] == 255), "输出必须不透明");
            }
            Some(Err(e)) => panic!("HDR 抓屏失败：{e}"),
        }
    }

    /// 区域抓取应返回请求尺寸，且越界不 panic。
    #[test]
    #[ignore]
    fn hdr_region_capture_works() {
        let _guard = CAPTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        make_per_monitor_v2();
        let (x, y) = unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};
            (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN))
        };
        let (w, h) = unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN};
            (
                GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
                GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
            )
        };
        match capture_region(x, y, w, h) {
            None => println!("当前非 HDR 环境，长截图走 BitBlt"),
            Some(Ok(s)) => assert_eq!(s.rgba.len(), (w * h * 4) as usize),
            Some(Err(e)) => panic!("HDR 区域抓取失败：{e}"),
        }
    }

    /// 临时耗时测量（红线：热键→可见 150ms / 4K 250ms）。
    #[test]
    #[ignore]
    fn hdr_capture_timing() {
        let _guard = CAPTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        make_per_monitor_v2();
        let _ = capture_virtual_screen(); // 预热（首次含驱动初始化）
        for i in 1..=5 {
            let t = std::time::Instant::now();
            let r = capture_virtual_screen();
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            match r {
                Some(Ok(s)) => println!("第 {i} 次：{ms:>7.1} ms  {}x{}", s.width, s.height),
                Some(Err(e)) => {
                    println!("失败：{e}");
                    return;
                }
                None => println!("非 HDR 环境（走 BitBlt），耗时 {ms:.1} ms"),
            }
        }
    }
}
