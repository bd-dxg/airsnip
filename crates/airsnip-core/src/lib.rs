// SPDX-License-Identifier: GPL-3.0-only
//! AirSnip 跨平台核心（ADR 0034）：与操作系统无关的标注数据模型。
//!
//! 只放「纯数据 + 纯算法」——对象模型、文档撤销栈、曲线几何、颜色调色板、
//! 命中测试、变换约束。**不得**引入任何平台 API（Windows / Wayland / X11），
//! 也不得依赖渲染后端：渲染由各平台前端自行实现，核心只提供几何与语义。
//!
//! Windows 前端（仓库根 crate `airsnip`）通过 `pub use airsnip_core::editor;`
//! 保持既有 `crate::editor::…` 路径不变；Linux 前端（`crates/airsnip-linux`）
//! 直接 `use airsnip_core::editor::…`。

pub mod draw;
pub mod editor;
