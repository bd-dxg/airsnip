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
mod session;

pub use frame::GrabError;
pub use session::HdrRegionSession;

use super::CapturedScreen;
use super::hdr_display::active_display_states;
use frame::grab_frame;
use output::{AttachedOutput, Frame, HdrOutput, attached_outputs, blit_into, hdr_output_intersecting};

/// 抓取整个虚拟屏幕。非 HDR 环境返回 `None`。
///
/// **混合 HDR + SDR 多屏**（issue #156）：只有 HDR 输出走 DDA，SDR 输出用 BitBlt 抓各自的
/// 桌面矩形贴进画布。不能把 SDR 屏留空（画布初值是零）——那会让该屏在覆盖层里整块是黑的。
pub fn capture_virtual_screen() -> Option<Result<CapturedScreen, String>> {
    let states = active_display_states();
    let attached = match attached_outputs(&states) {
        Ok(v) => v,
        Err(e) => return Some(Err(e)),
    };
    if !attached.iter().any(|o| o.color.hdr_enabled) {
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

    let rects: Vec<windows::Win32::Foundation::RECT> = attached.iter().map(|o| o.rect).collect();
    let mut canvas = vec![0u8; (vw as usize) * (vh as usize) * 4];
    if let Err(e) = assemble(&mut canvas, vw, vh, vx, vy, &rects, |i| grab_output(&attached[i])) {
        return Some(Err(e));
    }

    Some(Ok(CapturedScreen {
        width: vw,
        height: vh,
        origin_x: vx,
        origin_y: vy,
        rgba: canvas,
    }))
}

/// 抓单个输出：HDR 走 DDA + 色调映射，SDR 走 BitBlt。
fn grab_output(out: &AttachedOutput) -> Result<Frame, String> {
    if out.color.hdr_enabled {
        return grab_frame(&HdrOutput::attach(out)?);
    }
    let w = (out.rect.right - out.rect.left).max(0) as u32;
    let h = (out.rect.bottom - out.rect.top).max(0) as u32;
    // 整屏底图与纯 SDR 路径一致：带 CAPTUREBLT，把分层窗口一并抓入
    let shot = super::capture_region_bitblt(out.rect.left, out.rect.top, w, h, true)?;
    Ok(Frame {
        width: shot.width,
        height: shot.height,
        rgba: shot.rgba,
    })
}

/// 逐输出取帧并贴进画布（`vx`/`vy` 为虚拟屏左上角，用于负坐标布局平移）。
///
/// 帧来源由 `grab` 决定，便于单测验证「每个输出都被填上」（issue #156）。
fn assemble<F>(
    canvas: &mut [u8],
    cw: u32,
    ch: u32,
    vx: i32,
    vy: i32,
    rects: &[windows::Win32::Foundation::RECT],
    mut grab: F,
) -> Result<(), String>
where
    F: FnMut(usize) -> Result<Frame, String>,
{
    for (i, rect) in rects.iter().enumerate() {
        let frame = grab(i)?;
        blit_into(canvas, cw, ch, rect.left - vx, rect.top - vy, &frame)?;
    }
    Ok(())
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
    use windows::Win32::Foundation::RECT;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    /// 混合 HDR + SDR 多屏（issue #156）：SDR 输出的矩形必须被填上，不能留黑。
    #[test]
    fn assemble_fills_every_output_including_sdr() {
        // 笔记本内屏（SDR）在左 2×2，外接屏（HDR）在右侧 2×2
        let rects = [rect(0, 0, 2, 2), rect(2, 0, 4, 2)];
        let mut canvas = vec![0u8; 4 * 2 * 4];
        assemble(&mut canvas, 4, 2, 0, 0, &rects, |i| {
            Ok(Frame {
                width: 2,
                height: 2,
                rgba: vec![if i == 0 { 10 } else { 20 }; 2 * 2 * 4],
            })
        })
        .unwrap();
        let at = |x: usize, y: usize| canvas[(y * 4 + x) * 4];
        assert_eq!(at(0, 0), 10, "SDR 屏不能被留空");
        assert_eq!(at(1, 1), 10);
        assert_eq!(at(2, 0), 20);
        assert_eq!(at(3, 1), 20);
    }

    /// 负坐标布局（副屏在左）：帧按虚拟屏原点平移后落位。
    #[test]
    fn assemble_translates_negative_layout() {
        // 虚拟屏左上角 (-2, 0)：副屏在左，主屏在右
        let rects = [rect(-2, 0, 0, 1), rect(0, 0, 2, 1)];
        let mut canvas = vec![0u8; 4 * 1 * 4];
        assemble(&mut canvas, 4, 1, -2, 0, &rects, |i| {
            Ok(Frame {
                width: 2,
                height: 1,
                rgba: vec![if i == 0 { 7 } else { 9 }; 2 * 1 * 4],
            })
        })
        .unwrap();
        let at = |x: usize| canvas[x * 4];
        assert_eq!((at(0), at(1), at(2), at(3)), (7, 7, 9, 9));
    }

    /// 任一输出取帧失败时整体失败，不静默返回黑画布。
    #[test]
    fn assemble_propagates_grab_error() {
        let rects = [rect(0, 0, 1, 1)];
        let mut canvas = vec![0u8; 1 * 1 * 4];
        let r = assemble(&mut canvas, 1, 1, 0, 0, &rects, |_| Err("抓帧失败".to_string()));
        assert!(r.is_err());
    }

    /// 混合多屏修复（issue #156）的前提：按 DXGI 输出的桌面矩形 BitBlt 能抓到该屏内容。
    /// 尺寸必须与输出矩形一致（坐标口径一致），且不能整块黑。
    #[test]
    #[ignore]
    fn sdr_output_rect_is_capturable() {
        let states = active_display_states();
        let attached = attached_outputs(&states).expect("枚举输出失败");
        assert!(!attached.is_empty(), "至少应有一个已连接输出");
        for out in &attached {
            let w = (out.rect.right - out.rect.left) as u32;
            let h = (out.rect.bottom - out.rect.top) as u32;
            let shot =
                super::super::capture_region_bitblt(out.rect.left, out.rect.top, w, h, true).expect("BitBlt 失败");
            assert_eq!((shot.width, shot.height), (w, h), "矩形口径不一致：{:?}", out.rect);
            assert!(
                shot.rgba.chunks_exact(4).any(|p| p[0] | p[1] | p[2] != 0),
                "输出矩形抓到全黑：{:?}",
                out.rect
            );
        }
    }

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

    /// 长截图的区域抓帧会话（issue #128）：建会话后应先拿到一帧真实画面。
    /// 复用会话在后续帧桌面静止时返回 None，由长截图流程覆盖。
    #[test]
    #[ignore]
    fn hdr_region_session_grabs_first_frame() {
        let _guard = CAPTURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        make_per_monitor_v2();
        let (x, y) = unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN};
            (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN))
        };
        let (w, h) = (400u32, 300u32);
        match HdrRegionSession::open(x, y, w, h) {
            None => println!("当前非 HDR 环境，长截图走 BitBlt"),
            Some(Ok(c)) => {
                let f = c.grab(true).expect("抓帧失败").expect("应拿到首帧");
                assert_eq!(f.len(), (w * h * 4) as usize);
            }
            Some(Err(e)) => panic!("建立 HDR 区域会话失败：{e}"),
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
