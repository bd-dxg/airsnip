// SPDX-License-Identifier: GPL-3.0-only
//! 长截图（LNG-1/3/4/5/6/7/10/11）：框选滚动视口 → 用户手动滚动 → 条带拼接 → 输出。
//!
//! 模块分工：`stitch` 条带匹配纯函数、`rows` 行缓冲画布、`engine` 拼接状态机、
//! `session` 会话状态机、`window` 执行窗口、`bar` 状态条绘制、`present` 上屏、
//! `preview` 拼接结果预览窗。

mod bar;
mod border;
mod debug;
mod engine;
mod events;
mod present;
mod preview;
mod rows;
mod session;
mod stitch;
mod window;

use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK};

/// 长截图主流程：创建执行窗口阻塞运行，完成后复制到剪贴板并保存。
/// 保存目标：用户点过「另存为」则写到所选路径；否则自动存到默认目录（PNG）。
/// 两种情况都会复制到剪贴板（OUT-1）。
pub fn run(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    hinstance: HINSTANCE,
    max_height: u32,
    palette: &'static crate::theme::Palette,
    ui_scale: crate::settings::UiScale,
    save: crate::settings::SavePrefs,
) {
    let Some(outcome) = window::run(x, y, w, h, hinstance, max_height, palette, ui_scale, save) else {
        return; // 取消或失败（窗口内已提示）
    };
    let window::Outcome { pixmap, save_as } = outcome;
    // 输出：复制 + 保存（与普通截图 Enter / Ctrl+S 行为一致，OUT-1/OUT-2）
    std::thread::spawn(move || {
        if let Err(e) = crate::output::copy_to_clipboard(&pixmap) {
            crate::app::message_box(None, &format!("复制失败：{e}"), MB_OK | MB_ICONERROR);
        }
        // 另存为：用户已选好路径与格式，不再额外往默认目录写一份
        let path = match save_as {
            Some(path) => path,
            None => {
                let dir = crate::output::default_save_dir();
                let _ = std::fs::create_dir_all(&dir);
                dir.join(crate::output::timestamped_filename("png"))
            }
        };
        if let Err(e) = crate::output::save_file(&pixmap, &path) {
            crate::app::message_box(None, &format!("保存失败：{e}"), MB_OK | MB_ICONERROR);
        }
    });
}
