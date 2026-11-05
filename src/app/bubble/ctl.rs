// SPDX-License-Identifier: GPL-3.0-only
//! 文本区用的原生 `EDIT` 子控件（OCR-9）。
//!
//! 自绘文本区等于手工重造一整列系统能力：输入法候选框定位、双击选词、跨行拖选、
//! `Ctrl+V`/`Ctrl+Z`、右键菜单、长文本滚动跟随。第一版自绘方案就卡在
//! 「候选框始终落在窗口左上角」上（ADR 0022），这里直接交给系统控件。
//!
//! 代价是窗口不能再用 `WS_EX_LAYERED`（分层窗口不渲染子控件），圆角改 `SetWindowRgn`。

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CreateFontIndirectW, CreateSolidBrush, DEFAULT_CHARSET, DeleteObject, GetDC, GetTextMetricsW,
    HBRUSH, HFONT, LOGFONTW, ReleaseDC, SelectObject, SetBkColor, SetTextColor, TEXTMETRICW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{EM_GETLINECOUNT, EM_GETSEL, EM_SETREADONLY};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, ES_AUTOVSCROLL, ES_MULTILINE, ES_NOHIDESEL, ES_WANTRETURN, GWLP_WNDPROC,
    GetParent, GetWindowTextLengthW, GetWindowTextW, PostMessageW, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
    SetWindowLongPtrW, SetWindowPos, SetWindowTextW, WINDOW_STYLE, WM_CLOSE, WM_COPY, WM_KEYDOWN, WM_SETFONT, WNDPROC,
    WS_CHILD, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use crate::overlay::geometry::SelRect;
use crate::theme::Palette;

/// 子类化前的 EDIT 窗口过程。气泡是全局单实例，只有一个 EDIT，静态量足够。
static OLD_PROC: AtomicIsize = AtomicIsize::new(0);

/// 原生文本区。
pub struct Ctl {
    pub hwnd: HWND,
    font: HFONT,
    /// `WM_CTLCOLOREDIT` 返回的底色刷（与卡片底色一致）。
    brush: HBRUSH,
    /// 取不到 `GetTextMetricsW` 时的兜底行高。
    fallback_h: i32,
}

impl Ctl {
    /// 在父窗口里创建文本区（无边框、自动折行 + 垂直滚动、换行插入）。
    pub fn create(parent: HWND, size: f32, palette: &Palette) -> Option<Ctl> {
        let hinstance = unsafe { GetModuleHandleW(None) }.ok()?.into();
        let style =
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_NOHIDESEL | ES_WANTRETURN) as u32);
        let hwnd = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("EDIT"),
                w!(""),
                style,
                0,
                0,
                10,
                10,
                Some(parent),
                None,
                Some(hinstance),
                None,
            )
        }
        .ok()?;

        let font = make_font(size).unwrap_or_default();
        let bg = palette.bg;
        let brush = unsafe { CreateSolidBrush(COLORREF(rgb(bg))) };
        unsafe {
            // 换行符要经过 EDIT 自己的处理，lParam 置 1 让它立即重绘
            SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
        let old = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, subclass as *const () as isize) };
        OLD_PROC.store(old, Ordering::Release);
        Some(Ctl {
            hwnd,
            font,
            brush,
            fallback_h: (size * 1.4).round().max(1.0) as i32,
        })
    }

    /// 放到布局给定的矩形里（父窗口客户区坐标）。
    pub fn place(&self, r: SelRect) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, r.x, r.y, r.w, r.h, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    /// 设置文本。`EDIT` 内部用 CRLF 记换行，直接喂 `\n` 会被吞掉。
    pub fn set_text(&self, text: &str) {
        let crlf = text.replace("\r\n", "\n").replace('\n', "\r\n");
        let wide: Vec<u16> = crlf.encode_utf16().chain([0]).collect();
        unsafe {
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wide.as_ptr()));
        }
    }

    /// 读全文（换行统一回 `\n`，便于写剪贴板）。
    pub fn text(&self) -> String {
        let len = unsafe { GetWindowTextLengthW(self.hwnd) };
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let got = unsafe { GetWindowTextW(self.hwnd, &mut buf) };
        String::from_utf16_lossy(&buf[..got.max(0) as usize]).replace("\r\n", "\n")
    }

    /// 只读（识别中 / 错误说明）：文字仍可选中复制，但不能改。
    pub fn set_readonly(&self, on: bool) {
        unsafe {
            SendMessageW(self.hwnd, EM_SETREADONLY, Some(WPARAM(on as usize)), Some(LPARAM(0)));
        }
    }

    /// 折行后的视觉行数（高度自适应用）。
    pub fn lines(&self) -> i32 {
        unsafe { SendMessageW(self.hwnd, EM_GETLINECOUNT, None, None).0 as i32 }.max(1)
    }

    /// 单行行高（含行距），取自控件自己的字体度量。
    ///
    /// **必须先 `SelectObject` 选入 `WM_SETFONT` 那个字体**：`GetDC` 返回的 DC 里是
    /// 窗口类的默认字体，不选的话 `GetTextMetricsW` 报的是默认字体的 16px，
    /// 而 YaHei UI 16px 的真实行距是 21px —— 乘上行数会把卡片算矮两行
    /// （用户实测：识别文本完整，卡片显示不全）。
    pub fn line_h(&self) -> i32 {
        unsafe {
            let dc = GetDC(Some(self.hwnd));
            if dc.0.is_null() {
                return self.fallback_h;
            }
            let old = SelectObject(dc, self.font.into());
            let mut tm = TEXTMETRICW::default();
            let ok = GetTextMetricsW(dc, &mut tm).as_bool();
            if !old.is_invalid() {
                let _ = SelectObject(dc, old);
            }
            let _ = ReleaseDC(Some(self.hwnd), dc);
            if ok {
                (tm.tmHeight + tm.tmExternalLeading).max(1)
            } else {
                self.fallback_h
            }
        }
    }

    /// 内容所需高度：视觉行数 × 行高。
    pub fn content_height(&self) -> i32 {
        self.lines() * self.line_h()
    }

    /// 是否有选中内容。
    pub fn has_selection(&self) -> bool {
        let (mut start, mut end) = (0u32, 0u32);
        unsafe {
            SendMessageW(
                self.hwnd,
                EM_GETSEL,
                Some(WPARAM(&mut start as *mut u32 as usize)),
                Some(LPARAM(&mut end as *mut u32 as isize)),
            );
        }
        start != end
    }

    /// 让控件自己把选中内容送进剪贴板（无选中时什么也不做）。
    pub fn copy(&self) {
        unsafe {
            SendMessageW(self.hwnd, WM_COPY, None, None);
        }
    }

    /// 文本区是否持有焦点（边框高亮用）。
    pub fn focused(&self) -> bool {
        unsafe { GetFocus() == self.hwnd }
    }

    /// 底色刷（`WM_CTLCOLOREDIT` 用）。
    pub fn brush(&self) -> HBRUSH {
        self.brush
    }

    /// 把控件的前景/背景色刷成主题色（父窗口在 `WM_CTLCOLOR*` 里调用）。
    pub fn paint_colors(&self, hdc: windows::Win32::Graphics::Gdi::HDC, palette: &Palette) {
        unsafe {
            SetTextColor(hdc, COLORREF(rgb(palette.text)));
            SetBkColor(hdc, COLORREF(rgb(palette.bg)));
        }
    }
}

