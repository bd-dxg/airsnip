// SPDX-License-Identifier: GPL-3.0-only
//! 「工具栏」页专用绘制（EDT-16，issue #143）：组头三态勾选框与页脚提示。
//! 从 `draw` 拆出，保证单文件不超过 300 行。

use tiny_skia::{Pixmap, Transform};

use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{paint_of, rounded_rect, rounded_rect_outline};
use crate::render::text as dwrite;
use crate::settings::{GroupState, ToolbarGroup};

use super::Panel;
use super::controls::{Layout, RowId, Section};
use super::draw::draw_text;

/// 「工具栏」页脚注（EDT-16 R7）：文件组全部隐藏时提醒快捷键仍在。
/// 不拦全隐藏、不弹确认框 —— 只是把「隐藏 ≠ 禁用」写在用户看得到的地方。
pub(super) fn draw_hint(pixmap: &mut Pixmap, panel: &Panel, layout: &Layout) {
    if panel.section != Section::Toolbar {
        return;
    }
    if panel.config.toolbar.group_state(ToolbarGroup::File) != GroupState::None {
        return;
    }
    let s = panel.scale;
    let text = "工具栏上已无复制 / 保存按钮，Enter 仍复制、Ctrl+S 仍保存";
    let x = (super::controls::NAV_W as f32 + super::controls::PAD as f32) * s;
    draw_text(pixmap, x, hint_y(layout, s), text, 14.0 * s, panel.palette.text_dim);
}

/// 页脚提示的基线 y：贴着页面底部内边距放（不随内容行数增长）。
/// 「工具栏」页当前内容 446 / 固定高 520，空出 74px；**加行时先跑
/// `hint_sits_below_last_row`**，别等到提示静默压住最后一排开关。
fn hint_y(layout: &Layout, s: f32) -> f32 {
    layout.height as f32 - 26.0 * s
}

/// 组头（EDT-16）：组名（小一号的次要色）+ 右侧三态勾选框。
/// 勾选框表达「全显 / 部分可见 / 全隐」，整行可点（点一下就批量切换）。
pub(super) fn draw_group(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, hovered: bool) {
    let p = panel.palette;
    let s = panel.scale;
    let RowId::ToolbarGroup(group) = row.id else {
        return;
    };
    let state = panel.config.toolbar.group_state(group);
    // 组名是分节标签，不是可调项：小一号字 + 次要色
    let size = 14.0 * s;
    let ly = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    draw_text(
        pixmap,
        row.rect.x as f32 + row.label_pad as f32,
        ly,
        row.label,
        size,
        p.text_dim,
    );

    let d = (16.0 * s) as i32;
    // 勾选框紧跟组名（而不是靠到行右缘）：离标题 400px 远的话看不出它管的是这一组
    let (title_w, _) = dwrite::measure(row.label, &dwrite::FontSpec::new(size));
    let box_rect = SelRect {
        x: row.rect.x + row.label_pad + title_w as i32 + (10.0 * s) as i32,
        y: row.rect.y + (row.rect.h - d) / 2,
        w: d,
        h: d,
    };
    let radius = 4.0 * s;
    if state == GroupState::None {
        // 全隐：只留边框（悬停时边框提亮，提示可点）
        rounded_rect_outline(pixmap, box_rect, radius, if hovered { p.text } else { p.text_dim });
        return;
    }
    rounded_rect(pixmap, box_rect, radius, p.accent, None);
    let white = [255u8, 255, 255, 255];
    if state == GroupState::Partial {
        // 半选：实心底 + 白横线
        let bar_h = (3.0 * s).max(1.0) as i32;
        let bar = SelRect {
            x: box_rect.x + (d as f32 * 0.25) as i32,
            y: box_rect.y + (d - bar_h) / 2,
            w: (d as f32 * 0.5) as i32,
            h: bar_h,
        };
        rounded_rect(pixmap, bar, 1.0, white, None);
        return;
    }
    // 全显：实心底 + 白勾
    let (x0, y0, f) = (box_rect.x as f32, box_rect.y as f32, d as f32);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x0 + f * 0.26, y0 + f * 0.52);
    pb.line_to(x0 + f * 0.44, y0 + f * 0.70);
    pb.line_to(x0 + f * 0.76, y0 + f * 0.32);
    if let Some(path) = pb.finish() {
        let stroke = tiny_skia::Stroke {
            width: (2.0 * s).max(1.0),
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint_of(white), &stroke, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::super::controls::{self, Item};
    use super::*;

    /// 页脚提示必须落在「工具栏」页最后一行之下。
    /// 这一页没有滚动条（高度固定 520），将来加行时若把内容顶到页脚位置，
    /// 提示会静默压住最后一排开关 —— 这条测试就是那个「静默错误的报警器」。
    #[test]
    fn hint_sits_below_last_row() {
        for s in [1.0f32, 1.5, 2.0] {
            let layout = controls::layout(Section::Toolbar, s);
            let bottom = layout
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Row(r) => Some(r.rect.y + r.rect.h),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            // 留出 4px 呼吸空间：最后一行紧贴页脚时虽不重叠，视觉上已经糊成一块
            assert!(
                bottom as f32 + 4.0 * s <= hint_y(&layout, s),
                "@{s}x 「工具栏」页最后一行底部 {bottom} 离页脚提示 {} 太近（需 ≥4px）",
                hint_y(&layout, s)
            );
        }
    }
}
