// SPDX-License-Identifier: GPL-3.0-only
//! 「变量说明」浮层（OUT-2 / ADR 0041）：列出全部可用变量与含义。
//! 表格与替换器共用 `output::quick::VARS` 一张表，加一个变量不需要改两处。
//! 浮层贴「变量说明」按钮下方、右对齐内容区右缘，塞不下翻到上方。

use tiny_skia::Pixmap;

use crate::output::quick::VARS;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::render::text as dwrite;

use super::super::Panel;
use super::super::controls::{Item, RowId, W};
use super::super::draw::draw_text;

/// 「变量说明」浮层的矩形：贴在按钮下方，右对齐内容区右缘，塞不下翻到上方。
pub(super) fn help_rect(panel: &Panel) -> Option<SelRect> {
    let layout = panel.layout();
    let anchor = layout.items.iter().find_map(|it| match it {
        Item::Row(r) if r.id == RowId::VarHelp => Some(r.rect),
        _ => None,
    })?;
    let s = panel.scale;
    let size = 14.0 * s;
    let spec = dwrite::FontSpec::new(size);
    let mut w: f32 = 0.0;
    for (name, desc) in VARS {
        let line = format!("{name}    {}", desc.text());
        w = w.max(dwrite::measure(&line, &spec).0);
    }
    let line_h = (20.0 * s) as i32;
    let pad = (12.0 * s) as i32;
    let bw = (w as i32 + pad * 2).min((W as f32 * s) as i32 - 2 * pad);
    let bh = line_h * VARS.len() as i32 + pad * 2;
    let right = anchor.x + anchor.w;
    let bottom_limit = layout.height - pad;
    let y = if anchor.y + anchor.h + bh <= bottom_limit {
        anchor.y + anchor.h
    } else {
        (anchor.y - bh).max(pad)
    };
    Some(SelRect {
        x: (right - bw).max(pad),
        y,
        w: bw,
        h: bh,
    })
}

/// 画「变量说明」浮层（画在所有行之上，避免被后续行覆盖）。
pub(in crate::settings::panel) fn draw_help(pixmap: &mut Pixmap, panel: &Panel) {
    if !panel.var_help {
        return;
    }
    let Some(rect) = help_rect(panel) else {
        return;
    };
    let p = panel.palette;
    let s = panel.scale;
    rounded_rect(
        pixmap,
        rect,
        (8.0 * s) as f32,
        [p.bg[0], p.bg[1], p.bg[2], 255],
        Some(p.border),
    );
    let size = 14.0 * s;
    let pad = (12.0 * s) as i32;
    let line_h = (20.0 * s) as i32;
    for (i, (name, desc)) in VARS.iter().enumerate() {
        let line = format!("{name}    {}", desc.text());
        let x = rect.x + pad;
        let y = rect.y + pad + i as i32 * line_h;
        draw_text(pixmap, x as f32, y as f32, &line, size, p.text);
    }
}