impl Drop for Ctl {
    fn drop(&mut self) {
        unsafe {
            if !self.font.0.is_null() {
                let _ = DeleteObject(self.font.into());
            }
            if !self.brush.0.is_null() {
                let _ = DeleteObject(self.brush.into());
            }
        }
    }
}

/// `[r, g, b, _]` → `COLORREF`（0x00BBGGRR）。
fn rgb(c: [u8; 4]) -> u32 {
    (c[2] as u32) << 16 | (c[1] as u32) << 8 | c[0] as u32
}

/// 按字号建字体（负高度 = 字符高度，与 DirectWrite 的字号语义一致）。
fn make_font(size: f32) -> Option<HFONT> {
    let mut lf = LOGFONTW {
        lfHeight: -(size.round().max(1.0) as i32),
        lfCharSet: DEFAULT_CHARSET,
        lfQuality: CLEARTYPE_QUALITY,
        ..Default::default()
    };
    let face: Vec<u16> = crate::render::text::DEFAULT_FAMILY.encode_utf16().collect();
    let n = face.len().min(lf.lfFaceName.len() - 1);
    lf.lfFaceName[..n].copy_from_slice(&face[..n]);
    let font = unsafe { CreateFontIndirectW(&lf) };
    (!font.0.is_null()).then_some(font)
}

