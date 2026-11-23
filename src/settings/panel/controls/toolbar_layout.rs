// SPDX-License-Identifier: GPL-3.0-only
//! 「工具栏」页的行布局（EDT-16 / I18N-4，issue #151 / ADR 0047）：五组 + 多列排布、
//! **列数由量宽决定**、行高按可用高度收缩。
//!
//! 从 `sections` 拆出：那边同时管各页行清单与本页几何，加 i18n 的自适应逻辑后越界。
//! 拆的边界是「本页自己算几何」与「哪一页有哪些行」。

use crate::settings::{ToolbarGroup, ToolbarSlot};

use super::{
    CELL_PAD, CONTENT_HEADER_H, GRID_GAP, GROUP_H, H, Item, NAV_W, PAD, ROW_H, RowId, RowKind, TOGGLE_W, W,
    mk_row_tall, mk_toggle_cell,
};

/// 「工具栏」页（EDT-16）：五组 + 组内元素多列紧凑排布。
/// 面板没有滚动条，五组 + 组内元素必须落在固定高内。
/// 中文 5 列 / 7 行仍是 442 + 节标题 40 → 482 < 520，页脚提示留 12px
/// （有 `toolbar_page::tests::hint_sits_below_last_row` 守门）；英文 / 日文降到 4 列后
/// 行数变 8，靠 `toolbar_row_h` 收缩行高补回来。
pub(super) fn toolbar_rows(mut y: i32, s: f32, items: &mut Vec<Item>) {
    let w = (W as f32 * s) as i32;
    let nav_w = (NAV_W as f32 * s) as i32;
    let pad = (PAD as f32 * s) as i32;
    let gap = (GRID_GAP as f32 * s) as i32;
    let group_h = (GROUP_H as f32 * s) as i32;
    let (left, right) = (nav_w + pad, w - pad);
    let cols = toolbar_columns(crate::i18n::current(), s, right - left) as i32;
    let row_h = toolbar_row_h(s, cols);
    for g in ToolbarGroup::ALL {
        let head = mk_row_tall(RowId::ToolbarGroup(g), RowKind::Group, g.title(), y, s, group_h);
        items.push(Item::Row(head));
        y += group_h;
        let cell_w = (right - left - gap * (cols - 1)) / cols;
        for (i, slot) in g.slots().iter().enumerate() {
            let (col, line) = (i as i32 % cols, i as i32 / cols);
            let x = left + col * (cell_w + gap);
            let cell = mk_toggle_cell(
                RowId::Toolbar(*slot),
                slot.label(),
                x,
                y + line * row_h,
                cell_w,
                row_h,
                s,
            );
            items.push(Item::Row(cell));
        }
        y += g.slots().len().div_ceil(cols as usize) as i32 * row_h;
    }
}

/// 工具栏页列数：由「最长标签 + 左右内距 + 开关胶囊」在固定内容宽里量宽决定，上限
/// `ToolbarGroup::MAX_COLUMNS`（5）。中文得 5 列，英文 / 日文自动降到 4 列。
/// **不写 `if lang`**：新加一门语言不用改这里（issue #151 R2 / ADR 0047）。
/// `width` 是内容区可用宽（已扣导航栏与左右边距）。
pub(super) fn toolbar_columns(lang: crate::i18n::Lang, s: f32, width: i32) -> usize {
    // 缓存：面板每次重绘会调好几次 `layout()`（绘制 / 命中各自一份），而算列数要量宽
    // 24 个标签（每次 `CreateTextLayout` 几十微秒）。语言 / 缩放 / 可用宽都是稳定键，
    // 照键缓存即可，不需要失效逻辑。
    type Key = (crate::i18n::Lang, i32, i32);
    static CACHE: std::sync::Mutex<Option<(Key, usize)>> = std::sync::Mutex::new(None);
    let key: Key = (lang, (s * 1000.0) as i32, width);
    if let Ok(guard) = CACHE.lock()
        && let Some((cached_key, cols)) = *guard
        && cached_key == key
    {
        return cols;
    }
    let cols = compute_columns(lang, s, width);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((key, cols));
    }
    cols
}

fn compute_columns(lang: crate::i18n::Lang, s: f32, width: i32) -> usize {
    let gap = (GRID_GAP as f32 * s) as i32;
    let cellpad = (CELL_PAD as f32 * s) as i32;
    let toggle = (TOGGLE_W as f32 * s) as i32;
    // 单元格里标签可用的宽 = cell_w - 2*CELL_PAD - TOGGLE_W
    let spec =
        crate::render::text::FontSpec::from_parts(&std::sync::Arc::from(lang.ui_family()), 16.0 * s, false, false);
    let need = ToolbarSlot::all()
        .map(|slot| crate::render::text::measure(slot.label_in(lang), &spec).0 as i32)
        .max()
        .unwrap_or(0);
    for cols in (2..=ToolbarGroup::MAX_COLUMNS).rev() {
        let cell_w = (width - gap * (cols as i32 - 1)) / cols as i32;
        if need + 2 * cellpad + toggle <= cell_w {
            return cols;
        }
    }
    2
}

