// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键行绘制与清空（SYS-2）：键帽 + 行内清除按钮、「恢复默认」操作。
//! 从 `draw`/`interact` 拆出，避免单文件超限；模块名沿用「快捷键行」含义。
//!
//! 视觉约定：键帽左对齐、清除按钮右对齐，两者之间留 `CLEAR_GAP`；
//! 已清空的项键帽显示灰色「未设置」，清除按钮同步置灰（避免连点）。

use tiny_skia::Pixmap;

use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{rounded_rect, rounded_rect_outline};
use crate::render::icon::{self, Icon};
use crate::render::text as dwrite;

use super::Panel;
use super::controls::{CLEAR_GAP, KEY_ROWS, Row, RowId, spec};
use super::draw::draw_text;

/// 行内命中区域：键帽区与清除按钮区分开，点击行为不同。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Zone {
    /// 行内空白（标签等）：不执行动作。
    Label,
    /// 键帽区：进入捕获态。
    Control,
    /// 清除按钮：直接清空。
    Clear,
}

/// 是否为快捷键行（热键 + 工具切换键，名单见 `controls::KEY_ROWS`）。
/// 这些行共用「点键帽捕获 / 点清除按钮清空」的交互。
pub(super) fn is_key_row(id: RowId) -> bool {
    spec(id).is_some()
}

/// 绘制键帽与清除按钮。`hovered`：鼠标悬停该行时（或该行处于捕获态时）才显现清除按钮，
/// 否则满屏垃圾桶太噪。
pub(super) fn draw(pixmap: &mut Pixmap, panel: &Panel, row: &Row, hovered: bool) {
    let p = panel.palette;
    let s = panel.scale;
    let capturing = panel.capturing == Some(row.id);
    let bound = row_key(panel, row.id) != crate::settings::UNBOUND;
    let c = &row.control;
    let kh = (30.0 * s) as i32;
    let gap = (CLEAR_GAP as f32 * s) as i32;
    let kb = SelRect {
        x: c.x,
        y: c.y + (c.h - kh) / 2,
        w: c.w - row.clear.w - gap,
        h: kh,
    };
    let fill = if capturing {
        p.accent_soft
    } else {
        [p.bg[0], p.bg[1], p.bg[2], 255]
    };
    let border = if capturing { p.accent } else { p.border };
    rounded_rect(pixmap, kb, (6.0 * s) as f32, fill, Some(border));

    let text = keycap_text(panel, row.id);
    let size = 16.0 * s;
    let color = if capturing {
        p.accent
    } else if bound {
        p.text
    } else {
        p.text_dim
    };
    let (w, _) = dwrite::measure(&text, &dwrite::FontSpec::new(size));
    // 文字水平居中于键帽（减去按钮后的可用区）
    let x = kb.x as f32 + (kb.w as f32 - w) / 2.0;
    let y = kb.y as f32 + (kh as f32 - size * 1.2) / 2.0;
    draw_text(pixmap, x, y, &text, size, color);

    // 清除按钮：悬停该行或该行处于捕获态时才出现（等于键帽高度，右侧对齐）
    if hovered || capturing {
        rounded_rect_outline(pixmap, row.clear, (6.0 * s) as f32, p.border);
        let icon_color = if bound { p.icon } else { p.icon_dim };
        icon::draw(
            pixmap,
            Icon::Delete,
            row.clear.x as f32 + row.clear.w as f32 / 2.0,
            row.clear.y as f32 + row.clear.h as f32 / 2.0,
            row.clear.w as f32,
            icon_color,
        );
    }
}

/// 该行当前的虚拟键码（未绑定返回 `UNBOUND`）。
fn row_key(panel: &Panel, id: RowId) -> u32 {
    spec(id).map_or(crate::settings::UNBOUND, |s| s.current(&panel.config).0)
}

/// 热键按钮文本。
fn keycap_text(panel: &Panel, id: RowId) -> String {
    if panel.capturing == Some(id) {
        // 两列排布后键帽较窄，捕获提示保持简短
        return "按下新键…".into();
    }
    let Some(key) = spec(id) else { return String::new() };
    let (vk, mods) = key.current(&panel.config);
    super::controls::key_name(vk, mods)
}

impl Panel {
    /// 清空某一行的快捷键（SYS-2）：置为 `UNBOUND` 并立即生效。
    /// 已清空的项直接返回（清除按钮此时置灰，连点无副作用）。
    pub(super) fn clear_key(&mut self, id: RowId) {
        self.end_capture();
        let Some(key) = spec(id) else { return };
        if key.current(&self.config).0 == crate::settings::UNBOUND {
            return;
        }
        // 两个全局热键是唯一入口时先说清楚：清空后只能从托盘菜单触发
        if key.confirm_clear {
            let name = key.label;
            let text = format!("清除「{name}」热键后，只能从托盘菜单触发{name}。\n确定清除吗？");
            if !confirm(self.hwnd, &text) {
                return;
            }
        }
        key.apply(&mut self.config, crate::settings::UNBOUND, 0);
        self.config_dirty = true;
    }

    /// 一键恢复默认快捷键（全部键帽行还原，含已清空的项）。
    pub(super) fn reset_keys(&mut self) {
        self.end_capture();
        let text = format!(
            "恢复默认会覆盖当前全部 {} 项快捷键设置（包括已清空的项）。\n确定恢复吗？",
            KEY_ROWS.len()
        );
        if !confirm(self.hwnd, &text) {
            return;
        }
        self.config.hotkey = crate::settings::HotkeyConfig::default();
        self.config.tool_keys = crate::settings::ToolKeys::default();
        self.config_dirty = true;
    }
}

/// 是/否确认框（模态于设置面板）；确认返回 true。
fn confirm(owner: windows::Win32::Foundation::HWND, text: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONWARNING, MB_YESNO, MessageBoxW};
    use windows::core::{PCWSTR, w};
    let ws: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(ws.as_ptr()),
            w!("AirSnip"),
            MB_YESNO | MB_ICONWARNING,
        ) == IDYES
    }
}
