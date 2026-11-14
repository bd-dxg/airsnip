// SPDX-License-Identifier: GPL-3.0-only
//! 面板布局计算（M3）：行几何 + 各分类的行集合 + 总高度。
//! 从 `controls` 拆出，保证单文件不超过 300 行（类型定义留在父模块）。

use crate::overlay::geometry::SelRect;

use super::{
    CELL_PAD, CELLPAD, CLEAR_BTN, CONTENT_HEADER_H, GRID_GAP, GROUP_H, H, HOVER_PAD, Item, KEY_ROWS, Layout, NAV_H,
    NAV_W, PAD, QR_ROW_H, ROW_H, Row, RowId, RowKind, Section, TOGGLE_W, W, ui_scale_row,
};
use crate::settings::ToolbarGroup;

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

/// 单元格内控件区宽度基准（含键帽 + 间隙 + 清除按钮）：键帽 ≈ 140 − 8 − 30 = 102，
/// 足够显示常见组合键（如 Ctrl+F3、鼠标侧键1）。
const CELL_CONTROL_W: f32 = 140.0;

/// 内容行几何（`s` 为 DPI 缩放系数，x 从导航栏右缘开始）。
fn mk_row(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32) -> Row {
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

/// 单元格行（两列排布用）：标签在左、键帽 + 清除按钮在右，控件区宽度按单元格收缩。
fn mk_cell(id: RowId, label: &'static str, x: i32, y: i32, w: i32, s: f32) -> Row {
    let h = (ROW_H as f32 * s) as i32;
    let cellpad = (CELLPAD as f32 * s) as i32;
    let cw = (CELL_CONTROL_W * s) as i32;
    let btn = (CLEAR_BTN as f32 * s) as i32;
    // 内容区 = 单元格左右各缩进 `CELLPAD`；悬停高亮再外扩 `HOVER_PAD`
    let (content_l, content_r) = (x + cellpad, x + w - cellpad);
    Row {
        id,
        kind: RowKind::KeyCap,
        label,
        label_pad: cellpad,
        hover: hover_rect(content_l, content_r, y, h, s),
        rect: SelRect { x, y, w, h },
        control: SelRect {
            x: content_r - cw,
            y,
            w: cw,
            h,
        },
        clear: SelRect {
            x: content_r - btn,
            y: y + (h - btn) / 2,
            w: btn,
            h: btn,
        },
    }
}

/// 高行：高度按内容给（支持项目块 / 组头），几何与标准行一致。
fn mk_row_tall(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32, h: i32) -> Row {
    let mut row = mk_row(id, kind, label, y, s);
    row.rect.h = h;
    row.control.h = h;
    row.hover.h = h;
    row
}

/// 「工具栏」页的开关单元格（EDT-16）：标签在左、开关靠右，控件区就是开关本身。
fn mk_toggle_cell(id: RowId, label: &'static str, x: i32, y: i32, w: i32, s: f32) -> Row {
    let h = (ROW_H as f32 * s) as i32;
    let cellpad = (CELL_PAD as f32 * s) as i32;
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

/// 指定分类的内容行。
fn section_rows(section: Section, y0: i32, s: f32, items: &mut Vec<Item>) {
    let mut y = y0;
    let push = |id: RowId, kind: RowKind, label: &'static str, y: &mut i32, items: &mut Vec<Item>| {
        items.push(Item::Row(mk_row(id, kind, label, *y, s)));
        *y += (ROW_H as f32 * s) as i32;
    };
    match section {
        Section::General => {
            push(RowId::Autostart, RowKind::Toggle, "开机自启", &mut y, items);
            push(
                RowId::ConfirmCancel,
                RowKind::Toggle,
                "连按两次 Esc / 右键退出",
                &mut y,
                items,
            );
            push(RowId::Lang, RowKind::Readonly, "语言", &mut y, items);
            push(RowId::PinPosOriginal, RowKind::Radio, "贴图位置·原位置", &mut y, items);
            push(RowId::PinPosCenter, RowKind::Radio, "贴图位置·屏幕中间", &mut y, items);
        }
        Section::Toolbar => {
            // 多列紧凑排布（EDT-16）：面板没有滚动条，五组 + 组内元素必须落在固定高内。
            // 高度：5 个组头（GROUP_H=24）+ 7 行元素（ROW_H=46）= 442，加节标题 40 → 482 < 520，
            // 页脚提示还能留 12px（有 `toolbar_page::tests::hint_sits_below_last_row` 守门）。
            // 工具组从 10 项到 11 项（ADR 0036 新增「移动」）后是 3 行，组头行高从 26 收到 24 就是为此腾的。
            let w = (W as f32 * s) as i32;
            let nav_w = (NAV_W as f32 * s) as i32;
            let pad = (PAD as f32 * s) as i32;
            let gap = (GRID_GAP as f32 * s) as i32;
            let row_h = (ROW_H as f32 * s) as i32;
            let group_h = (GROUP_H as f32 * s) as i32;
            let (left, right) = (nav_w + pad, w - pad);
            for g in ToolbarGroup::ALL {
                let head = mk_row_tall(RowId::ToolbarGroup(g), RowKind::Group, g.title(), y, s, group_h);
                items.push(Item::Row(head));
                y += group_h;
                let cols = g.columns() as i32;
                let cell_w = (right - left - gap * (cols - 1)) / cols;
                for (i, slot) in g.slots().iter().enumerate() {
                    let (col, line) = (i as i32 % cols, i as i32 / cols);
                    let x = left + col * (cell_w + gap);
                    let cell = mk_toggle_cell(RowId::Toolbar(*slot), slot.label(), x, y + line * row_h, cell_w, s);
                    items.push(Item::Row(cell));
                }
                y += g.slots().len().div_ceil(cols as usize) as i32 * row_h;
            }
        }
        Section::Hotkeys => {
            // 两列排布：快捷键不必每个独占一行，行数减半、不把窗口撑高（保持横向比例）
            let w = (W as f32 * s) as i32;
            let nav_w = (NAV_W as f32 * s) as i32;
            let pad = (PAD as f32 * s) as i32;
            let cellpad = (CELLPAD as f32 * s) as i32;
            let gap = (GRID_GAP as f32 * s) as i32;
            let row_h = (ROW_H as f32 * s) as i32;
            // 单元格左缘让出 CELLPAD，使单元格内标签与整行标签（NAV_W+PAD）对齐
            let cell_x0 = nav_w + pad - cellpad;
            let cell_w = (w - pad + cellpad - cell_x0 - gap) / 2;
            // 键帽行来自元数据表（`keyrows`）：顺序与标签都以表为准，新增一行只改表
            for (i, spec) in KEY_ROWS.iter().enumerate() {
                let col = i % 2;
                let row = i / 2;
                let x = cell_x0 + col as i32 * (cell_w + gap);
                let y = y0 + row as i32 * row_h;
                items.push(Item::Row(mk_cell(spec.id, spec.label, x, y, cell_w, s)));
            }
            y += KEY_ROWS.len().div_ceil(2) as i32 * row_h;
            // 「恢复默认」做成右侧真正的按钮（标签留空，文案在按钮上）
            push(RowId::ResetKeys, RowKind::Button, "", &mut y, items);
            push(RowId::Widenote, RowKind::Readonly, "提示", &mut y, items);
        }
        Section::Save => {
            push(RowId::NameDateTime, RowKind::Radio, "文件名·日期 + 时间", &mut y, items);
            push(
                RowId::NameTimestamp,
                RowKind::Radio,
                "文件名·时间戳（毫秒）",
                &mut y,
                items,
            );
            push(RowId::NameDate, RowKind::Radio, "文件名·仅日期", &mut y, items);
        }
        Section::Appearance => {
            push(RowId::ThemeFollow, RowKind::Radio, "跟随系统", &mut y, items);
            push(RowId::ThemeLight, RowKind::Radio, "浅色", &mut y, items);
            push(RowId::ThemeDark, RowKind::Radio, "深色", &mut y, items);
            for m in crate::settings::UiScale::ALL {
                push(ui_scale_row(m), RowKind::Radio, m.title(), &mut y, items);
            }
        }
        Section::About => {
            push(RowId::About, RowKind::About, "AirSnip 轻截", &mut y, items);
            push(RowId::Repo, RowKind::Link, "项目地址", &mut y, items);
            push(RowId::Licenses, RowKind::Link, "第三方许可", &mut y, items);
            push(RowId::CheckUpdate, RowKind::Button, "版本更新", &mut y, items);
            push(RowId::AutoCheckUpdate, RowKind::Toggle, "启动时自动检查", &mut y, items);
            let qh = (QR_ROW_H as f32 * s) as i32;
            items.push(Item::Row(mk_row_tall(RowId::Reward, RowKind::Qr, "支持项目", y, s, qh)));
        }
    }
}

/// 构建布局：导航项 + 当前节内容；高度固定为 `H`（内容更高时按内容兜底）。
pub fn layout(section: Section, scale: f32) -> Layout {
    let s = scale;
    let mut items = Vec::new();
    for sec in Section::ALL {
        items.push(Item::Nav(sec));
    }
    // 内容区高度：节标题 + 各行伸到最底部的像素（按实际几何取，兼容两列排布，不能按行数×行高）
    let mut max_content = 0i32;
    for sec in Section::ALL {
        let mut rows = Vec::new();
        section_rows(sec, 0, s, &mut rows);
        let bottom = rows
            .iter()
            .filter_map(|it| match it {
                Item::Row(r) => Some(r.rect.y + r.rect.h),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let h = (CONTENT_HEADER_H as f32 * s) as i32 + bottom;
        max_content = max_content.max(h);
    }
    // 当前节的内容行（实际绘制用）
    section_rows(section, (CONTENT_HEADER_H as f32 * s) as i32, s, &mut items);

    let nav_h = (NAV_H as f32 * s) as i32 * Section::ALL.len() as i32;
    let fixed_h = (H as f32 * s) as i32;
    Layout {
        items,
        height: fixed_h.max(max_content).max(nav_h),
    }
}
