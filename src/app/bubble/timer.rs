// SPDX-License-Identifier: GPL-3.0-only
//! 气泡的定时器：复制成功的 0.8s 反馈。
//!
//! **短命定时器**：到时立刻 `KillTimer`，不会在空闲期留下任何轮询
//! （待机成本与卡片关闭时一致）。光标闪烁已交回原生 `EDIT`，不需要定时器。

use windows::Win32::UI::WindowsAndMessaging::SetTimer;

use super::Bubble;

impl Bubble {
    /// 显示「已复制」反馈（一次性定时器，不引入常驻轮询）。
    pub fn start_copied_timer(&self) {
        unsafe {
            let _ = SetTimer(Some(self.hwnd), super::TIMER_COPIED, super::COPIED_MS, None);
        }
    }
}
