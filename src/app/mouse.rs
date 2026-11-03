// SPDX-License-Identifier: GPL-3.0-only
//! 鼠标按键热键（SYS-2）：中键 / 侧键 1 / 侧键 2 的全局监听。
//!
//! `RegisterHotKey` 只认键盘虚拟键码，鼠标键改由 Raw Input 订阅（ADR 0019）：
//! 只注册鼠标设备并投递到隐藏消息窗口，不抢占其他程序、不需要常驻钩子线程；
//! 无鼠标输入时没有消息，待机开销仍为 0（移动鼠标时每个输入批次一条 WM_INPUT）。

use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK,
    RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONWARNING, MB_OK, RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_5_DOWN, RI_MOUSE_MIDDLE_BUTTON_DOWN, WM_MBUTTONDOWN,
    WM_XBUTTONDOWN,
};

use super::{App, OVERLAY_RUNNING, message_box};

/// 鼠标键的虚拟键码：沿用 winuser.h 的 `VK_MBUTTON` / `VK_XBUTTON1` / `VK_XBUTTON2` 编号。
/// 0x04–0x06 在键盘虚拟键码的空档里，配置中直接存这三个值即可。
pub const VK_MBUTTON: u32 = 0x04;
pub const VK_XBUTTON1: u32 = 0x05;
pub const VK_XBUTTON2: u32 = 0x06;

/// 是否为鼠标键：这类键不能交给 `RegisterHotKey`，由 Raw Input 分发。
pub fn is_mouse_key(vk: u32) -> bool {
    matches!(vk, VK_MBUTTON | VK_XBUTTON1 | VK_XBUTTON2)
}

/// 配置里是否有需要全局监听的鼠标热键（截图 / 贴图）。
/// 贴图边框键在贴图窗口内处理，不依赖 Raw Input，不在此列。
pub fn wanted(hk: &crate::settings::HotkeyConfig) -> bool {
    is_mouse_key(hk.screenshot_key) || is_mouse_key(hk.pin_key)
}

/// 鼠标按下消息 → 虚拟键码（设置面板 / 覆盖层 / 贴图窗口共用）。
/// 非鼠标消息返回 None。
pub fn message_vk(msg: u32, wparam: usize) -> Option<u32> {
    match msg {
        WM_MBUTTONDOWN => Some(VK_MBUTTON),
        // wParam 高位：XBUTTON1 = 1、XBUTTON2 = 2
        WM_XBUTTONDOWN => Some(if (wparam >> 16) & 0xFFFF == 1 {
            VK_XBUTTON1
        } else {
            VK_XBUTTON2
        }),
        _ => None,
    }
}

/// `AIRSNIP_DEBUG` 只读一次：`WM_INPUT` 是热路径，每次按键查环境变量不划算。
pub fn debug_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("AIRSNIP_DEBUG").is_some())
}

/// 注册全局鼠标输入（Raw Input）。失败只影响鼠标热键，不阻塞启动；
/// 但配置里确实用到鼠标热键时必须告诉用户，否则表现为「按了没反应」。
pub fn install(hwnd: HWND, wanted: bool) {
    let devices = [RAWINPUTDEVICE {
        usUsagePage: 0x01,        // Generic Desktop Controls
        usUsage: 0x02,            // Mouse
        dwFlags: RIDEV_INPUTSINK, // 窗口不在前台也接收
        hwndTarget: hwnd,
    }];
    let ok = unsafe { RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32) }.is_ok();
    if debug_enabled() {
        eprintln!(
            "[airsnip] Raw Input 注册{}（鼠标热键）",
            if ok { "成功" } else { "失败" }
        );
    }
    if !ok && wanted {
        message_box(
            Some(hwnd),
            "无法注册鼠标输入，鼠标热键不会生效。\n请在设置中改用键盘热键。",
            MB_OK | MB_ICONWARNING,
        );
    }
}

/// 从 `WM_INPUT` 解析刚按下的鼠标键；移动事件与抬起事件返回 None。
pub fn pressed_vk(lparam: LPARAM) -> Option<u32> {
    let mut raw = RAWINPUT::default();
    let mut size = std::mem::size_of::<RAWINPUT>() as u32;
    let n = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as *mut core::ffi::c_void),
            RID_INPUT,
            Some(&mut raw as *mut RAWINPUT as *mut core::ffi::c_void),
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if n == 0 || n == u32::MAX {
        return None;
    }
    // 只看按键标志（低 16 位）：鼠标移动每帧都有 WM_INPUT，在这里直接丢弃
    let flags = unsafe { raw.data.mouse.Anonymous.ulButtons & 0xFFFF };
    if flags & RI_MOUSE_MIDDLE_BUTTON_DOWN != 0 {
        Some(VK_MBUTTON)
    } else if flags & RI_MOUSE_BUTTON_4_DOWN != 0 {
        Some(VK_XBUTTON1)
    } else if flags & RI_MOUSE_BUTTON_5_DOWN != 0 {
        Some(VK_XBUTTON2)
    } else {
        None
    }
}

impl App {
    /// 鼠标键热键分发：与键盘热键同一套配置，只比对虚拟键码
    /// （鼠标+修饰键的组合不实用，修饰键不参与匹配，见 ADR 0019）。
    pub fn handle_mouse_hotkey(&self, vk: u32) {
        // 覆盖层运行期间不响应全局热键：Raw Input 照样投递到主窗口，
        // 不挡的话按鼠标截图键会再起一份覆盖层（每份 = 一份全屏位图）。
        if OVERLAY_RUNNING.load(Ordering::Acquire) {
            return;
        }
        let hk = &self.config.hotkey;
        if hk.screenshot_key == vk {
            self.start_capture();
        } else if hk.pin_key == vk {
            crate::pin::from_clipboard(self.hinstance, self.config.pin_position, self.config.pin_border);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三个鼠标键都能被识别，键盘码不受影响（配置里两类键混存）。
    #[test]
    fn mouse_keys_are_distinguished_from_keyboard() {
        assert!(is_mouse_key(VK_MBUTTON));
        assert!(is_mouse_key(VK_XBUTTON1));
        assert!(is_mouse_key(VK_XBUTTON2));
        assert!(!is_mouse_key(0x70), "F1 不是鼠标键");
        assert!(!is_mouse_key(crate::settings::UNBOUND), "未绑定不是鼠标键");
        // 不能与现有热键默认值撞码
        let hk = crate::settings::HotkeyConfig::default();
        for vk in [hk.screenshot_key, hk.pin_key, hk.border_key, hk.longshot_key] {
            assert!(!is_mouse_key(vk), "{vk:#x} 与鼠标键区间冲突");
        }
        assert!(!wanted(&hk), "默认配置全是键盘键，不需要全局鼠标监听");
    }

    /// `WM_XBUTTONDOWN` 靠 wParam 高位区分两个侧键，写错就永远只有侧键 1。
    #[test]
    fn xbutton_hiword_picks_the_right_side_key() {
        let x = |hi: usize| WM_XBUTTONDOWN as usize | (hi << 16);
        assert_eq!(message_vk(WM_XBUTTONDOWN, x(1)), Some(VK_XBUTTON1));
        assert_eq!(message_vk(WM_XBUTTONDOWN, x(2)), Some(VK_XBUTTON2));
        assert_eq!(message_vk(WM_MBUTTONDOWN, WM_MBUTTONDOWN as usize), Some(VK_MBUTTON));
        assert_eq!(message_vk(0x0201, 0), None, "非鼠标消息不应产出键码");
    }
}
