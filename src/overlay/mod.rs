// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层（CAP-2/3）：覆盖整个虚拟屏幕的无边框置顶窗口，
//! 冻结画面 + 选区暗化（预生成暗图 + 选区回贴原图），支持框选、移动与八向缩放。
//! 选区确认后进入标注模式（EDT-1/EDT-2/EDT-7）：工具栏切换工具与样式，在选区内拖拽即绘制标注。
//!
//! 文件分工：`wndproc` 消息分发、`setup` 窗口创建、`geometry` 矩形运算、`surface` 像素缓冲与上屏、
//! `cursor` 光标、`selection` 脏区重绘、`interaction` 鼠标交互、`pick` 对象选中与控制柄、
//! `objedit` 对象手柄与拖动缩放、`wheel` 滚轮调属性、`font` 字体设置与字体面板、`annotate` 标注与输出、
//! `toolbar` 工具栏、`ui` 按显示器 DPI 的界面缩放。
//! 按下优先级（ADR 0023）见 `pick::pick_target`：手柄 > 曲线控制柄 > 选区手柄 > 空格移动选区
//! > Ctrl + 对象 > 选中对象框边 > 对象内部（绘制）> 空白绘制。

mod annotate;
mod control;
mod cursor;
mod font;
pub(crate) mod geometry;
mod ime;
mod init;
mod interaction;
mod number_edit;
mod objedit;
mod pick;
mod pin_edit;
mod selection;
mod setup;
pub(crate) mod shape;
pub(crate) mod surface;
mod text_edit_state;
mod textinput;
pub(crate) mod toolbar;
pub(crate) mod ui;
mod wheel;
mod wndproc;

use tiny_skia::Pixmap;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{HBITMAP, HDC, InvalidateRect, UpdateWindow};
use windows::Win32::UI::Input::Ime::HIMC;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture::CapturedScreen;
use crate::editor::{Document, Object, Point, Tool};
use crate::theme::Palette;

use geometry::SelRect;
use interaction::{Drag, Phase};
use text_edit_state::TextEdit;

/// 覆盖层运行结果（M4 长截图视口选取复用）。#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayResult {
    /// 截图完成（已复制/保存/贴图）。
    Done,
    /// 用户取消。
    Cancelled,
    /// 长截图视口选取完成：屏幕坐标 (x, y, w, h)。
    Viewport((i32, i32, u32, u32)),
}

/// 覆盖层运行时状态（完整实现分散在各子模块的 `impl Overlay` 中）。
pub struct Overlay {
    pub capture: CapturedScreen,
    pub original: Pixmap,
    pub dimmed: Pixmap,
    pub display: Pixmap,
    /// 首帧标记：首帧 `display` 已是暗图，跳过回贴（M5 性能）。
    pub first_paint: bool,
    /// 当前主题调色板（截图开始时由设置解析，M3）。
    pub palette: &'static Palette,
    /// 界面缩放（按光标所在显示器 DPI，CAP-5）：工具栏、手柄、尺寸提示与光标随之缩放。
    pub ui: ui::Ui,
    pub selection: Option<SelRect>,
    phase: Phase,
    drag: Option<Drag>,
    /// 标注文档（对象列表 + 撤销/重做，EDT-1/EDT-2/EDT-7）。
    doc: Document,
    /// 当前工具与样式（EDT-7）。
    tool: Tool,
    /// 工具切换键配置（M3：数字键 1-5 切换标注工具）。
    tool_keys: crate::settings::ToolKeys,
    /// 长截图键（覆盖层内按下，默认 S；设置面板可改）。
    longshot_key: u32,
    /// OCR 键（覆盖层内按下，默认 T；设置面板可改，OCR-3）。
    ocr_key: u32,
    /// OCR 功能是否开启（OCR-1）：关闭时工具栏不出现 OCR 按钮、OCR 键不生效。
    ocr_enabled: bool,
    /// 贴图键（覆盖层内按下贴选区，默认 F3；设置面板可改，ADR 0019）。
    pin_key: u32,
    /// 保存偏好（OUT-2）：文件名模板、上次格式与上次目录。
    save: crate::settings::SavePrefs,
    /// 当前色块下标（0..SWATCH_COUNT，EDT-7）。
    color_index: usize,
    /// 色块颜色（EDT-7，issue #73 后续）：初值来自配置，双击色块取色改写。
    colors: [[u8; 4]; crate::editor::SWATCH_COUNT],
    /// 新建对象的默认线宽（滚轮 / 二级工具栏可连续调整）。
    line_width: f32,
    /// 新建模糊对象的默认半径（EDT-5）。
    blur_radius: f32,
    /// 新建文本对象的默认字号（EDT-4，滚轮 / 第二行可调）。
    font_size: f32,
    /// 新建序号标记的默认字号（EDT-12，滚轮可调；徽标圆随字号缩放）。
    number_font_size: f32,
    /// 新建文本对象的默认字体族、粗体、斜体（EDT-4，第二行工具栏可改）。
    font_family: std::sync::Arc<str>,
    bold: bool,
    italic: bool,
    /// 展开中的字体列表面板（EDT-4）。
    font_list: Option<toolbar::fontlist::FontPanel>,
    /// 展开中的取色面板（EDT-7，issue #73；双击色块打开）。
    color_panel: Option<toolbar::colorpanel::ColorPanel>,
    /// 取色拖动中是否已为选中对象开启撤销快照（按下时 `doc.begin`，松手归档，issue #73）。
    color_edit: bool,
    /// 取色拖动开始时选中对象的颜色（判断松手时是否真的变化）。
    color_edit_before: Option<[u8; 4]>,
    filled: bool,
    /// 正在拖拽、尚未提交的标注对象。
    draft: Option<Object>,
    /// 按下但还没决定是「画」还是「选中」的点击（ADR 0023）：对象内部与选区内空白都先记一笔。
    pending_click: Option<interaction::PendingClick>,
    /// 上次左键按下是否启动了选区内空白绘制（双击完成截图判据，M3）。
    /// 不用 `draft.is_some()`：双击第一击的草稿会在弹起时被提交/丢弃。
    pub down_started_draw: bool,
    /// 当前选中的对象索引（EDT-7）；控制柄仅在选中时显示（EDT-3）。
    selected: Option<usize>,
    /// 正在拖动的曲线控制柄：(控制点索引, 按下时的坐标)（EDT-3）。
    ctrl_drag: Option<(usize, Point)>,
    /// 正在拖动的选中对象（EDT-7 移动/缩放）。
    obj_drag: Option<objedit::ObjDrag>,
    /// 选区是否锁定（贴图编辑模式，ADR 0028）：禁止移动 / 缩放选区。
    locked_selection: bool,
    /// 贴图编辑目标句柄（ADR 0028）：完成时随结果回带给主线程写回贴图。
    pin_edit_target: Option<usize>,
    /// 文本输入中状态（EDT-4）。
    text_edit: Option<TextEdit>,
    /// 当前工具栏布局（选区存在时才有）。
    bar: Option<toolbar::Toolbar>,
    /// 悬浮提示状态（EDT-10）：悬停目标、延迟定时器与已弹气泡。
    tip: toolbar::tip::Tip,
    pub hwnd: HWND,
    /// 禁用输入法前的 IME 上下文（截图期间禁用，文本输入时恢复，EDT-4）。
    ime_old: HIMC,
    /// 主消息窗口句柄（贴图载荷/exel 完成后通知用，PIN-1）。
    pub main_hwnd: HWND,
    pub cancelled: bool,
    /// 长截图触发（M4）：工具栏按钮 / 长截图键设置屏幕坐标选区，随后覆盖层关闭。
    pub viewport: Option<(i32, i32, u32, u32)>,
    /// 上屏用内存 DC（持有 DIB section，BGRA 像素直接写入 dib_bits）。
    mem_dc: HDC,
    /// DIB section 位图句柄。
    dib_bmp: HBITMAP,
    /// DIB section 像素指针（BGRA，top-down）。
    dib_bits: *mut u8,
}

