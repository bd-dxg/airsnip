// SPDX-License-Identifier: GPL-3.0-only
//! 面板布局计算（M3）：行几何原语 + 总高度。
//! 各分类「放哪些行」在 `sections`（ADR 0041 之后那边更长，故按职责拆开）。
//! 类型定义留在父模块 `controls`。

use crate::overlay::geometry::SelRect;

use super::{
    CELLPAD, CLEAR_BTN, CLEAR_GAP, CONTENT_HEADER_H, H, HOVER_PAD, INPUT_H, Item, KEY_ROW_H, KEYCAP_H, KEYCAP_W,
    KeyGroup, Layout, NAV_H, NAV_W, PAD, ROW_H, Row, RowId, RowKind, Section, TOGGLE_W, W, sections,
};

/// 内容区 [left, right] 外扩 `HOVER_PAD` 得到悬停高亮矩形（四周留出内边距）。
fn hover_rect(left: i32, right: i32, y: i32, h: i32, s: f32) -> SelRect {
    let hp = (HOVER_PAD as f32 * s) as i32;
    SelRect {
        x: left - hp,
        y,
        w: (right - left) + hp * 2,
        h,
    }
}

/// 内容行几何（`s` 为 DPI 缩放系数，x 从导航栏右缘开始）。
pub(super) fn mk_row(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32) -> Row {
    // 横向几何同样按 DPI 缩放，否则高 DPI 下内容区宽度不变、行高变大（比例失真）
    let h = (ROW_H as f32 * s) as i32;
    let w = (W as f32 * s) as i32;
    let nav_w = (NAV_W as f32 * s) as i32;
    let pad = (PAD as f32 * s) as i32;
    let cw = (260.0 * s) as i32;
    let btn = (CLEAR_BTN as f32 * s) as i32;
    Row {
        id,
        kind,
        label,
        label_pad: pad,
        // 内容区（标签左内边距到右侧控件右缘）外扩 `HOVER_PAD`
        hover: hover_rect(nav_w + pad, w - pad, y, h, s),
        rect: SelRect {
            x: nav_w,
            y,
            w: w - nav_w - pad,
            h,
        },
        control: SelRect {
            x: w - pad - cw,
            y,
            w: cw,
            h,
        },
        // 只有键帽行带清除按钮；其余行给零尺寸，命中测试永不命中（SYS-2）
        clear: if kind == RowKind::KeyCap {
            SelRect {
                x: w - pad - btn,
                y: y + (h - btn) / 2,
                w: btn,
                h: btn,
            }
        } else {
            SelRect { x: 0, y: 0, w: 0, h: 0 }
        },
    }
}

/// 快捷键单元格（ADR 0041：**标签左、键帽右**并排）：
/// 标签垂直居中在左，右侧固定宽的键帽框，最右是清除按钮（不压键帽）。
pub(super) fn mk_key_cell(id: RowId, label: &'static str, x: i32, y: i32, w: i32, s: f32) -> Row {
    let h = (KEY_ROW_H as f32 * s) as i32;
    let cellpad = (CELLPAD as f32 * s) as i32;
    let btn = (CLEAR_BTN as f32 * s) as i32;
    let gap = (CLEAR_GAP as f32 * s) as i32;
    let cap = (KEYCAP_W as f32 * s) as i32;
    let caph = (KEYCAP_H as f32 * s) as i32;
    // 内容区 = 单元格左右各缩进 `CELLPAD`；悬停高亮再外扩 `HOVER_PAD`
    let (content_l, content_r) = (x + cellpad, x + w - cellpad);
    Row {
        id,
        kind: RowKind::KeyCap,
        label,
        label_pad: cellpad,
        hover: hover_rect(content_l, content_r, y, h, s),
        rect: SelRect { x, y, w, h },
        // 键帽框右对齐，右侧依次让出清除按钮与间距
        control: SelRect {
            x: content_r - btn - gap - cap,
            y: y + (h - caph) / 2,
            w: cap,
            h: caph,
        },
        clear: SelRect {
            x: content_r - btn,
            y: y + (h - btn) / 2,
            w: btn,
            h: btn,
        },
    }
}

