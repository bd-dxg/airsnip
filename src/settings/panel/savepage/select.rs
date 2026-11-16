// SPDX-License-Identifier: GPL-3.0-only
//! 「保存」页的两个**真下拉**（OUT-2 / ADR 0041）：格式与同名策略。
//! 点开一层列表 → 点选项生效 → 点列表外 / `Esc` 收起；展开期间点击**只**用于选项或收起，
//! 绝不穿透到下层控件（`interact::on_click` 会先问 `on_list_click`）。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::render::text as dwrite;
use crate::settings::{ExistingFile, SaveFormat};

use super::super::Panel;
use super::super::controls::{CONTENT_HEADER_H, Item, Layout, PAD, Row, RowId, W};
use super::super::draw::draw_text;

/// 下拉候选（顺序即列表顺序，索引用于写回）。
pub(super) fn select_options(id: RowId) -> Vec<&'static str> {
    match id {
        RowId::QuickFormat => SaveFormat::ALL.iter().map(|f| f.title()).collect(),
        RowId::QuickOnExist => ExistingFile::ALL.iter().map(|e| e.title()).collect(),
        _ => Vec::new(),
    }
}

/// 当前选中的下标。
pub(super) fn select_index(panel: &Panel, id: RowId) -> usize {
    match id {
        RowId::QuickFormat => SaveFormat::ALL
            .iter()
            .position(|f| *f == panel.config.quick_save.format)
            .unwrap_or(0),
        RowId::QuickOnExist => ExistingFile::ALL
            .iter()
            .position(|e| *e == panel.config.quick_save.on_exist)
            .unwrap_or(0),
        _ => 0,
    }
}

/// 当前值文字。
pub(super) fn select_value(panel: &Panel, id: RowId) -> &'static str {
    select_options(id).get(select_index(panel, id)).copied().unwrap_or("")
}

/// 写回选中项（并标记待应用）。
pub(super) fn apply_option(panel: &mut Panel, id: RowId, index: usize) {
    match id {
        RowId::QuickFormat => match SaveFormat::ALL.get(index) {
            Some(f) => panel.config.quick_save.format = *f,
            None => return,
        },
        RowId::QuickOnExist => match ExistingFile::ALL.get(index) {
            Some(e) => panel.config.quick_save.on_exist = *e,
            None => return,
        },
        _ => return,
    }
    panel.config_dirty = true;
}

/// 控件胶囊的位置：绘制与命中共用这一份几何（否则「看着在哪」与「点在哪」会分叉）。
pub(super) fn pill_rect(panel: &Panel, row: &Row, value: &str) -> SelRect {
    let s = panel.scale;
    let size = 15.0 * s;
    let text = format!("{value}   ▾");
    let (tw, _) = dwrite::measure(&text, &dwrite::FontSpec::new(size));
    let bw = ((tw + 24.0 * s) as i32).min(row.control.w);
    let bh = ((30.0 * s) as i32).min(row.control.h);
    SelRect {
        x: row.control.x + row.control.w - bw,
        y: row.control.y + (row.control.h - bh) / 2,
        w: bw,
        h: bh,
    }
}

/// 展开列表的外框：右对齐到胶囊、默认贴其下方；下方塞不下就翻到上方，最后钳回面板内。
pub(in crate::settings::panel) fn list_rect(panel: &Panel, layout: &Layout, id: RowId) -> Option<SelRect> {
    let row = find_row(layout, id)?;
    let pill = pill_rect(panel, &row, select_value(panel, id));
    let s = panel.scale;
    let opts = select_options(id);
    let spec = dwrite::FontSpec::new(15.0 * s);
    let mut w = pill.w;
    for o in &opts {
        w = w.max(dwrite::measure(o, &spec).0 as i32 + 2 * (14.0 * s) as i32);
    }
    let item_h = (30.0 * s) as i32;
    let pad = (5.0 * s) as i32;
    let h = item_h * opts.len() as i32 + pad * 2;
    let panel_w = (W as f32 * s) as i32;
    let margin = (PAD as f32 * s) as i32;
    let x = (pill.x + pill.w - w).clamp(margin, (panel_w - margin - w).max(margin));
    let below = pill.y + pill.h + (2.0 * s) as i32;
    let y = if below + h <= layout.height - margin {
        below
    } else {
        (pill.y - h - (2.0 * s) as i32).max((CONTENT_HEADER_H as f32 * s) as i32)
    };
    Some(SelRect { x, y, w, h })
}

