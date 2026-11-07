// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键配置（SYS-2）：每项独立虚拟键码 + 修饰键位标志。
//! 从 `settings::mod` 拆出（原文件已接近单文件规模上限）；`mod.rs` 原样转出，
//! 外部引用 `crate::settings::HotkeyConfig` 等无需改动。

use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW};

/// 未绑定语义：虚拟键码为 0 表示「该项不绑定任何键」（SYS-2）。
/// 旧配置里不存在 0 值，只在用户显式清空时出现，回退默认逻辑不受影响。
pub const UNBOUND: u32 = 0;

/// 热键配置：每键独立虚拟键码 + 修饰键位标志（0 = 无修饰键，SYS-2 需提示风险）。
/// 截图与贴图各自持有修饰键，互不影响（M3 修复历史共享缺陷）。
/// 虚拟键码 `UNBOUND`（0）表示该项被用户清空，注册层跳过、显示为「未设置」。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// 截图热键虚拟键码（默认 F1 = 0x70）
    pub screenshot_key: u32,
    /// 截图热键修饰键位标志（MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN）
    pub screenshot_modifiers: u32,
    /// 贴图热键虚拟键码（默认 F3 = 0x72）
    pub pin_key: u32,
    /// 贴图热键修饰键位标志
    pub pin_modifiers: u32,
    /// 贴图边框开关热键（默认 Ctrl+F3；`UNBOUND` = 未启用）
    pub border_key: u32,
    /// 贴图边框开关热键修饰键位标志
    pub border_modifiers: u32,
    /// 长截图键虚拟键码（截图覆盖层内按下，默认 S = 0x53；无修饰键，与工具切换键同组；
    /// `UNBOUND` = 未启用）
    pub longshot_key: u32,
    /// OCR 键虚拟键码（截图覆盖层内按下，默认 T = 0x54；无修饰键，与长截图键同组；
    /// `UNBOUND` = 未启用，OCR-3）
    pub ocr_key: u32,
    /// 遗留字段（旧版本两键共享修饰键，TOML 键名仍为 `modifiers`）：仅加载迁移用，不再写入。
    #[serde(default, skip_serializing, rename = "modifiers")]
    pub legacy_modifiers: u32,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            screenshot_key: 0x70,
            screenshot_modifiers: 0,
            pin_key: 0x72,
            pin_modifiers: 0,
            border_key: 0x72,    // Ctrl+F3
            border_modifiers: 2, // MOD_CONTROL
            longshot_key: 0x53,  // S
            ocr_key: 0x54,       // T
            legacy_modifiers: 0,
        }
    }
}

/// 工具切换键（M3）：覆盖层内按数字键切换标注工具，每工具一个虚拟键码；
/// `UNBOUND`（0）= 该项不绑定（SYS-2）。
type ToolKey = u32;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolKeys {
    pub rect: ToolKey,
    pub arrow: ToolKey,
    pub text: ToolKey,
    pub blur: ToolKey,
    pub highlight: ToolKey,
    pub glow: ToolKey,
    /// EDT-12 序号标记。
    pub number: ToolKey,
}

/// 修饰键位掩码（与 `HotkeyConfig` 的 `*_modifiers` 一致）。
pub const MOD_ALT_BIT: u32 = 1 << 0;
pub const MOD_CTRL_BIT: u32 = 1 << 1;
pub const MOD_SHIFT_BIT: u32 = 1 << 2;
pub const MOD_WIN_BIT: u32 = 1 << 3;

/// 虚拟键 → 显示名（如 F1、Ctrl+C）。未绑定（0）显示为「未设置」。
/// 定义放在配置模块与键码同源：设置面板键帽与截图控件悬浮提示（EDT-10）都要用，
/// 不该让覆盖层反向依赖设置面板的内部布局模块。
pub fn key_name(vk: u32, mods: u32) -> String {
    if vk == UNBOUND {
        return "未设置".into();
    }
    let mut s = String::new();
    if mods & MOD_CTRL_BIT != 0 {
        s += "Ctrl+";
    }
    if mods & MOD_ALT_BIT != 0 {
        s += "Alt+";
    }
    if mods & MOD_SHIFT_BIT != 0 {
        s += "Shift+";
    }
    if mods & MOD_WIN_BIT != 0 {
        s += "Win+";
    }
    s += &vk_name(vk);
    s
}

/// 虚拟键 → 键名。
pub fn vk_name(vk: u32) -> String {
    match vk {
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x2E => "Del".into(),
        0x08 => "Backspace".into(),
        0x14 => "CapsLock".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x21 => "PgUp".into(),
        0x22 => "PgDn".into(),
        0x2D => "Insert".into(),
        0x90 => "NumLock".into(),
        0x91 => "ScrollLock".into(),
        // 少数紧凑键盘把 PrtScn 位置上报为 VK_PAUSE（#67）
        0x13 => "Pause".into(),
        // 鼠标键（ADR 0019）：沿用 winuser.h 的 VK_MBUTTON / VK_XBUTTON* 编号
        0x04 => "鼠标中键".into(),
        0x05 => "鼠标侧键1".into(),
        0x06 => "鼠标侧键2".into(),
        // PrintScreen：无对应字符，直接给键名（#67）
        0x2C => "PrtScn".into(),
        0x27 => "→".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x28 => "↓".into(),
        // 常规字符键：MapVirtualKeyW 取键名（组合键 Shift+ 时字符可能大写/符号）
        _ => {
            let ch = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) } & 0xFFFF;
            if ch != 0 && (ch as u8).is_ascii_graphic() {
                (ch as u8 as char).to_uppercase().to_string()
            } else {
                format!("键 {vk:#x}")
            }
        }
    }
}

impl Default for ToolKeys {
    fn default() -> Self {
        Self {
            rect: 0x31,      // 1
            arrow: 0x32,     // 2
            text: 0x33,      // 3
            blur: 0x34,      // 4
            highlight: 0x35, // 5
            glow: 0x36,      // 6
            number: 0x37,    // 7
        }
    }
}
