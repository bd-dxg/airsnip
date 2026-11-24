// SPDX-License-Identifier: GPL-3.0-only
//! 外部链接（M5）：用系统默认浏览器打开 URL。
//! 用于设置面板「关于」页的项目地址与第三方许可声明入口。

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

/// 项目主页（设置面板「关于」页入口）。
pub const REPO_URL: &str = "https://github.com/bd-dxg/airsnip";

/// 官网（设置面板「关于」页入口）：唯一官方下载渠道，永久免费。
/// 与 README「下载与安装」一节的说明对应，用于让拿到第三方打包版的用户能对照出官方版本。
pub const HOMEPAGE_URL: &str = "https://airsnip.bddxg.top";

/// 第三方组件声明（设置面板「关于」页入口，见仓库根目录 `THIRD-PARTY.md`）。
pub const THIRD_PARTY_URL: &str = "https://github.com/bd-dxg/airsnip/blob/main/THIRD-PARTY.md";

/// 新建反馈 issue（设置面板「关于」行「我要反馈」按钮，SYS-8）：先复制系统信息、
/// 2 秒后打开模板选择页——反馈不一定是 bug，也可能是功能建议，所以不直达某个表单。
pub const NEW_ISSUE_URL: &str = "https://github.com/bd-dxg/airsnip/issues/new/choose";

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