/// 覆盖层启动参数（SYS-2 / CAP-5 / OUT-2）：均为配置快照，
/// 打包传入覆盖层线程，避开 `new` / `run` 的长参数列表。
pub struct OverlayOpts {
    pub palette: &'static Palette,
    pub tool_keys: crate::settings::ToolKeys,
    /// 长截图键（默认 S，设置面板可改）。
    pub longshot_key: u32,
    /// OCR 键（默认 T，设置面板可改，OCR-3）。
    pub ocr_key: u32,
    /// OCR 是否开启（OCR-1）：关闭时工具栏不出现 OCR 按钮、OCR 键不生效。
    pub ocr_enabled: bool,
    /// 贴图键（默认 F3，设置面板可改；ADR 0019：可为鼠标键）。
    pub pin_key: u32,
    /// 本次会话的默认工具（记住上次使用，M3）。
    pub default_tool: Tool,
    /// 色块颜色（EDT-7，issue #73 后续）：来自配置，缺省为 `DEFAULT_COLORS`。
    pub colors: [[u8; 4]; crate::editor::SWATCH_COUNT],
    /// 保存偏好（OUT-2）：文件名模板、上次格式与上次目录。
    pub save: crate::settings::SavePrefs,
    /// 界面缩放（CAP-5）。
    pub ui_scale: crate::settings::UiScale,
    /// 贴图编辑模式（ADR 0028）：以贴图显示像素为底图、选区锁定为内容矩形，完成写回贴图。
    pub pin_edit: Option<PinEdit>,
}

/// 贴图编辑模式的启动参数（ADR 0028）。
pub struct PinEdit {
    /// 贴图内容在覆盖层坐标中的矩形（= 屏幕矩形 − 抓屏原点）。
    pub rect: (i32, i32, u32, u32),
    /// 贴图当前显示像素（straight RGBA，`PinWindow::export_content` 产出）。
    pub pixels: Pixmap,
    /// 目标贴图窗口句柄（跨线程传原生值；完成消息回带给主线程）。
    pub target: usize,
}

/// 显示覆盖层并阻塞运行，直到截图完成、取消或触发长截图。
/// 返回类型区分三种结果（普通截图 / 取消 / 长截图视口选取）。
pub fn run(capture: CapturedScreen, opts: OverlayOpts, hinstance: HINSTANCE, main_hwnd: HWND) -> OverlayResult {
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

    let mut overlay = match Overlay::new(capture, opts) {
        Ok(o) => o,
        Err(_) => {
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
    let raw = Box::into_raw(Box::new(overlay));

    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
        let t_pos = std::time::Instant::now();
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
        let _ = PostMessageW(Some(hwnd), wndproc::WM_FOCUS_OVERLAY, WPARAM(0), LPARAM(0));
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

    // 记录本会话最后使用的工具（下次截图默认沿用，首次为矩形，M3）
    crate::perf::log_frames_summary();
    crate::settings::last_tool_store(unsafe { (*raw).tool });

    let cancelled = unsafe { (*raw).cancelled };
    let viewport = unsafe { (*raw).viewport };
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
