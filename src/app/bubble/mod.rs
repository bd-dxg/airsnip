// SPDX-License-Identifier: GPL-3.0-only
//! 文字气泡（OCR-7～OCR-10）：取字结果的独立小窗口。
//!
//! 与截图覆盖层的线程模型不同：气泡跑在**主线程**的消息循环里，所以覆盖层关闭后它仍然存在
//! （OCR-10），用户可以继续校对、改错字、复制。全局单实例——再次取字时复用同一个窗口。
//!
//! 分工：`layout` 几何、`ctl` 原生 `EDIT` 文本区、`view` 状态切换与绘制上屏、
//! `draw` 纯绘制、`interact` 键鼠交互、`wnd` 窗口与消息。
//!
//! 文本区是系统 `EDIT` 控件（见 `ctl`），所以窗口不能是分层窗口：
//! 圆角改 `SetWindowRgn`，上屏改 `BitBlt`。

mod create;
mod ctl;
mod draw;
mod interact;
mod layout;
mod timer;
mod view;
mod wnd;

use std::sync::atomic::{AtomicIsize, Ordering};

use tiny_skia::Pixmap;
use windows::Win32::Foundation::{GlobalFree, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{HBITMAP, HDC};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::overlay::ui::Ui;
use crate::theme::Palette;

use ctl::Ctl;
use layout::{Layout, layout};

/// 识别完成（后台线程 → 气泡窗口）：
/// wparam = HGLOBAL(`[u32 text_len][u32 lang_len][text][lang]`)，lparam = 0 结果 / 1 错误说明 / 2 载荷失败。
pub(super) const WM_OCR_DONE: u32 = WM_APP + 1;

/// 「已复制」反馈的定时器（一次性，回调里立刻 KillTimer）。
pub(super) const TIMER_COPIED: usize = 0x8A02;
/// 「已复制」显示多久（毫秒）。
pub(super) const COPIED_MS: u32 = 800;

/// 当前气泡窗口（全局单实例，OCR-7）。
static CURRENT: AtomicIsize = AtomicIsize::new(0);

/// 气泡内容状态（OCR-8）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// 识别中：正文显示「识别中…」。
    Loading,
    /// 已有结果（文本可能为空 = 未识别到文字）。
    Ready,
    /// 失败：正文是错误说明。
    Error,
}

/// 打开气泡的配置快照。
pub struct Opts {
    /// 识别语言：`None` 跟随系统首选（OCR-6）。
    pub language: Option<String>,
    /// 识别成功后是否自动复制（OCR-10）。
    pub auto_copy: bool,
    /// 主窗口句柄：切开关时通知它写回配置。
    pub main_hwnd: HWND,
    pub palette: &'static Palette,
    pub ui: Ui,
}

/// 气泡窗口状态（挂在窗口 `GWLP_USERDATA` 上）。
pub struct Bubble {
    pub hwnd: HWND,
    pub ui: Ui,
    pub palette: &'static Palette,
    pub state: State,
    /// 文本区（原生 `EDIT` 子控件）。
    pub ctl: Ctl,
    pub layout: Layout,
    /// 刚复制完成（复制按钮短暂显示「已复制」）。
    pub copied: bool,
    /// 图钉点亮：窗口置顶（不被其他窗口遮挡）。
    pub pinned: bool,
    /// 识别成功后自动写剪贴板（与配置同步，默认关）。
    pub auto_copy: bool,
    /// 主窗口句柄：切开关时通知它写回配置。
    main_hwnd: HWND,
    /// 悬停目标（图标的高亮反馈）。
    hover: Option<interact::Hit>,
    /// 文本区最大高度（像素）：按选区所在显示器的工作区算，防止卡片溢出屏幕。
    pub max_body: i32,
    /// 程序正在写文本区：此时 `EN_CHANGE` 是自家人为的，不要重复重排。
    pub syncing: bool,
    /// 上屏缓冲。
    dib_bmp: HBITMAP,
    mem_dc: HDC,
    dib_bits: *mut u8,
}

impl Drop for Bubble {
    /// 窗口销毁时必须释放上屏缓冲：`apply_size` 只释放「上一次」的，
    /// 最后一份会随窗口一起泄漏（每个 DIB section + 内存 DC）。
    fn drop(&mut self) {
        self.free_surface();
    }
}

/// 收到覆盖层的取字请求（OCR-7）：解析载荷、弹气泡、后台识别。
pub fn on_request(hg: HGLOBAL, opts: Opts) {
    let Some((pixmap, pos)) = take_payload(hg) else {
        return;
    };
    let Some(hwnd) = create::ensure_window(opts.palette, opts.ui) else {
        return;
    };
    let Some(bubble) = (unsafe { create::state_of(hwnd) }) else {
        return;
    };
    bubble.auto_copy = opts.auto_copy;
    bubble.main_hwnd = opts.main_hwnd;
    // 卡片高度上限按选区所在显示器的工作区算：靠屏幕下方时不能溢出（OCR-7）
    bubble.max_body = max_body(bubble.ui, pos);
    bubble.set_loading();
    create::place_at(hwnd, bubble, pos);
    bubble.paint();
    create::focus(bubble);
    // 后台识别（OCR-12）：结果用自定义消息回投到气泡窗口
    let raw = hwnd.0 as usize;
    std::thread::spawn(move || {
        // 调试：`AIRSNIP_DEBUG=1` 时把送给引擎的原图与识别结果落盘，
        // 用于分辨「图裁错了」还是「引擎漏了」（与 `longshot/session.rs` 同一做法）
        if std::env::var_os("AIRSNIP_DEBUG").is_some() {
            let dir = std::env::temp_dir();
            let _ = pixmap.save_png(dir.join("airsnip-ocr-input.png"));
        }
        let (code, body, lang) = match crate::ocr::recognize(&pixmap, opts.language.as_deref()) {
            Ok(r) => (0isize, r.text, r.language),
            Err(e) => (1isize, e.message(), String::new()),
        };
        if std::env::var_os("AIRSNIP_DEBUG").is_some() {
            let _ = std::fs::write(std::env::temp_dir().join("airsnip-ocr-text.txt"), &body);
        }
        let Ok(hg) = alloc_text(&body, &lang) else {
            // 内存不足：也要回一帧，否则卡片永远停在「识别中…」
            unsafe {
                let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_OCR_DONE, WPARAM(0), LPARAM(2));
            }
            return;
        };
        unsafe {
            let _ = PostMessageW(
                Some(HWND(raw as *mut _)),
                WM_OCR_DONE,
                WPARAM(hg.0 as usize),
                LPARAM(code),
            );
        }
    });
}

