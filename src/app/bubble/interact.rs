// SPDX-License-Identifier: GPL-3.0-only
//! 气泡交互（OCR-9/OCR-10）：按钮命中与点击。
//!
//! 文本区是原生 `EDIT` 子控件，选中、编辑、右键菜单、滚动全由系统处理，
//! 这里只剩卡片外壳上的四个热区（图钉 / 关闭 / 复制 / 自动复制）。
//! 文本区以外的空白处按左键拖动窗口（卡片没有标题栏）。

use super::Bubble;
use super::view::contains;

/// 鼠标按下的落点（也用作悬停目标）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    Pin,
    Close,
    /// 左下角「复制」按钮。
    Copy,
    /// 右下角「自动复制」开关（整块可点）。
    AutoCopy,
    /// 其他（含文本区外的空白）：按下即拖动窗口。
    Other,
}

impl Bubble {
    /// 命中测试。
    pub(super) fn hit(&self, x: i32, y: i32) -> Hit {
        if self.hit_pin(x, y) {
            Hit::Pin
        } else if self.hit_close(x, y) {
            Hit::Close
        } else if contains(self.layout.copy, x, y) {
            Hit::Copy
        } else if contains(self.layout.auto_copy, x, y) {
            Hit::AutoCopy
        } else {
            Hit::Other
        }
    }

    /// 按下：图钉 / 关闭 / 复制 / 自动复制开关。返回 `true` 表示窗口需要重绘。
    pub(super) fn on_mouse_down(&mut self, x: i32, y: i32) -> bool {
        match self.hit(x, y) {
            Hit::Pin => {
                self.toggle_pin();
                true
            }
            Hit::Close => {
                self.request_close();
                false
            }
            Hit::AutoCopy => {
                self.toggle_auto_copy();
                true
            }
            Hit::Copy => {
                // 识别中 / 错误说明不该被当成“结果”复制走
                if self.state == super::State::Ready && self.copy() {
                    self.copied = true;
                    self.start_copied_timer();
                }
                true
            }
            Hit::Other => false,
        }
    }

    /// 离开窗口：清掉悬停态。
    pub(super) fn clear_hover(&mut self) -> bool {
        if self.hover.is_none() {
            return false;
        }
        self.hover = None;
        true
    }

    /// 投递关闭（走消息循环，避免在窗口过程里直接销毁自己）。
    pub(super) fn request_close(&self) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(self.hwnd),
                windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}