/// 列表里第 `i` 项的矩形。
pub(in crate::settings::panel) fn item_rect(list: SelRect, i: usize, s: f32) -> SelRect {
    let item_h = (30.0 * s) as i32;
    let pad = (5.0 * s) as i32;
    SelRect {
        x: list.x + pad,
        y: list.y + pad + i as i32 * item_h,
        w: list.w - pad * 2,
        h: item_h,
    }
}

/// 鼠标落在哪一项上（未展开或无命中则 `None`）。
pub(in crate::settings::panel) fn list_hover_at(panel: &Panel, layout: &Layout, x: i32, y: i32) -> Option<usize> {
    let id = panel.open_select?;
    let list = list_rect(panel, layout, id)?;
    (0..select_options(id).len()).find(|i| {
        let r = item_rect(list, *i, panel.scale);
        x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
    })
}

/// 展开态的点击：命中选项则写回并收起，否则只收起。
/// **总是吃掉这次点击**（返回 `true`），绝不穿透到下层控件。
pub(in crate::settings::panel) fn on_list_click(panel: &mut Panel, x: i32, y: i32) -> bool {
    let Some(id) = panel.open_select else {
        return false;
    };
    let layout = panel.layout();
    if let Some(list) = list_rect(panel, &layout, id) {
        let inside = x >= list.x && x < list.x + list.w && y >= list.y && y < list.y + list.h;
        if inside {
            let n = select_options(id).len();
            if let Some(i) = (0..n).find(|i| {
                let r = item_rect(list, *i, panel.scale);
                x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
            }) {
                apply_option(panel, id, i);
            }
        }
    }
    panel.open_select = None;
    panel.select_hover = None;
    true
}

/// 在布局里按 ID 取行的几何（`Row` 非 `Clone`，只复制几何字段）。
fn find_row(layout: &Layout, id: RowId) -> Option<Row> {
    layout.items.iter().find_map(|it| match it {
        Item::Row(r) if r.id == id => Some(Row {
            id: r.id,
            kind: r.kind,
            label: r.label,
            label_pad: r.label_pad,
            hover: r.hover,
            rect: r.rect,
            control: r.control,
            clear: r.clear,
        }),
        _ => None,
    })
}

/// 展开的下拉列表（画在所有行之上；变量说明浮层之后、轻通知之前）。
pub(in crate::settings::panel) fn draw_list(pixmap: &mut Pixmap, panel: &Panel, layout: &Layout) {
    let Some(id) = panel.open_select else {
        return;
    };
    let Some(list) = list_rect(panel, layout, id) else {
        return;
    };
    let p = panel.palette;
    let s = panel.scale;
    rounded_rect(
        pixmap,
        list,
        (8.0 * s) as f32,
        [p.bg[0], p.bg[1], p.bg[2], 255],
        Some(p.border),
    );
    let current = select_index(panel, id);
    for (i, text) in select_options(id).iter().enumerate() {
        let r = item_rect(list, i, s);
        if i == current {
            rounded_rect(pixmap, r, (5.0 * s) as f32, p.accent_soft, None);
        } else if panel.select_hover == Some(i) {
            rounded_rect(pixmap, r, (5.0 * s) as f32, p.row_hover, None);
        }
        let size = 15.0 * s;
        let color = if i == current { p.accent } else { p.text };
        let x = r.x as f32 + 14.0 * s;
        let y = r.y as f32 + (r.h as f32 - size * 1.2) / 2.0;
        draw_text(pixmap, x, y, text, size, color);
    }
}

