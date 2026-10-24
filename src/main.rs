// SPDX-License-Identifier: GPL-3.0-only
//! 入口：初始化应用壳并进入消息循环。

// release 版为 GUI 子系统（双击不弹黑色控制台窗口）；debug 版保留控制台，
// 便于查看 `perf.rs` 的埋点输出。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod capture;
pub mod editor;
pub mod longshot;
pub mod ocr; // 二期
pub mod output;
pub mod overlay;
pub mod perf;
pub mod pin;
pub mod render;
pub mod settings;
pub mod theme;

fn main() {
    std::process::exit(app::run());
}
