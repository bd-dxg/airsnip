// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板的更新动作（SYS-7）：从 `interact` 拆出，保证单文件不超过 300 行。
//!
//! 网络与磁盘操作全部在后台线程，面板保持响应；进度经 `update::state` 与
//! `WM_UPDATE_STATE` 消息回传，由 `wndproc` 重绘关于页。

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

use crate::update::{self, notify, state};

use super::Panel;

impl Panel {
    /// 点击「版本更新」行：按当前状态决定检查还是下载安装。
    pub(super) fn start_update(&mut self) {
        if state::is_busy() {
            return; // 已有任务在进行中，忽略连点
        }
        if matches!(state::phase(), state::Phase::Available { .. })
            && let Some(u) = update::take_pending()
        {
            let main_hwnd = self.main_hwnd.0 as isize;
            std::thread::spawn(move || {
                // 失败时状态已写入 `Failed`，由面板重绘展示原因
                if update::download_and_install(&u).is_ok() {
                    notify::wake_ui();
                    // 给界面一点时间把「正在安装」画出来，然后退出进程让脚本替换 exe
                    std::thread::sleep(std::time::Duration::from_millis(1_500));
                    unsafe {
                        let _ = PostMessageW(Some(HWND(main_hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                } else {
                    notify::wake_ui();
                }
            });
            return;
        }
        let skipped = self.config.skipped_version.clone();
        update::check::check_in_background(skipped);
    }
}
