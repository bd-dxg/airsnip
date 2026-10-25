// SPDX-License-Identifier: GPL-3.0-only
//! 系统主题检测（SYS-6）+ 调色板（M3）：浅/深两套配色，
//! 覆盖层工具栏与设置面板共用；最终主题由 `resolve` 决定。

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{PCWSTR, w};

use crate::settings::ThemeSetting;

/// 系统当前主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

/// 主题调色板：界面控件配色（工具栏、设置面板共用）。
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// 面板/工具栏背景。
    pub bg: [u8; 4],
    /// 外边框。
    pub border: [u8; 4],
    /// 图标与文字主色。
    pub icon: [u8; 4],
    /// 图标与文字次级色（禁用态）。
    pub icon_dim: [u8; 4],
    /// 组分隔线。
    pub divider: [u8; 4],
    /// 强调色（选中/激活）。
    pub accent: [u8; 4],
    /// 强调色软底色（激活按钮背景）。
    pub accent_soft: [u8; 4],
    /// 色块描边。
    pub swatch_border: [u8; 4],
    /// 设置面板正文。
    pub text: [u8; 4],
    /// 设置面板次级文字（说明/描述）。
    pub text_dim: [u8; 4],
    /// 设置面板悬停行背景。
    pub row_hover: [u8; 4],
    /// 开关开（滑钮轨道）。
    pub toggle_on: [u8; 4],
    /// 开关关（轨道）。
    pub toggle_off: [u8; 4],
}

/// 深色配色（覆盖层工具栏现行风格）。
pub const DARK: Palette = Palette {
    bg: [32, 32, 32, 240],
    border: [255, 255, 255, 60],
    icon: [255, 255, 255, 235],
    icon_dim: [255, 255, 255, 70],
    divider: [255, 255, 255, 40],
    accent: [26, 115, 232, 255],
    accent_soft: [26, 115, 232, 90],
    swatch_border: [0, 0, 0, 120],
    text: [255, 255, 255, 235],
    text_dim: [150, 150, 150, 255],
    row_hover: [255, 255, 255, 18],
    toggle_on: [26, 115, 232, 255],
    toggle_off: [120, 120, 120, 255],
};

/// 浅色配色（白色底、深色字）。
pub const LIGHT: Palette = Palette {
    bg: [248, 248, 248, 246],
    border: [0, 0, 0, 45],
    icon: [40, 40, 40, 235],
    icon_dim: [40, 40, 40, 65],
    divider: [0, 0, 0, 25],
    accent: [26, 115, 232, 255],
    accent_soft: [26, 115, 232, 40],
    swatch_border: [0, 0, 0, 60],
    text: [40, 40, 40, 255],
    text_dim: [120, 120, 120, 255],
    row_hover: [0, 0, 0, 10],
    toggle_on: [26, 115, 232, 255],
    toggle_off: [180, 180, 180, 255],
};

/// 根据设置解析最终主题：跟随系统则读注册表。
pub fn resolve(setting: ThemeSetting) -> Theme {
    match setting {
        ThemeSetting::Follow => system_theme(),
        ThemeSetting::Light => Theme::Light,
        ThemeSetting::Dark => Theme::Dark,
    }
}

/// 取主题对应调色板。
pub fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Light => &LIGHT,
        Theme::Dark => &DARK,
    }
}

/// 读取 `AppsUseLightTheme`（1 = 浅色，0 = 深色）；读取失败时按深色处理。
fn system_light_theme() -> Option<i32> {
    let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = w!("AppsUseLightTheme");
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    unsafe {
        let err = RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        );
        if err == ERROR_SUCCESS && size == 4 {
            Some(value as i32)
        } else {
            None
        }
    }
}

/// 当前系统主题。
pub fn system_theme() -> Theme {
    match system_light_theme() {
        Some(1) => Theme::Light,
        _ => Theme::Dark,
    }
}

/// 判断 `WM_SETTINGCHANGE` 的 lParam 是否表示主题变更（"ImmersiveColorSet"）。
/// lParam 为 0 表示「非特定设置变更」，系统与第三方程序广播时常见；此时不能当
/// 宽字符串读：`PCWSTR::to_string` 内部走 `wcslen`，对 NULL 会访问冲突（0xC0000005）
/// 直接杀掉常驻进程 —— 登录阶段收到这种广播会让托盘图标随进程一起消失。
pub fn is_theme_change(lparam: usize) -> bool {
    if lparam == 0 {
        return false;
    }
    let ptr = PCWSTR::from_raw(lparam as *const u16);
    unsafe { ptr.to_string().is_ok_and(|s| s == "ImmersiveColorSet") }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// lParam 为 0（无设置名）时必须安全返回 false，不能解引用空指针。
    #[test]
    fn null_lparam_is_not_theme_change() {
        assert!(!is_theme_change(0));
    }

    /// 非 0 的合法字符串仍能识别主题变更。
    #[test]
    fn immersive_color_set_is_theme_change() {
        let name: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
        assert!(is_theme_change(name.as_ptr() as usize));
    }
}
