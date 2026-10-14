// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏绘制：把工具栏渲染到覆盖层像素图。

use tiny_skia::{Pixmap, Rect, Transform};

use crate::editor::{PALETTE, Tool};
use crate::overlay::SelRect;
use crate::overlay::toolbar::{Action, FONT_BTN_W, GAP, H, H2, State, Toolbar};
use crate::render;
use crate::render::text::FontSpec;
use crate::theme::Palette;

use super::icons::paint_of as paint;
use super::icons::*;
use super::label::{LABEL_SIZE, centered_text, divider, fit_name, left_text};

/// 绘制工具栏到像素图。
pub fn render(pixmap: &mut Pixmap, bar: &Toolbar, state: &State, p: &Palette) {
    rounded_rect(pixmap, bar.rect, 7.0, p.bg, Some(p.border));

    // 组分隔线：画在每个组首个元素左侧的空白处
    // 组划分（hits 索引）：[工具 0-5][颜色 6-10][填充 11 圆角 12][撤销 13 重做 14]
    // [贴图 15 长截图 16][复制 17 保存 18]
    for &index in &[6usize, 11, 13, 15, 17] {
        let Some((first, _)) = bar.hits.get(index) else {
            continue;
        };
        divider(pixmap, first.x - GAP / 2 - 1, bar.first.y + 7, H - 14, p);
    }
    // 第二行分隔线（EDT-4）：字体按钮之后、斜体重之后
    if let Some(second) = bar.second {
        for action in [Action::ToggleBold, Action::FontSmaller] {
            if let Some((r, _)) = bar.hits.iter().find(|(_, a)| *a == action) {
                divider(pixmap, r.x - GAP / 2 - 1, second.y + 7, H2 - 14, p);
            }
        }
    }

    for (r, action) in &bar.hits {
        match *action {
            Action::Tool(tool) => {
                let active = tool == state.tool;
                if active {
                    rounded_rect(pixmap, *r, 5.0, p.accent_soft, None);
                }
                match tool {
                    Tool::Rect => {
                        let icon = rect_icon(*r, 0.0, false, if active { p.icon } else { p.icon_dim });
                        render::draw_object(pixmap, &icon);
                    }
                    Tool::Arrow => {
                        let icon = arrow_icon(*r, if active { p.icon } else { p.icon_dim });
                        render::draw_object(pixmap, &icon);
                    }
                    // 文本图标：字母 T（EDT-4）
                    Tool::Text => text_icon(pixmap, *r, if active { p.icon } else { p.icon_dim }),
                    // 高亮图标固定荧光黄（激活时更浓）
                    Tool::Highlight => highlight_icon(pixmap, *r, if active { 230 } else { 150 }),
                    Tool::Blur => blur_icon(pixmap, *r, if active { p.icon } else { p.icon_dim }),
                    Tool::Glow => glow_icon(pixmap, *r, if active { p.icon } else { p.icon_dim }),
                }
            }
            Action::Color(i) => {
                let inset = 2;
                let swatch = SelRect {
                    x: r.x + inset,
                    y: r.y + inset,
                    w: r.w - inset * 2,
                    h: r.h - inset * 2,
                };
                rounded_rect(pixmap, swatch, 4.0, PALETTE[i], Some(p.swatch_border));
                if i == state.color {
                    rounded_rect_outline(pixmap, *r, 4.0, p.accent);
                }
            }
            Action::ToggleFill => {
                // 文本工具无背景功能：填充开关置灰禁用（同圆角）
                let enabled = state.tool != Tool::Text;
                if enabled && state.filled {
                    rounded_rect(pixmap, *r, 5.0, p.accent_soft, None);
                }
                let icon = rect_icon(*r, 0.0, state.filled, if enabled { p.icon } else { p.icon_dim });
                render::draw_object(pixmap, &icon);
            }
            Action::ToggleRound => {
                // 文本工具下圆角开关无意义：置灰禁用
                let enabled = state.tool != Tool::Text;
                if enabled && state.round {
                    rounded_rect(pixmap, *r, 5.0, p.accent_soft, None);
                }
                let color = if enabled { p.icon } else { p.icon_dim };
                let icon = rect_icon(*r, 6.0, false, color);
                render::draw_object(pixmap, &icon);
            }
            Action::Undo => draw_undo_icon(pixmap, *r, if state.can_undo { p.icon } else { p.icon_dim }, false),
            Action::Redo => draw_undo_icon(pixmap, *r, if state.can_redo { p.icon } else { p.icon_dim }, true),
            // PIN-1：贴图（图钉）
            Action::Pin => pin_icon(pixmap, *r, p.icon),
            // M4：长截图（向下展开的页面 + 箭头）
            Action::Longshot => longshot_icon(pixmap, *r, p.icon),
            // OUT-1：复制（两张叠放的纸）
            Action::Copy => copy_icon(pixmap, *r, p.icon),
            // OUT-2：保存（软盘）
            Action::Save => save_icon(pixmap, *r, p.icon),
            // EDT-4：字体族按钮（展开/收起字体列表），名字过长截断加省略号
            Action::FontPick => {
                if state.font_open {
                    rounded_rect(pixmap, *r, 5.0, p.accent_soft, None);
                }
                let spec = FontSpec::new(LABEL_SIZE);
                let color = if state.font_open { p.accent } else { p.text };
                let name = fit_name(&state.font_family, (FONT_BTN_W - 12) as f32, &spec);
                left_text(pixmap, *r, 6, &name, &spec, color);
            }
            Action::ToggleBold => {
                let spec = FontSpec {
                    bold: true,
                    ..FontSpec::new(LABEL_SIZE + 1.0)
                };
                toggle_label(pixmap, *r, "B", &spec, state.bold, p);
            }
            Action::ToggleItalic => {
                let spec = FontSpec {
                    italic: true,
                    ..FontSpec::new(LABEL_SIZE + 1.0)
                };
                toggle_label(pixmap, *r, "I", &spec, state.italic, p);
            }
            // 字号：− 数值 +（每个元素各占一格）
            Action::FontSmaller => glyph_icon(pixmap, *r, Glyph::Minus, p.icon),
            Action::FontLarger => glyph_icon(pixmap, *r, Glyph::Plus, p.icon),
            Action::FontSize => {
                let spec = FontSpec::new(LABEL_SIZE);
                let text = format!("{}", state.font_size.round() as i32);
                centered_text(pixmap, *r, &text, &spec, p.text);
            }
        }
    }
}

/// B / I 开关按钮：开启时强调底色 + 强调色文字。
fn toggle_label(pixmap: &mut Pixmap, r: SelRect, text: &str, spec: &FontSpec, on: bool, p: &Palette) {
    if on {
        rounded_rect(pixmap, r, 5.0, p.accent_soft, None);
    }
    let color = if on { p.accent } else { p.icon };
    centered_text(pixmap, r, text, spec, color);
}

/// 字号 − / + 图形（不依赖字体，用矩形拼）。
enum Glyph {
    Minus,
    Plus,
}

fn glyph_icon(pixmap: &mut Pixmap, r: SelRect, glyph: Glyph, color: [u8; 4]) {
    let (cx, cy) = (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0);
    let len = 10.0;
    let thick = 1.6;
    if let Some(rect) = Rect::from_xywh(cx - len / 2.0, cy - thick / 2.0, len, thick) {
        pixmap.fill_rect(rect, &paint(color), Transform::identity(), None);
    }
    if matches!(glyph, Glyph::Plus)
        && let Some(rect) = Rect::from_xywh(cx - thick / 2.0, cy - len / 2.0, thick, len)
    {
        pixmap.fill_rect(rect, &paint(color), Transform::identity(), None);
    }
}
