// SPDX-License-Identifier: GPL-3.0-only
//! 面板布局计算（M3）：行几何 + 各分类的行集合 + 总高度。
//! 从 `controls` 拆出，保证单文件不超过 300 行（类型定义留在父模块）。

use crate::overlay::geometry::SelRect;

use super::{
    CLEAR_BTN, CONTENT_HEADER_H, H, Item, Layout, NAV_H, NAV_W, PAD, QR_ROW_H, ROW_H, Row, RowId, RowKind, Section, W,
    ui_scale_row,
};

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

/// 高行：高度按内容给（支持项目块），几何与标准行一致。
fn mk_row_tall(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32, h: i32) -> Row {
    let mut row = mk_row(id, kind, label, y, s);
    row.rect.h = h;
    row.control.h = h;
    row
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
            push(RowId::Lang, RowKind::Readonly, "语言", &mut y, items);
            push(RowId::PinPosOriginal, RowKind::Radio, "贴图位置·原位置", &mut y, items);
            push(RowId::PinPosCenter, RowKind::Radio, "贴图位置·屏幕中间", &mut y, items);
        }
        Section::Hotkeys => {
            push(RowId::HotkeyShot, RowKind::KeyCap, "截图", &mut y, items);
            push(RowId::HotkeyPin, RowKind::KeyCap, "贴图", &mut y, items);
            push(RowId::HotkeyBorder, RowKind::KeyCap, "贴图边框·窗口内", &mut y, items);
            push(RowId::HotkeyLongshot, RowKind::KeyCap, "长截图·截图内", &mut y, items);
            push(RowId::ToolRect, RowKind::KeyCap, "工具·矩形", &mut y, items);
            push(RowId::ToolArrow, RowKind::KeyCap, "工具·箭头", &mut y, items);
            push(RowId::ToolText, RowKind::KeyCap, "工具·文本", &mut y, items);
            push(RowId::ToolBlur, RowKind::KeyCap, "工具·模糊", &mut y, items);
            push(RowId::ToolHighlight, RowKind::KeyCap, "工具·荧光笔", &mut y, items);
            push(RowId::ToolGlow, RowKind::KeyCap, "工具·聚光灯", &mut y, items);
            push(RowId::ResetKeys, RowKind::Action, "恢复默认", &mut y, items);
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
    // 内容区：节标题 + 各节最大行数（每节各自累计，取最高者）
    let mut max_content = 0i32;
    for sec in Section::ALL {
        let mut rows = Vec::new();
        section_rows(sec, 0, s, &mut rows);
        let h = (CONTENT_HEADER_H as f32 * s) as i32 + rows.len() as i32 * (ROW_H as f32 * s) as i32;
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
