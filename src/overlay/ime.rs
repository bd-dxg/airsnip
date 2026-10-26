// SPDX-License-Identifier: GPL-3.0-only
//! 输入法开关（EDT-4）：截图覆盖层默认禁用 IME，仅文本输入期间恢复。
//!
//! 不禁用的话，中文输入法活动时按 `S`（长截图）/ 回车等键会被 IME 截走
//! （转为 `VK_PROCESSKEY` 并启动拼音组合），覆盖层快捷键全部失效。

use windows::Win32::UI::Input::Ime::{HIMC, ImmAssociateContext, ImmCreateContext};

use super::Overlay;

impl Overlay {
    /// 输入法开关：`true` 恢复关联的 IME 上下文（文本输入），`false` 断开（默认态）。
    pub(super) fn set_ime_enabled(&mut self, on: bool) {
        unsafe {
            if on {
                // 窗口原本就没关联上下文时新建一个，否则文本工具无法输入中文
                let ctx = if self.ime_old.0.is_null() {
                    ImmCreateContext()
                } else {
                    self.ime_old
                };
                if !ctx.0.is_null() {
                    let _ = ImmAssociateContext(self.hwnd, ctx);
                }
            } else {
                let old = ImmAssociateContext(self.hwnd, HIMC(std::ptr::null_mut()));
                if !old.0.is_null() {
                    self.ime_old = old;
                }
            }
        }
    }
}