/// 子类化：`EDIT` 自己不吃 `Esc`，得在这里转给父窗口关卡片。
unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_KEYDOWN && wparam.0 as u16 == VK_ESCAPE.0 {
        if let Ok(parent) = unsafe { GetParent(hwnd) } {
            unsafe {
                let _ = PostMessageW(Some(parent), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        return LRESULT(0);
    }
    let old: WNDPROC = unsafe { std::mem::transmute(OLD_PROC.load(Ordering::Acquire)) };
    unsafe { CallWindowProcW(old, hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    //! 真实窗口测试（需要桌面会话）：`cargo test -- --ignored`。
    //!
    //! 卡片高度自适应完全靠「`EM_GETLINECOUNT` 会把软换行算进去」这个假设，
    //! 而这一点官方文档没说清，所以用测试锁住。

    use super::*;
    use windows::Win32::UI::Controls::EM_POSFROMCHAR;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW, WS_POPUP,
    };

    /// 测试用窗口过程：直接转 `DefWindowProcW`。
    unsafe extern "system" fn test_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, w, l) }
    }

    /// 建一个隐藏父窗口（只要有个父窗口，不需要消息泵）。
    fn parent() -> HWND {
        static CLASS: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        CLASS.get_or_init(|| unsafe {
            let hinstance = GetModuleHandleW(None).unwrap().into();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(test_proc),
                hInstance: hinstance,
                lpszClassName: w!("airsnip.bubble.test"),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });
        let hinstance = unsafe { GetModuleHandleW(None) }.unwrap().into();
        unsafe {
            CreateWindowExW(
                Default::default(),
                w!("airsnip.bubble.test"),
                w!(""),
                WS_POPUP,
                0,
                0,
                600,
                600,
                None,
                None,
                Some(hinstance),
                None,
            )
            .unwrap()
        }
    }

    fn ctl_at(width: i32) -> (HWND, Ctl) {
        let parent = parent();
        let ctl = Ctl::create(parent, 16.0, &crate::theme::DARK).unwrap();
        ctl.place(SelRect {
            x: 0,
            y: 0,
            w: width,
            h: 200,
        });
        (parent, ctl)
    }

    /// **关键假设（实测踩过坑）**：`line_h()` 必须等于控件自己排版的真实行距。
    ///
    /// 之前 `GetDC` 没 `SelectObject` 选入 `WM_SETFONT` 的字体，`GetTextMetricsW` 报的
    /// 是默认字体的 16px，而 YaHei UI 16px 的真实行距是 21px：行数 × 行高把卡片算矮两行，
    /// 表现为「识别文本完整，卡片显示不全」。
    #[test]
    #[ignore = "需要桌面会话"]
    fn line_height_matches_real_layout() {
        let (parent, ctl) = ctl_at(392);
        let text = (0..6).map(|i| format!("第{i}行内容")).collect::<Vec<_>>().join("\n");
        ctl.set_text(&text);
        let lines = ctl.lines();
        assert_eq!(lines, 6, "6 行文本应报 6 行");
        // 末字符的位置高 16 位是 y：除以 (行数 - 1) 就是控件自己的行距
        let r = unsafe {
            SendMessageW(
                ctl.hwnd,
                EM_POSFROMCHAR,
                Some(WPARAM(text.encode_utf16().count())),
                Some(LPARAM(0)),
            )
        };
        let last_y = ((r.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
        let real = last_y / (lines - 1);
        assert_eq!(ctl.line_h(), real, "line_h() 与控件真实行距不一致");
        // 内容高度要能装下最后一行
        assert!(
            ctl.content_height() >= last_y + real,
            "content_height()={} 装不下最后一行（y={last_y}）",
            ctl.content_height()
        );
        unsafe {
            let _ = DestroyWindow(parent);
        }
    }

    /// 换行符往返：`\n` 进、`\n` 出（`EDIT` 内部按 CRLF 记行）。
    #[test]
    #[ignore = "需要桌面会话"]
    fn text_roundtrips_newlines() {
        let (parent, ctl) = ctl_at(300);
        ctl.set_text("第一行\n\n第三行");
        assert_eq!(ctl.text(), "第一行\n\n第三行");
        assert_eq!(ctl.lines(), 3, "显式换行 + 空行应算 3 行");
        unsafe {
            let _ = DestroyWindow(parent);
        }
    }

    /// 长文本不能被 `EDIT` 的默认长度上限截断（“复制出来的字少了”的嫌疑点）。
    #[test]
    #[ignore = "需要桌面会话"]
    fn long_text_is_not_truncated() {
        let (parent, ctl) = ctl_at(400);
        for lines in [200usize, 2000, 20000] {
            let text = (0..lines).map(|i| format!("第{i}行内容\n")).collect::<String>();
            ctl.set_text(&text);
            let back = ctl.text();
            assert_eq!(
                back.chars().count(),
                text.chars().count(),
                "{lines} 行时被截断：{} → {}",
                text.chars().count(),
                back.chars().count()
            );
        }
        unsafe {
            let _ = DestroyWindow(parent);
        }
    }

    /// **关键假设**：折行后的软换行也计入 `EM_GETLINECOUNT`，否则卡片高度不会随折行长高。
    /// 文本故意长到超过控件可视高度，确认控件内部滚动后行数仍然全量统计。
    #[test]
    #[ignore = "需要桌面会话"]
    fn line_count_tracks_soft_wraps() {
        let text = "这是一段需要折行的中文文本，用来验证卡片高度能跟着折行走。".repeat(10);
        let (p1, narrow) = ctl_at(200);
        narrow.set_text(&text);
        let (p2, wide) = ctl_at(600);
        wide.set_text(&text);
        assert!(
            narrow.lines() > 20,
            "窄宽度应折成很多行（且不受可视高度限制），实际 {}",
            narrow.lines()
        );
        assert!(
            narrow.lines() > wide.lines(),
            "窄的应比宽的行数多：{} vs {}",
            narrow.lines(),
            wide.lines()
        );
        assert!(narrow.line_h() > 0);
        unsafe {
            let _ = DestroyWindow(p1);
            let _ = DestroyWindow(p2);
        }
    }
}