/// 解析取字载荷：`[u32 w][u32 h][i32 x][i32 y][RGBA]`，返回像素图与选区屏幕坐标。
fn take_payload(hg: HGLOBAL) -> Option<(Pixmap, (i32, i32, i32, i32))> {
    unsafe {
        let ptr = GlobalLock(hg);
        if ptr.is_null() {
            let _ = GlobalFree(Some(hg));
            return None;
        }
        let hp = ptr as *const u32;
        let (w, h) = (*hp, *hp.add(1));
        let sp = hp.add(2) as *const i32;
        let (x, y) = (*sp, *sp.add(1));
        let len = w as usize * h as usize * 4;
        let rgba = std::slice::from_raw_parts((ptr as *const u8).add(16), len).to_vec();
        let _ = GlobalUnlock(hg);
        let _ = GlobalFree(Some(hg));
        let size = tiny_skia::IntSize::from_wh(w, h)?;
        let pixmap = Pixmap::from_vec(rgba, size)?;
        Some((pixmap, (x, y, w as i32, h as i32)))
    }
}

/// 把文本 + 语言标签放进可跨线程传递的全局内存：`[u32 text_len][u32 lang_len][text][lang]`。
/// 带长度前缀是因为 `GlobalSize` 返回分配尺寸（可能大于实际内容），
/// 直接按它读会把尾部未初始化的字节也当成文本。
fn alloc_text(text: &str, lang: &str) -> Result<HGLOBAL, ()> {
    let (tb, lb) = (text.as_bytes(), lang.as_bytes());
    unsafe {
        let hg = GlobalAlloc(GMEM_MOVEABLE, 8 + tb.len() + lb.len()).map_err(|_| ())?;
        let ptr = GlobalLock(hg);
        if ptr.is_null() {
            let _ = GlobalFree(Some(hg));
            return Err(());
        }
        *(ptr as *mut u32) = tb.len() as u32;
        *((ptr as *mut u32).add(1)) = lb.len() as u32;
        let body = (ptr as *mut u8).add(8);
        std::ptr::copy_nonoverlapping(tb.as_ptr(), body, tb.len());
        std::ptr::copy_nonoverlapping(lb.as_ptr(), body.add(tb.len()), lb.len());
        let _ = GlobalUnlock(hg);
        Ok(hg)
    }
}

/// 取出全局内存里的（文本, 语言标签）并释放。
pub(super) fn take_text(hg: HGLOBAL) -> (String, String) {
    unsafe {
        let ptr = GlobalLock(hg);
        if ptr.is_null() {
            let _ = GlobalFree(Some(hg));
            return (String::new(), String::new());
        }
        let hp = ptr as *const u32;
        let (tl, ll) = (*hp as usize, *hp.add(1) as usize);
        let body = (ptr as *const u8).add(8);
        let text = String::from_utf8_lossy(std::slice::from_raw_parts(body, tl)).to_string();
        let lang = String::from_utf8_lossy(std::slice::from_raw_parts(body.add(tl), ll)).to_string();
        let _ = GlobalUnlock(hg);
        let _ = GlobalFree(Some(hg));
        (text, lang)
    }
}

/// 气泡窗口句柄（未打开时为 `None`）。
pub(super) fn current() -> Option<HWND> {
    let v = CURRENT.load(Ordering::Acquire);
    (v != 0).then(|| HWND(v as *mut _))
}

pub(super) fn set_current(hwnd: HWND) {
    CURRENT.store(hwnd.0 as isize, Ordering::Release);
}

/// 清空单实例指针（窗口销毁时）。
pub(super) fn clear_current() {
    CURRENT.store(0, Ordering::Release);
}

/// 正文磅值（随 `ui` 缩放）：文本区字体按它创建。
pub(super) fn body_size(ui: Ui) -> f32 {
    ui.f(draw::BODY_SIZE_BASE)
}

/// 文本区高度上限：工作区高度减去卡片外壳（图标行 + 动作行 + 内边距）。
fn max_body(ui: Ui, pos: (i32, i32, i32, i32)) -> i32 {
    let probe = layout(ui, 0);
    let chrome = probe.h - probe.body.h;
    let work = create::work_area(pos.0 + pos.2 / 2, pos.1 + pos.3 / 2);
    let avail = work.bottom - work.top - chrome - ui.px(8.0) * 2;
    avail.max(probe.body.h)
}
