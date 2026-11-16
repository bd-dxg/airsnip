// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层主循环（SYS-2）：建窗 → 显示 → 局部消息循环 → 归还结果。
//! 从 `overlay::mod` 拆出，控制单文件规模；类型与字段仍在父模块。

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, UpdateWindow};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::geometry::SelRect;
use super::{Overlay, OverlayOpts, OverlayResult, recapture};

/// 显示覆盖层并阻塞运行，直到截图完成、取消或触发长截图。
/// 返回类型区分三种结果（普通截图 / 取消 / 长截图视口选取）。
pub fn run(
    capture: crate::capture::CapturedScreen,
    opts: OverlayOpts,
    hinstance: HINSTANCE,
    main_hwnd: HWND,
) -> OverlayResult {
    let t_show = std::time::Instant::now();
    let (origin_x, origin_y) = (capture.origin_x, capture.origin_y);
    let (vw, vh) = (capture.width as i32, capture.height as i32);
    let t_win = std::time::Instant::now();
    let hwnd = match Overlay::create_window(hinstance) {
        Ok(h) => h,
        Err(e) => {
            crate::app::message_box(None, &e, MB_OK | MB_ICONERROR);
            return OverlayResult::Cancelled;
        }
    };

    // 再次截图键（issue #118）：覆盖层线程自己注册，覆盖层关闭 / 线程退出即失效。
    // 贴图编辑模式（ADR 0028）选区锁死，没有「重新框选」这回事，不注册。
    if opts.pin_edit.is_none() {
        recapture::register(hwnd, opts.recapture_key, opts.recapture_modifiers);
    }

    let mut overlay = match Overlay::new(capture, opts) {
        Ok(o) => o,
        Err(_) => {
            recapture::unregister(hwnd);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return OverlayResult::Cancelled;
        }
    };
    let t_init_done = std::time::Instant::now();
    overlay.hwnd = hwnd;
    // 截图期间禁用输入法：否则 S（长截图）/ 回车等按键被 IME 截走；
    // 进入文本输入时再恢复（见 `ime::set_ime_enabled`，EDT-4）。
    overlay.set_ime_enabled(false);
    overlay.main_hwnd = main_hwnd;
    // 窗口吸附（CAP-6 / ADR 0037）：**显示前**就把光标下的候选画进 `display`（不上屏）。
    // 否则首帧先上一次全暗（`UpdateWindow`），紧接着吸附再把候选改成半亮，
    // 用户会看到「明亮 → 极暗 → 半亮」两步闪烁。
    overlay.prime_snap_at_cursor();
    // 控件级捕捉（ADR 0039）：worker 的 COM 初始化与预热查询放在显示前，
    // 与淡入动画重叠，不占用户第一次悬停的时间。
    if overlay.control_snap && !overlay.locked_selection {
        overlay.spawn_uia_worker();
    }
    let raw = Box::into_raw(Box::new(overlay));

    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
        let t_pos = std::time::Instant::now();
        // 淡入（CAP-2 显示动画）：先把窗口做成「整窗 alpha 0」，再显示并推 alpha，
        // 120ms 后自动摘掉分层样式回到纯 GDI（见 `fade`）。
        (*raw).begin_fade();
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), origin_x, origin_y, vw, vh, SWP_SHOWWINDOW);
        let pos = t_pos.elapsed();
        // 贴图编辑模式（ADR 0028）：初始选区由 seed 设定，主动画一帧（选区装饰 + 工具栏 + HUD），
        // 否则首帧只会上传底图（普通截图的工具栏由用户框选触发重绘）
        if (*raw).locked_selection {
            (*raw).refresh_toolbar();
            (*raw).repaint(SelRect {
                x: 0,
                y: 0,
                w: vw,
                h: vh,
            });
        }
        // 显示后强制整窗重绘：隐藏窗口的 DC 内容在显示时会丢失，
        // 用 InvalidateRect + UpdateWindow 同步触发 WM_PAINT 全窗绘制，避免闪烁。
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = UpdateWindow(hwnd);
        let paint = t_pos.elapsed() - pos;
        // 前台与焦点改为异步（消息循环处理）：同步调用会阻塞至 ~45ms（M5 性能）
        let _ = PostMessageW(Some(hwnd), super::wndproc::WM_FOCUS_OVERLAY, WPARAM(0), LPARAM(0));
        if crate::perf::enabled() {
            eprintln!(
                "[airsnip] 显示分解：建窗+初始化 {:?}，SetWindowPos {pos:?}，首帧 {paint:?}，前台+焦点 {:?}",
                t_init_done - t_win,
                t_pos.elapsed() - pos - paint
            );
        }
        if std::env::var_os("AIRSNIP_TIMING").is_some() {
            eprintln!("[airsnip] 覆盖层创建到显示完成 {:?}", t_show.elapsed());
        }
        // M5 性能剖析：热键 → 可见总耗时（含抓屏、线程创建与首帧绘制）
        crate::perf::log_overlay_shown();
    }

    // 局部消息循环：WM_DESTROY 由 wndproc 调 PostQuitMessage(0) 结束（独立线程，
    // 不影响主线程消息队列）。退出后 `Box::from_raw` 释放 Overlay 及其 4 份全屏位图。
    unsafe {
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 || msg.message == WM_QUIT {
                break;
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
            if msg.hwnd == hwnd && msg.message == WM_DESTROY {
                break;
            }
        }
    }

    // 记录本会话最后使用的工具：ADR 0036 后不再记忆（每次截图都从移动态开始）
    crate::perf::log_frames_summary();

    let cancelled = unsafe { (*raw).cancelled };
    let viewport = unsafe { (*raw).viewport };
    recapture::unregister(hwnd);
    unsafe {
        let _ = Box::from_raw(raw);
        let _ = DestroyWindow(hwnd);
    }
    if cancelled {
        OverlayResult::Cancelled
    } else if let Some(vp) = viewport {
        OverlayResult::Viewport(vp)
    } else {
        OverlayResult::Done
    }
}
