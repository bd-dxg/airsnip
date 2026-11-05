// SPDX-License-Identifier: GPL-3.0-only
//! 气泡视图（OCR-8/OCR-9/OCR-10）：内容状态切换、高度自适应、绘制上屏、命中测试与复制。
//!
//! 从 `mod` 拆出：那里只留状态定义、取字请求入口与跨线程载荷，保证单文件不超 300 行。
//!
//! 上屏走 `GetDC` + `BitBlt`（普通窗口，内容全不透明）。圆角由 `SetWindowRgn` 裁掉，
//! 因为文本区是原生 `EDIT` 子控件，分层窗口（`UpdateLayeredWindow`）渲染不了子控件。

use tiny_skia::Pixmap;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateRoundRectRgn, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, ReleaseDC, SRCCOPY, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

use super::draw;
use super::interact::Hit;
use super::layout::{RADIUS_BASE, layout, layout_capped};
use super::{Bubble, State};
use crate::overlay::geometry::SelRect;
use crate::overlay::surface::create_dib_surface;

impl Bubble {
    /// 切到「识别中」。
    ///
    /// **置顶与自动复制开关是跨取字保留的用户偏好**，不在这里重置。
    pub fn set_loading(&mut self) {
        self.state = State::Loading;
        self.copied = false;
        self.sync_text("识别中…", true);
        self.relayout();
    }

    /// 填入识别结果；空文本按「未识别到文字」处理（OCR-8）。
    ///
    /// 自动复制开关打开时才写剪贴板（OCR-10）。卡片**不自动关闭**：
    /// 什么时候关由用户决定（Esc / ✕）。
    pub fn set_result(&mut self, text: String, language: String) {
        if text.trim().is_empty() {
            self.set_error(&format!(
                "未识别到文字（当前语言：{}）。\n可以试着缩小选区、避开图标与图片，或换一种识别语言。",
                crate::ocr::language_name(&language)
            ));
            return;
        }
        self.state = State::Ready;
        self.sync_text(&text, false);
        self.relayout();
        if self.auto_copy {
            // 自动复制：失败不弹窗（用户还能点「复制」或 Ctrl+C）
            if crate::output::copy_text_to_clipboard(&self.ctl.text()).is_ok() {
                self.copied = true;
                self.start_copied_timer();
            }
        }
    }

    /// 填入错误说明（就地展示，不弹系统对话框，OCR-8）。
    pub fn set_error(&mut self, message: &str) {
        self.state = State::Error;
        self.copied = false;
        self.sync_text(message, true);
        self.relayout();
    }

    /// 程序写文本区：期间屏蔽 `EN_CHANGE`，避免重复重排（重排由调用方做一次）。
    fn sync_text(&mut self, text: &str, readonly: bool) {
        self.syncing = true;
        self.ctl.set_readonly(readonly);
        self.ctl.set_text(text);
        self.syncing = false;
    }

    /// 切换「自动复制」并通知主窗口写回配置。
    pub fn toggle_auto_copy(&mut self) {
        self.auto_copy = !self.auto_copy;
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(self.main_hwnd),
                crate::app::WM_OCR_AUTOCOPY_CHANGED,
                windows::Win32::Foundation::WPARAM(self.auto_copy as usize),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }

    /// 按当前文本重算高度并重建窗口。
    ///
    /// 文本区宽度只取决于卡片宽度（与内容高度无关），所以可以「先把宽度定下来，
    /// 再让 EDIT 自己折行数行数」——不需要自己实现折行。
    pub fn relayout(&mut self) {
        let probe = layout(self.ui, 0);
        self.ctl.place(probe.body);
        let content_h = self.ctl.content_height();
        self.layout = layout_capped(self.ui, content_h, self.max_body);
        self.apply_size();
        self.ctl.place(self.layout.body);
    }

