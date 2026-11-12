// SPDX-License-Identifier: GPL-3.0-only
//! Linux 前端的绘制层：文本引擎 + （复用 core 的）对象绘制入口。
//!
//! 对象 → 像素的绘制代码在 `airsnip_core::draw`（ADR 0034），这里只提供
//! cosmic-text 版的 `TextSink` 与几个便捷函数。

pub mod text;

use tiny_skia::Pixmap;

use airsnip_core::draw::objects;
use airsnip_core::editor::Object;

use crate::canvas::Canvas;

/// 像素区域（x0, y0, x1, y1 半开区间）。
pub type Clip = (i32, i32, i32, i32);

/// 绘制对象列表（`draft` 为正在拖拽的草稿）。
pub fn draw_objects(pixmap: &mut Pixmap, objs: &[Object], draft: Option<&Object>, clip: Clip) {
    text::with_engine(|sink| objects::draw_objects(pixmap, objs, draft, None, clip, sink));
}

/// 导出烘焙：把对象栅格化进裁剪图（含聚光效果）。
pub fn bake(pixmap: &mut Pixmap, objs: &[Object], origin: (i32, i32)) {
    text::with_engine(|sink| airsnip_core::draw::bake::bake_objects(pixmap, objs, origin, sink));
}

/// `Canvas`（RGBA8888，直通 alpha）→ `Pixmap`（预乘）。画布恒为不透明，逐字节相同。
pub fn canvas_to_pixmap(canvas: &Canvas) -> Option<Pixmap> {
    let mut p = Pixmap::new(canvas.w, canvas.h)?;
    p.data_mut().copy_from_slice(&canvas.px);
    Some(p)
}

/// `Pixmap` → `Canvas`（导出用）。
pub fn pixmap_to_canvas(p: &Pixmap) -> Canvas {
    let mut c = Canvas::new(p.width(), p.height());
    c.px.copy_from_slice(p.data());
    c
}
