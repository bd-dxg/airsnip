// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏绘制：把工具栏渲染到覆盖层像素图。
//! 按钮图标统一由 `crate::render::icon` 绘制（资源见 `assets/icons/ui/`），
//! 这里只负责背景、选中态与文字类控件。

use tiny_skia::Pixmap;

use crate::editor::Tool;
use crate::overlay::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::overlay::toolbar::{Action, State, Toolbar};
use crate::overlay::ui::Ui;
use crate::render::icon::{self, Icon};
use crate::render::text::FontSpec;
use crate::theme::Palette;

use super::label::{LABEL_SIZE_BASE, centered_text, divider, fit_name, left_text};

/// 选中色块光晕的层数（由内向外每层扩 1 个基准像素）。
const GLOW_LAYERS: usize = 7;
/// 由内到外每层的不透明度（索引即扩展像素数）：越靠内越亮，叠出向外的长衰减。
const GLOW_ALPHAS: [u8; GLOW_LAYERS + 1] = [0, 150, 95, 70, 52, 40, 30, 22];

/// 绘制工具栏到像素图。
pub fn render(pixmap: &mut Pixmap, bar: &Toolbar, state: &State, p: &Palette) {
    let ui = bar.ui;
    let gap = bar.gap;
    let first_h = bar.first.h;
    rounded_rect(pixmap, bar.rect, ui.f(7.0), p.bg, Some(p.border));

    // 组分隔线（EDT-16）：位置由布局按**可见元素**算出（整组隐藏时该线也不画），
    // 不再用写死的动作锚点查位置——那种写法在用户隐藏元素后要么锚点消失、要么位置错位。
    for x in &bar.dividers {
        divider(
            pixmap,
            x - gap / 2 - 1,
            bar.first.y + ui.px(7.0),
            first_h - ui.px(14.0),
            p,
        );
    }
    // 第二行分隔线（EDT-4）：字体按钮之后、斜体重之后
    if let Some(second) = bar.second {
        for action in [Action::ToggleBold, Action::FontSmaller] {
            if let Some((r, _)) = bar.hits.iter().find(|(_, a)| *a == action) {
                divider(
                    pixmap,
                    r.x - gap / 2 - 1,
                    second.y + ui.px(7.0),
                    second.h - ui.px(14.0),
                    p,
                );
            }
        }
    }

    for (r, action) in &bar.hits {
        // 悬停底色（EDT-10）：气泡出现前先给反馈；激活态自己画 accent_soft，会盖住这层
        if state.hover == Some(*action) {
            rounded_rect(pixmap, *r, ui.f(5.0), p.row_hover, None);
        }
        match *action {
            Action::Tool(tool) => {
                let active = state.tool == Some(tool);
                if active {
                    rounded_rect(pixmap, *r, 5.0, p.accent_soft, None);
                }
                let color = if tool == Tool::Highlight {
                    // 荧光笔图标：颜色跟主题（浅色主题用暗金，否则看不清楚），激活时更浓
                    let c = p.highlight_ink;
                    [c[0], c[1], c[2], if active { 255 } else { c[3] }]
                } else if active {
                    p.icon
                } else {
                    p.icon_dim
                };
                draw_icon(pixmap, tool_icon(tool), *r, color);
            }
            // ADR 0036：移动（指针）按钮：移动态高亮，绘制态置灰
            Action::Move => {
                let active = state.tool.is_none();
                if active {
                    rounded_rect(pixmap, *r, 5.0, p.accent_soft, None);
                }
                draw_icon(pixmap, Icon::Move, *r, if active { p.icon } else { p.icon_dim });
            }
            Action::Color(i) => {
                let inset = ui.px(2.0);
                let swatch = SelRect {
                    x: r.x + inset,
                    y: r.y + inset,
                    w: r.w - inset * 2,
                    h: r.h - inset * 2,
                };
                let color = state.colors[i];
                // 正在使用的颜色：色块以自身颜色发光，替代早期的蓝色描边（EDT-7 后续）
                if i == state.color {
                    draw_glow(pixmap, swatch, ui.f(4.0), color, ui);
                }
                rounded_rect(pixmap, swatch, ui.f(4.0), color, Some(p.swatch_border));
            }
            // 填充开关：仅当前工具或选中对象是矩形 / 椭圆时可用，否则置灰禁用
            Action::ToggleFill => {
                let enabled = state.fill_enabled;
                if enabled && state.filled {
                    rounded_rect(pixmap, *r, ui.f(5.0), p.accent_soft, None);
                }
                draw_icon(pixmap, Icon::Fill, *r, toggle_color(enabled, state.filled, p));
            }
            Action::Undo => draw_icon(pixmap, Icon::Undo, *r, if state.can_undo { p.icon } else { p.icon_dim }),
            Action::Redo => draw_icon(pixmap, Icon::Redo, *r, if state.can_redo { p.icon } else { p.icon_dim }),
            Action::Pin => draw_icon(pixmap, Icon::Pin, *r, p.icon),
            Action::Longshot => draw_icon(pixmap, Icon::Longshot, *r, p.icon),
            Action::Copy => draw_icon(pixmap, Icon::Copy, *r, p.icon),
            Action::Save => draw_icon(pixmap, Icon::Save, *r, p.icon),
            // EDT-4：字体族按钮（展开/收起字体列表），名字过长截断加省略号
            Action::FontPick => {
                if state.font_open {
                    rounded_rect(pixmap, *r, ui.f(5.0), p.accent_soft, None);
                }
                let spec = FontSpec::new(ui.f(LABEL_SIZE_BASE));
                let color = if state.font_open { p.accent } else { p.text };
                let name = fit_name(&state.font_family, (r.w - ui.px(12.0)) as f32, &spec);
                left_text(pixmap, *r, ui.px(6.0), &name, &spec, color);
            }
            Action::ToggleBold => toggle_icon(pixmap, Icon::Bold, *r, state.bold, p, ui),
            Action::ToggleItalic => toggle_icon(pixmap, Icon::Italic, *r, state.italic, p, ui),
            Action::FontSmaller => draw_icon(pixmap, Icon::Minus, *r, p.icon),
            Action::FontLarger => draw_icon(pixmap, Icon::Plus, *r, p.icon),
            Action::FontSize => {
                let spec = FontSpec::new(ui.f(LABEL_SIZE_BASE));
                let text = format!("{}", state.font_size.round() as i32);
                centered_text(pixmap, *r, &text, &spec, p.text);
            }
        }
    }
}

