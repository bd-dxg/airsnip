// SPDX-License-Identifier: GPL-3.0-only
//! 外部链接（M5）：用系统默认浏览器打开 URL。
//! 用于设置面板「关于」页的项目地址与第三方许可声明入口。

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

/// 项目主页（设置面板「关于」页入口）。
pub const REPO_URL: &str = "https://github.com/bd-dxg/airsnip";

/// 第三方组件声明（设置面板「关于」页入口，见仓库根目录 `THIRD-PARTY.md`）。
pub const THIRD_PARTY_URL: &str = "https://github.com/bd-dxg/airsnip/blob/main/THIRD-PARTY.md";

/// 用默认程序打开 URL（失败时静默：链接打不开不影响主流程）。
pub fn url(url: &str) {
    let op: Vec<u16> = "open".encode_utf16().chain(std::iter::once(0)).collect();
    let file: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}
