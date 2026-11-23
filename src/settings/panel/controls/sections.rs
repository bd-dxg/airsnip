// SPDX-License-Identifier: GPL-3.0-only
//! 分类内容行（M3）：每一页要放哪些行、按什么几何排。
//! 从 `layout` 拆出（ADR 0041 的「保存」页与快捷键子页签把那边撑过 300 行）：
//! 几何原语（`mk_row` 等）留在 `layout`，这里只负责「哪一页有哪些行」。

use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::SelRect;

use super::toolbar_layout::toolbar_rows;
use super::{
    CELLPAD, GRID_GAP, H, Item, KEY_ROW_H, KeyGroup, KeyRowSpec, NAV_W, PAD, QR_ROW_H, ROW_H, RowId, RowKind, Section,
    TAB_H, TAB_PAD, W, mk_key_cell, mk_row, mk_row_tall, mk_row_wide, rows_of,
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
            push(
                RowId::Autostart,
                RowKind::Toggle,
                K::GeneralAutostart.text(),
                &mut y,
                items,
            );
            push(
                RowId::ConfirmCancel,
                RowKind::Toggle,
                K::GeneralConfirmCancel.text(),
                &mut y,
                items,
            );
            // 界面语言（I18N-1 / ADR 0047）：一行真下拉，选项 = 跟随系统 + 四门语言。
            push(RowId::Lang, RowKind::Select, K::GeneralLanguage.text(), &mut y, items);
            // 贴图位置与双击动作都是一行开关（ADR 0036）：行数不变，行高也没变，
            // 但面板少一行就少 46px——之前“原位置 / 屏幕中间”占两行单选。
            // 这两行的**显示文案随开关状态变**（`draw::toggle_label`），这里写默认（关）态的文字作兼底。
            push(
                RowId::PinPosCenter,
                RowKind::Toggle,
                K::GeneralPinOriginal.text(),
                &mut y,
                items,
            );
            push(
                RowId::DoubleClick,
                RowKind::Toggle,
                K::GeneralDoubleClickCopy.text(),
                &mut y,
                items,
            );
            // 窗口吸附（CAP-6 / ADR 0037，issue #62）：单一布尔开关，文案不随状态变
            push(
                RowId::WindowSnap,
                RowKind::Toggle,
                K::GeneralWindowSnap.text(),
                &mut y,
                items,
            );
            // 控件级捕捉（ADR 0039）：依赖窗口吸附；关掉后层级止于整窗
            push(
                RowId::ControlSnap,
                RowKind::Toggle,
                K::GeneralControlSnap.text(),
                &mut y,
                items,
            );
            // 长截图日志（SYS-9 / ADR 0046，issue #200）：日志**常开**，不设开关——
            // “先让用户去开个开关”只是把「不会设环境变量」换成「没开开关」，问题照旧。
            // 这里只给「把文件取出来」的入口；标签兼写隐私边界（不含密码 / 可删）。
            push(
                RowId::OpenLogFolder,
                RowKind::Button,
                K::GeneralOpenLogFolder.text(),
                &mut y,
                items,
            );
        }
        Section::Toolbar => toolbar_rows(y, s, items),
        Section::Hotkeys => hotkey_rows(y0, s, group, bar, items),
        Section::Save => save_rows(y, s, items),
        Section::Appearance => {
            push(
                RowId::ThemeFollow,
                RowKind::Radio,
                K::ThemeFollowSystem.text(),
                &mut y,
                items,
            );
            push(RowId::ThemeLight, RowKind::Radio, K::ThemeLight.text(), &mut y, items);
            push(RowId::ThemeDark, RowKind::Radio, K::ThemeDark.text(), &mut y, items);
            for m in crate::settings::UiScale::ALL {
                push(super::ui_scale_row(m), RowKind::Radio, m.title(), &mut y, items);
            }
        }
        Section::About => {
            push(RowId::About, RowKind::About, K::AboutAppName.text(), &mut y, items);
            push(RowId::Repo, RowKind::Link, K::AboutRepo.text(), &mut y, items);
            push(RowId::Licenses, RowKind::Link, K::AboutLicenses.text(), &mut y, items);
            push(
                RowId::CheckUpdate,
                RowKind::Button,
                K::AboutCheckUpdate.text(),
                &mut y,
                items,
            );
            push(
                RowId::AutoCheckUpdate,
                RowKind::Toggle,
                K::AboutAutoCheck.text(),
                &mut y,
                items,
            );
            let qh = (QR_ROW_H as f32 * s) as i32;
            items.push(Item::Row(mk_row_tall(
                RowId::Reward,
                RowKind::Qr,
                K::AboutReward.text(),
                y,
                s,
                qh,
            )));
        }
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
        items.push(Item::Row(mk_key_cell(spec.id, spec.label.text(), x, y, cell_w, s)));
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
        K::KeyHintLabel.text(),
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
    push(RowId::SaveNotify, RowKind::Toggle, K::SaveNotify.text(), &mut y, items);
    push(RowId::SaveCopy, RowKind::Toggle, K::SaveAlsoCopy.text(), &mut y, items);
    // 保存位置与预览都撑到内容区右缘：目录与完整路径都要看得见，不能被 260px 控件区截成尾巴
    let wide = |id, kind, label: &'static str, y: &mut i32, items: &mut Vec<Item>| {
        let spec = crate::render::text::FontSpec::new(16.0 * s);
        let (lw, _) = crate::render::text::measure(label, &spec);
        items.push(Item::Row(mk_row_wide(id, kind, label, lw, *y, s)));
        *y += (ROW_H as f32 * s) as i32;
    };
    wide(RowId::QuickDir, RowKind::Readonly, K::SaveDir.text(), &mut y, items);
    wide(RowId::QuickName, RowKind::TextInput, K::SaveName.text(), &mut y, items);
    push(RowId::QuickFormat, RowKind::Select, K::SaveFormat.text(), &mut y, items);
    push(
        RowId::QuickOnExist,
        RowKind::Select,
        K::SaveOnExist.text(),
        &mut y,
        items,
    );
    wide(
        RowId::QuickPreview,
        RowKind::Readonly,
        K::SavePreview.text(),
        &mut y,
        items,
    );
    push(RowId::SaveOpenFolder, RowKind::Button, "", &mut y, items);
    push(RowId::VarHelp, RowKind::Button, "", &mut y, items);
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
    /// 整行标签的可用宽：内容区左到控件区（固定 260）左。
    fn label_room(s: f32) -> f32 {
        let w = (W as f32 * s) as i32;
        let nav_w = (NAV_W as f32 * s) as i32;
        let pad = (PAD as f32 * s) as i32;
        (w - pad - (260.0 * s) as i32) as f32 - (nav_w + pad) as f32
    }

    /// 四门语言 × 四档缩放：与右侧控件区并排显示的行标签都不得溢到控件区。
    /// 控件重叠是「量宽没跟语言走」最直接的后果，而它只能在真字体下量出来。
    #[test]
    fn row_labels_do_not_overlap_controls() {
        // 与 260px 控件区并排出现的行标签（QuickDir / QuickName / QuickPreview 等
        // wide 行例外：它们的控件区撑到内容区右缘）
        const ROW_LABELS: &[K] = &[
            K::GeneralAutostart,
            K::GeneralConfirmCancel,
            K::GeneralLanguage,
            K::GeneralPinOriginal,
            K::GeneralPinCenter,
            K::GeneralDoubleClickCopy,
            K::GeneralDoubleClickPin,
            K::GeneralWindowSnap,
            K::GeneralControlSnap,
            K::GeneralOpenLogFolder,
            K::SaveNotify,
            K::SaveAlsoCopy,
            K::SaveFormat,
            K::SaveOnExist,
            K::ThemeFollowSystem,
            K::ThemeLight,
            K::ThemeDark,
            K::ScaleAuto,
            K::Scale100,
            K::Scale125,
            K::Scale150,
            K::Scale175,
            K::Scale200,
            K::AboutAppName,
            K::AboutRepo,
            K::AboutLicenses,
            K::AboutCheckUpdate,
            K::AboutAutoCheck,
            K::AboutReward,
            K::KeyHintLabel,
        ];
        for lang in Lang::ALL {
            for s in [1.0f32, 1.25, 1.5, 2.0] {
                let spec = label_spec(lang, s);
                // 留 8px 间隙，免贴到控件上
                let room = label_room(s) - 8.0 * s;
                for k in ROW_LABELS {
                    let label = crate::i18n::text(lang, crate::i18n::Key::Settings(*k));
                    let (lw, _) = text::measure(label, &spec);
                    assert!(
                        lw <= room,
                        "{lang:?} @{s}x：行标签「{label}」量宽 {lw:.1} 超过可用 {room:.1}"
                    );
                }
            }
        }
    }
}