/// 下拉外观：右侧的数值 + 箭头（展开时用强调色边框 / 文字区分）。
pub(super) fn draw_select(pixmap: &mut Pixmap, panel: &Panel, row: &Row, value: &str, open: bool) {
    let p = panel.palette;
    let s = panel.scale;
    let size = 15.0 * s;
    // 箭头固定用 `▾`（不用 `▴`）：部分字体没有 `▴` 字形，会回退成别的形状反而看着是「朝下」。
    let text = format!("{value}   ▾");
    let rect = pill_rect(panel, row, value);
    rounded_rect(
        pixmap,
        rect,
        (6.0 * s) as f32,
        [p.bg[0], p.bg[1], p.bg[2], 255],
        Some(if open { p.accent } else { p.border }),
    );
    let (tw, _) = dwrite::measure(&text, &dwrite::FontSpec::new(size));
    let x = rect.x as f32 + (rect.w as f32 - tw) / 2.0;
    let y = rect.y as f32 + (rect.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x, y),
        &text,
        &dwrite::FontSpec::new(size),
        if open { p.accent } else { p.text },
        Transform::identity(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::panel::controls::RowKind;

    fn panel() -> Panel {
        Panel::for_test(
            crate::settings::panel::controls::Section::Save,
            crate::theme::Theme::Light,
        )
    }

    /// 点下拉选项：写回对应枚举、收起列表、标记待应用。
    #[test]
    fn dropdown_applies_option_and_closes() {
        let mut p = panel();
        p.open_select = Some(RowId::QuickFormat);
        let layout = p.layout();
        let list = list_rect(&p, &layout, RowId::QuickFormat).expect("应有列表矩形");
        // 第三项 = .webp
        let r = item_rect(list, 2, p.scale);
        assert!(on_list_click(&mut p, r.x + 4, r.y + 4));
        assert_eq!(p.config.quick_save.format, SaveFormat::Webp);
        assert!(p.open_select.is_none(), "选完应收起");
        assert!(p.config_dirty, "应标记待应用");

        // 另一个下拉同样生效（保留序号策略）
        p.open_select = Some(RowId::QuickOnExist);
        let layout = p.layout();
        let list = list_rect(&p, &layout, RowId::QuickOnExist).expect("应有列表矩形");
        let r = item_rect(list, 1, p.scale);
        assert!(on_list_click(&mut p, r.x + 4, r.y + 4));
        assert_eq!(p.config.quick_save.on_exist, ExistingFile::Overwrite);
    }

    /// 点列表外只收起，不改值（且吞掉点击，不穿透到下层行）。
    #[test]
    fn dropdown_click_outside_only_closes() {
        let mut p = panel();
        p.open_select = Some(RowId::QuickFormat);
        let before = p.config.quick_save.format;
        assert!(on_list_click(&mut p, 5, 5), "应吞掉点击");
        assert_eq!(p.config.quick_save.format, before);
        assert!(p.open_select.is_none());
    }

    /// 列表矩形始终落在面板内，且右对齐到胶囊、不压住胶囊自身。
    #[test]
    fn dropdown_list_fits_inside_panel() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            for id in [RowId::QuickFormat, RowId::QuickOnExist] {
                let mut p = panel();
                p.scale = scale;
                p.open_select = Some(id);
                let layout = p.layout();
                let list = list_rect(&p, &layout, id).expect("应有列表矩形");
                let panel_w = (W as f32 * scale) as i32;
                assert!(
                    list.x >= 0 && list.x + list.w <= panel_w,
                    "@{scale}x 越出面板：{list:?}"
                );
                assert!(
                    list.y >= 0 && list.y + list.h <= layout.height,
                    "@{scale}x 越出面板纵向：{list:?}"
                );
                let row = layout
                    .items
                    .iter()
                    .find_map(|it| match it {
                        Item::Row(r) if r.id == id => Some(r.control),
                        _ => None,
                    })
                    .expect("应有控件区");
                let pill = pill_rect(&p, &find_row(&layout, id).unwrap(), select_value(&p, id));
                assert!(pill.x >= row.x && pill.x + pill.w <= row.x + row.w, "胶囊应在控件区内");
                // 列表要么全在胶囊下方、要么全在上方（不重叠胶囊）
                assert!(
                    list.y >= pill.y + pill.h || list.y + list.h <= pill.y,
                    "@{scale}x 列表与胶囊重叠：{list:?} / {pill:?}"
                );
            }
        }
    }

    /// 索引 ↔ 枚举互相对得上（按下标写回时不会串到另一项）。
    #[test]
    fn option_indices_map_to_enum_values() {
        let mut p = panel();
        for (i, f) in SaveFormat::ALL.iter().enumerate() {
            apply_option(&mut p, RowId::QuickFormat, i);
            assert_eq!(p.config.quick_save.format, *f);
            assert_eq!(select_index(&p, RowId::QuickFormat), i);
            assert_eq!(select_value(&p, RowId::QuickFormat), f.title());
        }
        for (i, e) in ExistingFile::ALL.iter().enumerate() {
            apply_option(&mut p, RowId::QuickOnExist, i);
            assert_eq!(p.config.quick_save.on_exist, *e);
            assert_eq!(select_index(&p, RowId::QuickOnExist), i);
        }
        // 越界下标不改值
        let before = p.config.quick_save.format;
        apply_option(&mut p, RowId::QuickFormat, 99);
        assert_eq!(p.config.quick_save.format, before);
        // 非下拉行不产生候选（防止行 ID 传错时静默画空列表）
        assert!(select_options(RowId::SaveNotify).is_empty());
        assert_eq!(
            find_row(&p.layout(), RowId::QuickFormat).map(|r| r.kind),
            Some(RowKind::Select)
        );
    }
}
