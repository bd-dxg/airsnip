// SPDX-License-Identifier: GPL-3.0-only
//! 入口：初始化应用壳并进入消息循环。

// release 版为 GUI 子系统（双击不弹黑色控制台窗口）；debug 版保留控制台，
// 便于查看 `perf.rs` 的埋点输出。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app;
pub mod capture;
// 跨平台核心（ADR 0034）：editor 已搬到 crates/airsnip-core，这里重导出，
// 全仓库既有的 `crate::editor::…` 路径保持不变。
pub use airsnip_core::editor;
pub mod longshot;
pub mod ocr; // 二期
pub mod output;
pub mod overlay;
pub mod perf;
pub mod pin;
pub mod render;
pub mod settings;
pub mod theme;
pub mod update;

fn main() {
    std::process::exit(app::run());
}
