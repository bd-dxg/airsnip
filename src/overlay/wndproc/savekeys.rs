// SPDX-License-Identifier: GPL-3.0-only
//! 保存 / 快速保存的按键分派（OUT-2 / ADR 0041）：覆盖层内**带修饰键**的组合键。
//!
//! 独立成模块的两个理由：一是 `wndproc` 已接近单文件规模上限；二是这两件事要在
//! `WM_KEYDOWN` 与 `WM_SYSKEYDOWN`（Alt 组合）两条路径上共用同一份判定 ——
//! 分开写迟早一条改了一条没改（`Alt+S` 与 `Ctrl+S` 就会有一边失效）。
//!
//! 判定顺序很关键：**组合键先于无修饰键的覆盖层键**（长截图 `S`、工具键、`F`/`R`），
//! 否则 `Shift+S` / `Ctrl+S` 会被「长截图 S」抢走。

use super::Overlay;
use super::quicksave::combo_match;
use super::{ctrl_down, shift_down, win_down};
use windows::Win32::Foundation::LPARAM;

/// 快速保存提示可见期间的消息闸门（OUT-2 / ADR 0041）：本次截图已经落盘，
/// 除「点提示定位」与「`Esc` / `Enter` / `Alt` 收尾」之外**任何输入都不再进入标注交互**——
/// 否则滚轮会改线宽并在提示上重绘、右键会弹「连按两次」确认、`F10` 之类的系统键
/// 还能在「已保存待收尾」的状态里把长截图起起来。
///
/// 只拦「输入类消息」：绘制 / 定时器 / 关闭 / 焦点等一律放行，
/// 免得写成一个白名单后把某条生命周期消息也吞掉。返回 `true` 表示已消费。
pub(super) fn toast_gate(overlay: &mut Overlay, msg: u32, lparam: LPARAM) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
        WM_RBUTTONUP, WM_SYSKEYDOWN,
    };
    if !overlay.toast_visible() {
        return false;
    }
    match msg {
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            // 点提示 = 在资源管理器里定位文件；点其它 = 直接收尾
            overlay.finish_quick(overlay.toast_hit(x, y));
            true
        }
        // 按键仍交给 `handle`（它决定 `Esc` / `Enter` / `Alt` 收尾、其余吞掉）
        WM_KEYDOWN | WM_SYSKEYDOWN => false,
        // 其余输入一律吞掉
        WM_MOUSEMOVE | WM_MOUSEWHEEL | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONUP => true,
        _ => false,
    }
}

/// 处理一次按键。返回 `true` 表示已消费（调用方不再继续分发）。
/// `alt` 由调用方按消息来源给出：`WM_KEYDOWN` 恒 `false`，`WM_SYSKEYDOWN` 取 lparam bit 29。
pub(super) fn handle(overlay: &mut Overlay, vk: u32, alt: bool) -> bool {
    // 快速保存的提示还挂在屏幕上：本次截图已经存好，只接受「收尾」——
    // `Esc` / `Enter` / 单独按 `Alt`（`VK_MENU`）关窗，其余按键一律吞掉
    // （不再改标注、不切工具，也不会把 `Alt+S` 当成第二次保存）。
    if overlay.toast_visible() {
        if vk == 0x1B || vk == 0x0D || vk == 0x12 {
            overlay.finish_quick(false);
        }
        return true;
    }
    // 贴图编辑模式（ADR 0028）：选区锁定，输出组整体不适用
    if overlay.locked_selection {
        return false;
    }
    let (c, sh, w) = (ctrl_down(), shift_down(), win_down());
    if combo_match(overlay.quick.key, overlay.quick.modifiers, vk, c, sh, alt, w) {
        overlay.on_quick_save();
        return true;
    }
    if combo_match(overlay.quick.save_key, overlay.quick.save_modifiers, vk, c, sh, alt, w) {
        overlay.on_save();
        return true;
    }
    false
}
