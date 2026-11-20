// SPDX-License-Identifier: GPL-3.0-only
//! 分类内容行（M3）：每一页要放哪些行、按什么几何排。
//! 从 `layout` 拆出（ADR 0041 的「保存」页与快捷键子页签把那边撑过 300 行）：
//! 几何原语（`mk_row` 等）留在 `layout`，这里只负责「哪一页有哪些行」。

use crate::overlay::geometry::SelRect;

use crate::settings::ToolbarGroup;

use super::{
    CELLPAD, GRID_GAP, GROUP_H, H, Item, KEY_ROW_H, KeyGroup, KeyRowSpec, NAV_W, PAD, QR_ROW_H, ROW_H, RowId, RowKind,
    Section, TAB_H, TAB_PAD, W, mk_key_cell, mk_row, mk_row_tall, mk_row_wide, mk_toggle_cell, rows_of,
};

/// 指定分类的内容行（`group` 只影响快捷键页：画哪一组的键帽；
/// `bar` 回传子页签分段控件的外框）。
pub(super) fn section_rows(
    section: Section,
    y0: i32,
    s: f32,
    group: KeyGroup,
    bar: &mut Option<SelRect>,
    items: &mut Vec<Item>,
) {
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
            // 贴图位置与双击动作都是一行开关（ADR 0036）：行数不变，行高也没变，
            // 但面板少一行就少 46px——之前“原位置 / 屏幕中间”占两行单选。
            // 这两行的**显示文案随开关状态变**（`draw::toggle_label`），这里写默认（关）态的文字作兼底。
            push(
                RowId::PinPosCenter,
                RowKind::Toggle,
                "贴图位置·截图原位置",
                &mut y,
                items,
            );
            push(
                RowId::DoubleClick,
                RowKind::Toggle,
                "双击选区空白·复制截图",
                &mut y,
                items,
            );
            // 窗口吸附（CAP-6 / ADR 0037，issue #62）：单一布尔开关，文案不随状态变
            push(RowId::WindowSnap, RowKind::Toggle, "窗口吸附", &mut y, items);
            // 控件级捕捉（ADR 0039）：依赖窗口吸附；关掉后层级止于整窗
            push(RowId::ControlSnap, RowKind::Toggle, "控件级捕捉", &mut y, items);
            // 长截图日志（SYS-9 / ADR 0046，issue #200）：日志**常开**，不设开关——
            // “先让用户去开个开关”只是把「不会设环境变量」换成「没开开关」，问题照旧。
            // 这里只给「把文件取出来」的入口；标签兼写隐私边界（不含密码 / 可删）。
            push(
                RowId::OpenLogFolder,
                RowKind::Button,
                "长截图日志（不含密码，可删）",
                &mut y,
                items,
            );
        }
        Section::Toolbar => toolbar_rows(y, s, items),
        Section::Hotkeys => hotkey_rows(y0, s, group, bar, items),
        Section::Save => save_rows(y, s, items),
        Section::Appearance => {
            push(RowId::ThemeFollow, RowKind::Radio, "跟随系统", &mut y, items);
            push(RowId::ThemeLight, RowKind::Radio, "浅色", &mut y, items);
            push(RowId::ThemeDark, RowKind::Radio, "深色", &mut y, items);
            for m in crate::settings::UiScale::ALL {
                push(super::ui_scale_row(m), RowKind::Radio, m.title(), &mut y, items);
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

/// 「工具栏」页（EDT-16）：五组 + 组内元素多列紧凑排布。
/// 面板没有滚动条，五组 + 组内元素必须落在固定高内。
/// 高度：5 个组头（GROUP_H=24）+ 7 行元素（ROW_H=46）= 442，加节标题 40 → 482 < 520，
/// 页脚提示还能留 12px（有 `toolbar_page::tests::hint_sits_below_last_row` 守门）。
/// 工具组从 10 项到 11 项（ADR 0036 新增「移动」）后是 3 行，组头行高从 26 收到 24 就是为此腾的。
fn toolbar_rows(mut y: i32, s: f32, items: &mut Vec<Item>) {
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

/// 快捷键页（ADR 0041）：一排**分段控件**式子页签 + 当前组的两列键帽（标签左、键帽右）。
fn hotkey_rows(y0: i32, s: f32, group: KeyGroup, bar: &mut Option<SelRect>, items: &mut Vec<Item>) {
    let w = (W as f32 * s) as i32;
    let nav_w = (NAV_W as f32 * s) as i32;
    let pad = (PAD as f32 * s) as i32;
    let cellpad = (CELLPAD as f32 * s) as i32;
    let gap = (GRID_GAP as f32 * s) as i32;
    let row_h = (KEY_ROW_H as f32 * s) as i32;
    // 单元格左缘让出 CELLPAD，使单元格内标签与整行标签（NAV_W+PAD）对齐
    let cell_x0 = nav_w + pad - cellpad;
    // 子页签：三等分等宽单元格拼成一条分段控件（外框由 `draw` 统一画）
    let tab_h = (TAB_H as f32 * s) as i32;
    let tab_pad = (TAB_PAD as f32 * s) as i32;
    let spec = crate::render::text::FontSpec::new(15.0 * s);
    let mut max_text = 0f32;
    for g in KeyGroup::ALL {
        max_text = max_text.max(crate::render::text::measure(g.title(), &spec).0);
    }
    let tab_w = max_text as i32 + tab_pad * 2;
    let bar_w = tab_w * KeyGroup::ALL.len() as i32;
    for (i, g) in KeyGroup::ALL.iter().enumerate() {
        let x = cell_x0 + i as i32 * tab_w;
        items.push(Item::Tab(super::Tab {
            group: *g,
            rect: SelRect {
                x,
                y: y0,
                w: tab_w,
                h: tab_h,
            },
        }));
    }
    *bar = Some(SelRect {
        x: cell_x0,
        y: y0,
        w: bar_w,
        h: tab_h,
    });
    let rows: Vec<&KeyRowSpec> = rows_of(group).collect();
    let cols = 2usize;
    let cell_w = (w - pad + cellpad - cell_x0 - gap * (cols as i32 - 1)) / cols as i32;
    for (i, spec) in rows.iter().enumerate() {
        let col = (i % cols) as i32;
        let row = (i / cols) as i32;
        let x = cell_x0 + col * (cell_w + gap);
        let y = y0 + tab_h + row * row_h;
        items.push(Item::Row(mk_key_cell(spec.id, spec.label, x, y, cell_w, s)));
    }
    // 底部两行**固定锚在内容区右下角**（ADR 0041）：三组行数不同（2/3/6 行），
    // 跟着最后一行走的话切页签时按钮与提示会上下跳。贴底部预留 `PAD` 边距，
    // 两行都右对齐（`mk_row` 的 control 靠右）。
    // 高度上安全：工具组 6 行最后一行底在 348，而底块从 404 起，不重叠。
    let footer_y = (H as f32 * s) as i32 - row_h * 2 - (PAD as f32 * s) as i32;
    items.push(Item::Row(mk_row(RowId::ResetKeys, RowKind::Button, "", footer_y, s)));
    items.push(Item::Row(mk_row(
        RowId::Widenote,
        RowKind::Readonly,
        "提示",
        footer_y + row_h,
        s,
    )));
}

/// 「保存」页（OUT-2 / ADR 0041）：快速保存设置。
/// 路径拆成两段——**目录靠资源管理器选**（只读一行，点击即弹文件夹选择器），
/// **文件名才可手改**（可含变量）。
fn save_rows(mut y: i32, s: f32, items: &mut Vec<Item>) {
    let push = |id: RowId, kind: RowKind, label: &'static str, y: &mut i32, items: &mut Vec<Item>| {
        items.push(Item::Row(mk_row(id, kind, label, *y, s)));
        *y += (ROW_H as f32 * s) as i32;
    };
    push(RowId::SaveNotify, RowKind::Toggle, "快速保存后显示通知", &mut y, items);
    push(RowId::SaveCopy, RowKind::Toggle, "同时复制到剪贴板", &mut y, items);
    // 保存位置与预览都撑到内容区右缘：目录与完整路径都要看得见，不能被 260px 控件区截成尾巴
    let wide = |id, kind, label: &'static str, y: &mut i32, items: &mut Vec<Item>| {
        let spec = crate::render::text::FontSpec::new(16.0 * s);
        let (lw, _) = crate::render::text::measure(label, &spec);
        items.push(Item::Row(mk_row_wide(id, kind, label, lw, *y, s)));
        *y += (ROW_H as f32 * s) as i32;
    };
    wide(
        RowId::QuickDir,
        RowKind::Readonly,
        "保存位置（点击更换）",
        &mut y,
        items,
    );
    wide(RowId::QuickName, RowKind::TextInput, "文件名", &mut y, items);
    push(RowId::QuickFormat, RowKind::Select, "格式", &mut y, items);
    push(RowId::QuickOnExist, RowKind::Select, "如果文件存在", &mut y, items);
    wide(RowId::QuickPreview, RowKind::Readonly, "预览", &mut y, items);
    push(RowId::SaveOpenFolder, RowKind::Button, "", &mut y, items);
    push(RowId::VarHelp, RowKind::Button, "", &mut y, items);
}
