// SPDX-License-Identifier: GPL-3.0-only
//! 纯 CPU 光栅绘制工具（tiny_skia）：两个前端共用，与平台 API 无关。
//!
//! 这里放的是「跟具体窗口系统无关、只跟像素打交道」的部分——矢量描边、
//! 盒式模糊、圆角遮罩、SVG 图标解析。平台文本引擎（Windows 的 DirectWrite /
//! Linux 的 cosmic-text）**不在这里**，各自前端负责。
//!
//! ADR 0034 §2 的边界据此放宽一条：core 允许依赖 tiny-skia（纯软件光栅、
//! 跨平台字节一致），但**不得**依赖任何平台 API 或平台文本渲染。

pub mod bake;
pub mod blur;
pub mod icon;
pub mod mask;
pub mod objects;
pub mod region;
pub mod stroke;
pub mod text;

use tiny_skia::Paint;

use crate::editor::Style;

/// 由 RGBA 构造 tiny-skia 画笔（描边与填充共用）。
pub fn paint_of(color: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint
}

/// 样式里「描边色 + 线宽」的快捷读取（各绘制函数共用，免得每处都写两行）。
pub fn stroke_style(style: &Style) -> ([u8; 4], f32) {
    (style.color, style.width)
}
