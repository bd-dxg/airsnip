// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键配置（SYS-2）：每项独立虚拟键码 + 修饰键位标志。
//! 从 `settings::mod` 拆出（原文件已接近单文件规模上限）；`mod.rs` 原样转出，
//! 外部引用 `crate::settings::HotkeyConfig` 等无需改动。

use serde::{Deserialize, Serialize};

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
        }
    }
}