    /// 按布局尺寸重建 DIB、调整窗口大小与圆角区域（保持左上角不动）。
    fn apply_size(&mut self) {
        let (w, h) = (self.layout.w, self.layout.h);
        self.free_surface();
        if let Ok((dc, bmp, bits)) = create_dib_surface(w as u32, h as u32) {
            self.mem_dc = dc;
            self.dib_bmp = bmp;
            self.dib_bits = bits;
        }
        if self.hwnd.0.is_null() {
            return;
        }
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            // 圆角：SetWindowRgn 接管 region 所有权，之后不能自己 DeleteObject
            let r = self.ui.px(RADIUS_BASE);
            let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, r * 2, r * 2);
            if !rgn.0.is_null() {
                let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            }
        }
    }

    /// 释放上屏缓冲（重建与 `Drop` 共用）。
    pub(super) fn free_surface(&mut self) {
        unsafe {
            if !self.dib_bmp.0.is_null() {
                let _ = DeleteObject(self.dib_bmp.into());
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
        self.dib_bmp = HBITMAP::default();
        self.mem_dc = HDC::default();
        self.dib_bits = std::ptr::null_mut();
    }

    /// 绘制并上屏。
    pub fn paint(&mut self) {
        let (w, h) = (self.layout.w, self.layout.h);
        if self.dib_bits.is_null() {
            return;
        }
        let Some(mut pixmap) = Pixmap::new(w as u32, h as u32) else {
            return;
        };
        draw::render(&mut pixmap, self, &self.layout);
        // tiny-skia 出 RGBA，DIB 要 BGRA（内容不透明，alpha 不参与合成）
        let src = pixmap.data();
        unsafe {
            for i in 0..w as usize * h as usize {
                let s = src.as_ptr().add(i * 4);
                let d = self.dib_bits.add(i * 4);
                *d = *s.add(2);
                *d.add(1) = *s.add(1);
                *d.add(2) = *s;
                *d.add(3) = *s.add(3);
            }
        }
        self.present(w, h);
    }

    /// 把 DIB 拷到窗口（`WS_CLIPCHILDREN` 保证不会盖到文本区上）。
    fn present(&self, w: i32, h: i32) {
        unsafe {
            let dc = GetDC(Some(self.hwnd));
            if dc.0.is_null() {
                return;
            }
            let _ = BitBlt(dc, 0, 0, w, h, Some(self.mem_dc), 0, 0, SRCCOPY);
            let _ = ReleaseDC(Some(self.hwnd), dc);
        }
    }

    /// 命中测试：点是否落在图钉 / 关闭 / 复制 / 自动复制开关上。
    pub fn hit_pin(&self, x: i32, y: i32) -> bool {
        contains(self.layout.pin, x, y)
    }

    pub fn hit_close(&self, x: i32, y: i32) -> bool {
        contains(self.layout.close, x, y)
    }

    /// 切换图钉：点亮后窗口置顶（对照粘贴时不被遮挡）。
    pub fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
        let z = if self.pinned { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(z), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// 更新悬停目标（返回是否需要重绘）。
    pub fn update_hover(&mut self, x: i32, y: i32) -> bool {
        let hover = match self.hit(x, y) {
            Hit::Pin => Some(Hit::Pin),
            Hit::Close => Some(Hit::Close),
            Hit::Copy => Some(Hit::Copy),
            Hit::AutoCopy => Some(Hit::AutoCopy),
            Hit::Other => None,
        };
        if hover == self.hover {
            return false;
        }
        self.hover = hover;
        true
    }

    /// 复制到剪贴板（OCR-10：只写剪贴板，不关窗口）。有选区复制选区，否则复制全文。
    ///
    /// 返回是否真的复制了内容（按钮的「已复制」反馈用）。
    pub fn copy(&self) -> bool {
        if self.ctl.has_selection() {
            self.ctl.copy();
            return true;
        }
        let text = self.ctl.text();
        if text.is_empty() {
            return false;
        }
        if let Err(e) = crate::output::copy_text_to_clipboard(&text) {
            crate::app::message_box(
                None,
                &format!("复制失败：{e}"),
                windows::Win32::UI::WindowsAndMessaging::MB_OK,
            );
            return false;
        }
        true
    }
}

/// 矩形命中测试（`interact` 也要用）。
pub(super) fn contains(r: SelRect, x: i32, y: i32) -> bool {
    x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
}
