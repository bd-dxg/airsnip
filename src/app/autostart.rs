// SPDX-License-Identifier: GPL-3.0-only
//! 开机自启（SYS-4）：任务计划 ONLOGON + 与当前权限一致的最高执行级别（见 ADR 0003 / 0048）。
//! 用任务计划而不是 HKCU Run 键：需要提权的程序 Run 键启不起来。

use windows::Win32::System::LibraryLoader::GetModuleFileNameW;

/// 任务计划名称。
const TASK_NAME: &str = "airsnip";

/// 开机自启是否启用（SYS-4）：任务计划中存在 "airsnip" 任务。
pub fn autostart_enabled() -> bool {
    run_schtasks(&["/Query", "/TN", TASK_NAME])
}

/// 设置开机自启（SYS-4）：创建/删除登录时运行的任务计划。
/// 返回是否成功（通常因权限不足失败，调用方负责提示）。
///
/// 执行级别跟随当前进程（ADR 0048）：已提权时 `/RL HIGHEST`，否则 `/RL LIMITED`。
/// 标准用户建不了 HIGHEST 任务（schtasks 直接拒绝），而降级任务本身也拿不到
/// 更高权限，所以普通权限下只有 LIMITED 有意义。
pub fn set_autostart(enabled: bool) -> bool {
    if enabled {
        let path = exe_path_quoted();
        let level = if crate::app::elevation::is_elevated() {
            "HIGHEST"
        } else {
            "LIMITED"
        };
        run_schtasks(&[
            "/Create", "/F", "/TN", TASK_NAME, "/TR", &path, "/SC", "ONLOGON", "/RL", level,
        ])
    } else {
        run_schtasks(&["/Delete", "/F", "/TN", TASK_NAME])
    }
}

/// 调用 schtasks.exe 并返回是否成功（隐藏控制台窗口）。
fn run_schtasks(args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("schtasks")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 当前可执行文件路径（带引号）。
fn exe_path_quoted() -> String {
    unsafe {
        let mut buf = [0u16; 1024];
        let len = GetModuleFileNameW(None, &mut buf) as usize;
        let path = String::from_utf16_lossy(&buf[..len]);
        format!("\"{path}\"")
    }
}