/// 整行宽的输入框控件（OUT-2 / ADR 0041：文件名模板）：`label` 仍画在左侧，
/// 输入框从标签右侧一直伸到内容区右缘（比标准 260px 控件区宽得多，长文件名不用翻滚着改）。
pub(super) fn mk_row_wide(id: RowId, kind: RowKind, label: &'static str, label_w: f32, y: i32, s: f32) -> Row {
    let mut row = mk_row(id, kind, label, y, s);
    let pad = (PAD as f32 * s) as i32;
    let gap = (16.0 * s) as i32;
    let left = row.rect.x + pad + label_w as i32 + gap;
    let right = row.hover.x + row.hover.w - (HOVER_PAD as f32 * s) as i32;
    let bh = (INPUT_H as f32 * s) as i32;
    row.control = SelRect {
        x: left,
        y: y + (row.rect.h - bh) / 2,
        w: (right - left).max(0),
        h: bh,
    };
    row
}

/// 高行：高度按内容给（支持项目块 / 组头），几何与标准行一致。
pub(super) fn mk_row_tall(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32, h: i32) -> Row {
    let mut row = mk_row(id, kind, label, y, s);
    row.rect.h = h;
    row.control.h = h;
    row.hover.h = h;
    row
}

/// 「工具栏」页的开关单元格（EDT-16）：标签在左、开关靠右，控件区就是开关本身。
/// `h` 由页面按「可容纳的行数」算（英文 / 日文降列后行数变多，见 `sections::toolbar_columns`）。
pub(super) fn mk_toggle_cell(id: RowId, label: &'static str, x: i32, y: i32, w: i32, h: i32, s: f32) -> Row {
    let cellpad = (super::CELL_PAD as f32 * s) as i32;
    let tw = (TOGGLE_W as f32 * s) as i32;
    let (content_l, content_r) = (x + cellpad, x + w - cellpad);
    Row {
        id,
        kind: RowKind::Toggle,
        label,
        label_pad: cellpad,
        hover: hover_rect(content_l, content_r, y, h, s),
        rect: SelRect { x, y, w, h },
        control: SelRect {
            x: content_r - tw,
            y,
            w: tw,
            h,
        },
        // 只有键帽行带清除按钮；这里给零尺寸，命中测试永不命中
        clear: SelRect { x: 0, y: 0, w: 0, h: 0 },
    }
}

/// 构建布局：导航项 + 当前节内容；高度固定为 `H`（内容更高时按内容兜底）。
/// 快捷键页的键帽只画 `KeyGroup::Capture`（测试与不关心子页签的调用方用这个）；
/// 面板要用 `layout_with` 把当前组传进去。
pub fn layout(section: Section, scale: f32) -> Layout {
    layout_with(section, scale, KeyGroup::Capture)
}

/// 带「当前子页签」的布局（设置面板用）。
pub fn layout_with(section: Section, scale: f32, group: KeyGroup) -> Layout {
    let s = scale;
    let mut items = Vec::new();
    for sec in Section::ALL {
        items.push(Item::Nav(sec));
    }
    // 内容区高度：各行伸到最底部的像素（行坐标已经是相对面板左上角的绝对值，
    // 含节标题偏移，因此**不再叠加** `CONTENT_HEADER_H`；否则固定在底部的行会被算两遍）。
    // 快捷键页取三组里**最高**的那组，否则切组时窗口会高度跳动。
    let mut max_content = 0i32;
    for sec in Section::ALL {
        for g in KeyGroup::ALL {
            let mut rows = Vec::new();
            sections::section_rows(sec, (CONTENT_HEADER_H as f32 * s) as i32, s, g, &mut None, &mut rows);
            let bottom = rows
                .iter()
                .filter_map(|it| match it {
                    Item::Row(r) => Some(r.rect.y + r.rect.h),
                    Item::Tab(t) => Some(t.rect.y + t.rect.h),
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            max_content = max_content.max(bottom);
            if sec != Section::Hotkeys {
                break; // 其余页与子页签无关，只算一次
            }
        }
    }
    // 当前节的内容行（实际绘制用）
    let mut tab_bar = None;
    sections::section_rows(
        section,
        (CONTENT_HEADER_H as f32 * s) as i32,
        s,
        group,
        &mut tab_bar,
        &mut items,
    );

    let nav_h = (NAV_H as f32 * s) as i32 * Section::ALL.len() as i32;
    let fixed_h = (H as f32 * s) as i32;
    Layout {
        items,
        height: fixed_h.max(max_content).max(nav_h),
        tab_bar,
    }
}
