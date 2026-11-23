// SPDX-License-Identifier: GPL-3.0-only
//! 当前进程是否以管理员权限运行（issue #198，ADR 0048）。
//!
//! `app.manifest` 用 `highestAvailable`：管理员账户照旧自动提权，标准用户
//! （含公司电脑上被组策略限制提权的账户）以普通权限直接启动，不再被 UAC
//! 卡住。代价是普通权限下盖不住、也截不到以管理员身份运行的程序窗口
//! （UIPI，见 ADR 0003），所以启动后提示一次，自启任务也相应降低执行级别。

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// 当前进程是否处于提权（管理员）状态。
///
/// 查询失败按「未提权」处理：调用方只会因此多提示一次气泡、把自启任务降一级，
/// 两个方向都不影响运行，也偏安全。
pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let queried = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut std::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        queried && elevation.TokenIsElevated != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进程能正常查询且结果稳定。
    /// 不硬断言 `false`：`cargo test` 若在提权的终端里跑就是 `true`。
    #[test]
    fn elevation_query_is_stable() {
        assert_eq!(is_elevated(), is_elevated());
    }
}
