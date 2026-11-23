// SPDX-License-Identifier: GPL-3.0-only
//! 面板右侧「控件」类自绘（M3 / EDT-16 / OUT-2）：开关胶囊、主题单选、只读说明文本。
//! 从 `draw` 拆出（ADR 0041 的保存页与子页签把那边撑过 300 行）：
//! 那边只保留「整体构图 + 行分发」，具体控件怎么画在这里。

use tiny_skia::Pixmap;

use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{rounded_rect, rounded_rect_outline};
use crate::settings::ThemeSetting;

use super::Panel;
use super::controls::{Row, RowId};
use super::draw::draw_right_text;

/// 开关：胶囊轨道 + 白色滑钮。状态按行类型取。
pub(super) fn draw_toggle(pixmap: &mut Pixmap, panel: &Panel, row: &Row) {
    let p = panel.palette;
    let s = panel.scale;
    let on = match row.id {
        RowId::Autostart => panel.autostart_on,
        RowId::ConfirmCancel => panel.config.confirm_cancel,
        RowId::AutoCheckUpdate => panel.config.auto_check_update,
        // 贴图位置（M2b）：开 = 屏幕中间，关 = 截图原位置
        RowId::PinPosCenter => panel.config.pin_position == crate::settings::PinPosition::Center,
        // 双击选区空白（ADR 0036）：开 = 贴图，关 = 复制
        RowId::DoubleClick => panel.config.double_click_action == crate::settings::DoubleClickAction::Pin,
        // 窗口吸附（CAP-6 / ADR 0037）：开 = 悬停预览窗口矩形，关 = 只有自由框选
        RowId::WindowSnap => panel.config.window_snap,
        // 控件级捕捉（ADR 0039）：开 = 在整窗上再下钻到 UIA 子控件
        RowId::ControlSnap => panel.config.control_snap,
        // 快速保存的两个开关（OUT-2 / ADR 0041）
        RowId::SaveNotify => panel.config.quick_save.notify,
        RowId::SaveCopy => panel.config.quick_save.also_copy,
        // 工具栏元素显隐（EDT-16）：纯视觉开关，默认全开
        RowId::Toolbar(slot) => panel.config.toolbar.visible(slot),
        _ => false,
    };
    let c = &row.control;
    let tw = (super::controls::TOGGLE_W as f32 * s) as i32;
    let th = (18.0 * s) as i32;
    let tx = c.x + c.w - tw;
    let ty = c.y + (c.h - th) / 2;
    let track = SelRect {
        x: tx,
        y: ty,
        w: tw,
        h: th,
    };
    rounded_rect(
        pixmap,
        track,
        (th / 2) as f32,
        if on { p.toggle_on } else { p.toggle_off },
        None,
    );
    let kd = (14.0 * s) as i32;
    let kx = if on { tx + tw - kd - 2 } else { tx + 2 };
    let knob = SelRect {
        x: kx,
        y: ty + (th - kd) / 2,
        w: kd,
        h: kd,
    };
    rounded_rect(pixmap, knob, (kd / 2) as f32, [255, 255, 255, 255], None);
}

/// 主题单选：○ 外圈 + 选中时 ● 内圈。
pub(super) fn draw_radio(pixmap: &mut Pixmap, panel: &Panel, row: &Row) {
    let p = panel.palette;
    let s = panel.scale;
    let selected = match row.id {
        RowId::ThemeFollow => panel.config.theme == ThemeSetting::Follow,
        RowId::ThemeLight => panel.config.theme == ThemeSetting::Light,
        RowId::ThemeDark => panel.config.theme == ThemeSetting::Dark,
        // 界面缩放（CAP-5）：选项与行 ID 的映射集中在 `controls`
        id => super::controls::ui_scale_of(id).is_some_and(|m| panel.config.ui_scale == m),
    };
    let c = &row.control;
    let d = (14.0 * s) as i32;
    let ring = SelRect {
        x: c.x + c.w - d,
        y: c.y + (c.h - d) / 2,
        w: d,
        h: d,
    };
    rounded_rect_outline(
        pixmap,
        ring,
        (d / 2) as f32,
        if selected { p.accent } else { p.text_dim },
    );
    if selected {
        let inset = (d as f32 * 0.28) as i32;
        let dot = SelRect {
            x: ring.x + inset,
            y: ring.y + inset,
            w: d - inset * 2,
            h: d - inset * 2,
        };
        rounded_rect(pixmap, dot, (dot.h / 2) as f32, p.accent, None);
    }
}

/// 只读行：右侧说明文本。
pub(super) fn draw_readonly(pixmap: &mut Pixmap, panel: &Panel, row: &Row) {
    // 保存位置 / 预览两行的值由 `savepage` 绘（右对齐 + 尾部截断；ADR 0041）
    if matches!(row.id, RowId::QuickPreview | RowId::QuickDir) {
        super::savepage::draw(pixmap, panel, row);
        return;
    }
    let text = match row.id {
        RowId::Widenote => K::KeyModifierNote.text(),
        _ => "",
    };
    draw_right_text(pixmap, panel, row, text);
}