/// 工具栏页行高：装得下所有行，同时给页脚提示留出位置（`hint_sits_below_last_row` 守门）。
/// 上限仍是 `ROW_H`：列数少时不要把行拉高，否则五组看起来疏密不均。
fn toolbar_row_h(s: f32, cols: i32) -> i32 {
    let groups = ToolbarGroup::ALL.len() as i32;
    let rows: i32 = ToolbarGroup::ALL
        .iter()
        .map(|g| (g.slots().len() as i32 + cols - 1) / cols)
        .sum();
    let full_h = (ROW_H as f32 * s) as i32;
    if rows == 0 {
        return full_h;
    }
    // 页脚提示占 26px（`hint_y`）+ 4px 呼吸空间
    let avail = (H - CONTENT_HEADER_H as i32 - GROUP_H * groups - 30) as f32 * s;
    ((avail / rows as f32) as i32).clamp((32.0 * s) as i32, full_h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::render::text::{self, FontSpec};
    use std::sync::Arc;

    fn label_spec(lang: Lang, s: f32) -> FontSpec {
        FontSpec::from_parts(&Arc::from(lang.ui_family()), 16.0 * s, false, false)
    }

    /// 工具栏页列数：中文 5 列（与引入 i18n 前一致），英文 / 日文自动降到 4 列。
    /// 这条是「加语言不改代码」的实测锚点 —— 掉到 3 列说明文案变长了，
    /// 去缩短标签，而不是继续降列数（3 列会把页高顶爆）。
    #[test]
    fn toolbar_columns_follow_label_width() {
        let width = W - NAV_W - PAD * 2;
        assert_eq!(toolbar_columns(Lang::ZhHans, 1.0, width), 5, "中文应保持 5 列");
        assert_eq!(toolbar_columns(Lang::ZhHant, 1.0, width), 5, "繁中与简中同宽");
        assert_eq!(toolbar_columns(Lang::En, 1.0, width), 4, "英文应降到 4 列");
        assert_eq!(toolbar_columns(Lang::Ja, 1.0, width), 4, "日文应降到 4 列");
    }

    /// 四门语言 × 四档缩放：每格的「标签 + 开关」都装得进单元格（不靠省略号掩盖溢出）。
    #[test]
    fn toolbar_cells_fit_label_and_toggle() {
        for lang in Lang::ALL {
            for s in [1.0f32, 1.25, 1.5, 2.0] {
                let w = (W as f32 * s) as i32;
                let gap = (GRID_GAP as f32 * s) as i32;
                let cellpad = (CELL_PAD as f32 * s) as i32;
                let toggle = (TOGGLE_W as f32 * s) as i32;
                let width = w - (NAV_W as f32 * s) as i32 - 2 * (PAD as f32 * s) as i32;
                let cols = toolbar_columns(lang, s, width);
                let cell_w = (width - gap * (cols as i32 - 1)) / cols as i32;
                let spec = label_spec(lang, s);
                for slot in ToolbarSlot::all() {
                    let label = slot.label_in(lang);
                    let (lw, _) = text::measure(label, &spec);
                    assert!(
                        lw as i32 + 2 * cellpad + toggle <= cell_w,
                        "{lang:?} @{s}x：标签「{label}」量宽 {lw} 放不进单元格 \
                         （cell_w {cell_w}，标签区 {}）",
                        cell_w - 2 * cellpad - toggle
                    );
                }
            }
        }
    }

    /// 行高收缩后整页仍在 H 以内，且页脚提示不会被最后一排压住。
    #[test]
    fn toolbar_page_fits_height_for_every_language() {
        for lang in Lang::ALL {
            for s in [1.0f32, 1.25, 1.5, 2.0] {
                let width = (W as f32 * s) as i32 - (NAV_W as f32 * s) as i32 - 2 * (PAD as f32 * s) as i32;
                let cols = toolbar_columns(lang, s, width) as i32;
                let rows: i32 = ToolbarGroup::ALL
                    .iter()
                    .map(|g| (g.slots().len() as i32 + cols - 1) / cols)
                    .sum();
                let row_h = toolbar_row_h(s, cols);
                let bottom = (CONTENT_HEADER_H as f32 * s) as i32
                    + (GROUP_H as f32 * s) as i32 * ToolbarGroup::ALL.len() as i32
                    + rows * row_h;
                let hint_y = (H as f32 * s) as i32 - (26.0 * s) as i32;
                assert!(
                    bottom as f32 + 4.0 * s <= hint_y as f32,
                    "{lang:?} @{s}x：内容底 {bottom} 压到页脚提示 {hint_y}（{rows} 行 × {row_h}）"
                );
            }
        }
    }
}
