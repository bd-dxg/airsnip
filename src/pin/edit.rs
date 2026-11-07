// SPDX-License-Identifier: GPL-3.0-only
//! 贴图编辑（ADR 0028，issue #56）：内容屏幕矩形、显示/隐藏、用新内容写回。
//! 从 `window` 拆出的 `PinWindow` 方法，控制单文件规模。

use tiny_skia::Pixmap;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, HWND_TOPMOST, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SetWindowPos, ShowWindow,
};

use super::border::{self, BORDER_PAD};
use super::scale;
use super::window::PinWindow;

impl PinWindow {
    /// 内容区在屏幕上的矩形 (x, y, w, h)：窗口矩形去掉边框扩展（贴图编辑用）。
    pub(super) fn content_screen_rect(&self) -> (i32, i32, u32, u32) {
        let rect = unsafe {
            let mut r = RECT::default();
            let _ = GetWindowRect(self.hwnd(), &mut r);
            r
        };
        let off = (self.pad as f32 * self.scale()).round() as i32;
        let w = (self.scaled.width() as i32 - off * 2).max(1) as u32;
        let h = (self.scaled.height() as i32 - off * 2).max(1) as u32;
        (rect.left + off, rect.top + off, w, h)
    }

    /// 显示 / 隐藏贴图（编辑期间隐藏）。
    pub(super) fn set_visible(&self, visible: bool) {
        unsafe {
            let _ = ShowWindow(self.hwnd(), if visible { SW_SHOW } else { SW_HIDE });
            if visible {
                let _ = SetWindowPos(
                    self.hwnd(),
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    /// 用新内容替换贴图（贴图编辑完成）：内容左上角屏幕位置不变，
    /// 缩放复位为 1，按原边框设置重新烘焙并上屏。
    pub(super) fn replace_content(&mut self, pixmap: Pixmap) -> Result<(), String> {
        let (cx, cy, _, _) = self.content_screen_rect();
        let show_border = self.pad > 0;
        let data = scale::premultiply_rgba(pixmap.data());
        let Some(size) = tiny_skia::IntSize::from_wh(pixmap.width(), pixmap.height()) else {
            return Err("贴图内容尺寸非法".into());
        };
        let raw = Pixmap::from_vec(data, size).ok_or("贴图内容尺寸非法")?;
        let framed = border::with_border(&raw, show_border).ok_or("贴图边框合成失败")?;
        self.pad = if show_border { BORDER_PAD } else { 0 };
        self.raw = raw;
        self.scaled = framed.clone();
        self.original = framed;
        let (w, h) = (self.scaled.width() as i32, self.scaled.height() as i32);
        let off = self.pad as i32; // 缩放已复位为 1
        unsafe {
            let _ = SetWindowPos(
                self.hwnd(),
                Some(HWND_TOPMOST),
                cx - off,
                cy - off,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
        self.rebind()
    }
}