/// 选中色块的光晕：用色块自身的颜色叠几层低透明度圆角矩形，
/// 由外到内逐层加亮，纯 CPU 近似柔和发光（不引 GPU / 高斯模糊，符合无独显目标）。
fn draw_glow(pixmap: &mut Pixmap, swatch: SelRect, radius: f32, color: [u8; 4], ui: Ui) {
    for k in (1..=GLOW_LAYERS).rev() {
        let e = ui.px(k as f32);
        let layer = SelRect {
            x: swatch.x - e,
            y: swatch.y - e,
            w: swatch.w + e * 2,
            h: swatch.h + e * 2,
        };
        rounded_rect(
            pixmap,
            layer,
            radius + e as f32,
            [color[0], color[1], color[2], GLOW_ALPHAS[k]],
            None,
        );
    }
}

/// 工具 → 图标。
fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Rect => Icon::Rect,
        Tool::Arrow => Icon::Arrow,
        Tool::Text => Icon::Text,
        Tool::Highlight => Icon::Highlight,
        Tool::Blur => Icon::Blur,
        Tool::Glow => Icon::Glow,
        Tool::Number => Icon::Number,
        Tool::Line => Icon::Line,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Freehand => Icon::Freehand,
    }
}

/// 在矩形中心按边长绘制图标（活动区由 `render::icon` 统一归一化）。
fn draw_icon(pixmap: &mut Pixmap, icon: Icon, r: SelRect, color: [u8; 4]) {
    let cx = r.x as f32 + r.w as f32 / 2.0;
    let cy = r.y as f32 + r.h as f32 / 2.0;
    icon::draw(pixmap, icon, cx, cy, r.w.min(r.h) as f32, color);
}

/// 开关类按钮的图标颜色：禁用置灰、开启用强调色、关闭用主色。
fn toggle_color(enabled: bool, on: bool, p: &Palette) -> [u8; 4] {
    if !enabled {
        p.icon_dim
    } else if on {
        p.accent
    } else {
        p.icon
    }
}

/// B / I 开关按钮：开启时强调底色 + 强调色图标。
fn toggle_icon(pixmap: &mut Pixmap, icon: Icon, r: SelRect, on: bool, p: &Palette, ui: Ui) {
    if on {
        rounded_rect(pixmap, r, ui.f(5.0), p.accent_soft, None);
    }
    draw_icon(pixmap, icon, r, if on { p.accent } else { p.icon });
}
