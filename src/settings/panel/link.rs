// SPDX-License-Identifier: GPL-3.0-only
//! 右侧控件行绘制（M5/SYS-2/SYS-7）：关于页的外部链接入口（点击用浏览器打开，见 `interact`）、
//! 版本更新按钮与快捷键页的「恢复默认」操作按钮。从 `draw` 拆出，保证单文件不超过 300 行。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::render::text as dwrite;
use crate::update::state;

use super::Panel;
use super::controls::RowId;

/// 外部链接行：右侧强调色文字 + 「›」箭头，文案按行类型取。
pub(super) fn draw_link(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = match row.id {
        // 仓库地址与官网域名是 ASCII 标识，不翻译
        RowId::Repo => "github.com/bd-dxg/airsnip \u{203a}",
        RowId::Homepage => "airsnip.bddxg.top \u{203a}",
        RowId::Licenses => K::AboutLicensesLink.text(),
        _ => "",
    };
    draw_trailing(pixmap, panel, row, text, panel.palette.accent);
}

/// 按钮行（`RowKind::Button`）的按钮文案与状态：固定文案的行走 `fixed_button_text`，
/// 其余只有「版本更新」一行，随更新状态变。
fn button_state(row: &super::controls::Row) -> (String, bool, bool) {
    if let Some(t) = fixed_button_text(row.id) {
        return (t.to_string(), true, false);
    }
    let phase = state::phase();
    let (text, enabled) = state::button(&phase);
    (text, enabled, state::button_emphasis(&phase))
}

/// 按钮行的固定文案（不随更新状态变）：快捷键页的「恢复默认」、保存页的三个按钮
/// （OUT-2 / ADR 0041）与通用页的「打开日志文件夹」（SYS-9 / ADR 0046）。
///
/// 注意：这里必须列出**所有**固定文案的按钮行。漏一项会让那个按钮落到
/// 「随更新状态变」分支，显示成「检查更新 / 已是最新」之类——快捷键页的
/// 「恢复默认」就这样被误显示成「已是最新」过一次。
pub(super) fn fixed_button_text(id: RowId) -> Option<&'static str> {
    match id {
        RowId::ResetKeys => Some(K::KeyResetAll.text()),
        RowId::SaveOpenFolder => Some(K::SaveOpenFolder.text()),
        RowId::OpenLogFolder => Some(K::GeneralOpenLogFolderBtn.text()),
        RowId::VarHelp => Some(K::SaveVarHelp.text()),
        _ => None,
    }
}

/// 「关于」行右侧的反馈按钮文案（SYS-8）。
pub(super) fn about_btn_text() -> &'static str {
    K::AboutFeedback.text()
}

/// 按钮几何（纯计算，便于测试）：高度与快捷键行的键帽一致（30px 基准），宽度按文字量算，
/// 右对齐、不超出控件区。
fn btn_geom(control: SelRect, text_w: f32, s: f32) -> SelRect {
    let pad = 16.0 * s;
    let bh = (30.0 * s) as i32;
    let bw = ((text_w + pad * 2.0) as i32).clamp((90.0 * s) as i32, control.w);
    SelRect {
        x: control.x + control.w - bw,
        y: control.y + (control.h - bh) / 2,
        w: bw,
        h: bh,
    }
}

/// 按钮行的按钮矩形：绘制与鼠标命中共用这一份几何，保证「看得见的边界 == 点得中的边界」。
/// 命中测试必须用它——曾经整行可点，按钮外一点就触发（比按钮自身宽得多）。
pub(super) fn button_rect(panel: &Panel, row: &super::controls::Row) -> SelRect {
    button_geom(panel, row).0
}

/// 按钮矩形 + 文案宽度（绘制需要宽度来居中文字，不重复测量）。
fn button_geom(panel: &Panel, row: &super::controls::Row) -> (SelRect, f32) {
    let s = panel.scale;
    let (text, ..) = button_state(row);
    let (tw, _) = dwrite::measure(&text, &dwrite::FontSpec::new(15.0 * s));
    (btn_geom(row.control, tw, s), tw)
}

/// 「关于」行反馈按钮的矩形（SYS-8）：绘制与命中共用一份几何。
pub(super) fn about_button_rect(panel: &Panel, row: &super::controls::Row) -> SelRect {
    let (tw, _) = dwrite::measure(about_btn_text(), &dwrite::FontSpec::new(15.0 * panel.scale));
    btn_geom(row.control, tw, panel.scale)
}

/// 「关于」行右侧的反馈按钮（SYS-8）：与更新按钮同一套外观。
pub(super) fn draw_about_button(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, hovered: bool) {
    let btn = about_button_rect(panel, row);
    paint_button(pixmap, panel, btn, about_btn_text(), false, hovered);
}

/// 按钮外观（圆角底 + 边框 + 居中文字）；行按钮与关于行反馈按钮共用。
fn paint_button(pixmap: &mut Pixmap, panel: &Panel, btn: SelRect, text: &str, emphasis: bool, hovered: bool) {
    let p = panel.palette;
    let s = panel.scale;
    let (fill, border, fg) = if emphasis || hovered {
        // 悬停，或「有新版本 / 失败可重试」：强调底色
        (p.accent_soft, p.accent, p.accent)
    } else {
        ([p.bg[0], p.bg[1], p.bg[2], 255], p.border, p.text)
    };
    rounded_rect(pixmap, btn, (6.0 * s) as f32, fill, Some(border));
    let size = 15.0 * s;
    let (tw, _) = dwrite::measure(text, &dwrite::FontSpec::new(size));
    let x = btn.x as f32 + (btn.w as f32 - tw) / 2.0;
    let y = btn.y as f32 + (btn.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x, y),
        text,
        &dwrite::FontSpec::new(size),
        fg,
        Transform::identity(),
    );
}

/// 按钮风格行：真正的按钮外观（圆角底 + 边框 + 居中文字）。
/// 「恢复默认」（快捷键页）为固定文案；「版本更新」（关于页）文案随状态变、忙时置灰。
/// `hovered`：鼠标悬停在按钮上时给强调底色，明确「可点」（悬停在行内空白处不算）。
pub(super) fn draw_button(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, hovered: bool) {
    let (text, enabled, emphasis) = button_state(row);
    let (btn, _) = button_geom(panel, row);
    if !enabled {
        // 忙：素色底 + 灰字，看起来就是「点不了」
        let p = panel.palette;
        let s = panel.scale;
        rounded_rect(
            pixmap,
            btn,
            (6.0 * s) as f32,
            [p.bg[0], p.bg[1], p.bg[2], 255],
            Some(p.border),
        );
        let size = 15.0 * s;
        let (tw, _) = dwrite::measure(&text, &dwrite::FontSpec::new(size));
        let x = btn.x as f32 + (btn.w as f32 - tw) / 2.0;
        let y = btn.y as f32 + (btn.h as f32 - size * 1.2) / 2.0;
        dwrite::draw(
            pixmap,
            Point::new(x, y),
            &text,
            &dwrite::FontSpec::new(size),
            p.text_dim,
            Transform::identity(),
        );
        return;
    }
    paint_button(pixmap, panel, btn, &text, emphasis, hovered);
}

/// 右侧对齐的指定颜色文本。
fn draw_trailing(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, text: &str, color: [u8; 4]) {
    let size = 16.0 * panel.scale;
    let (w, _) = dwrite::measure(text, &dwrite::FontSpec::new(size));
    let x = row.control.x + row.control.w as i32 - w as i32;
    let y = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x as f32, y),
        text,
        &dwrite::FontSpec::new(size),
        color,
        Transform::identity(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::panel::controls::{Item, Section, layout};

    /// 固定文案的按钮不能被更新状态盖掉（回归：快捷键页「恢复默认」曾显示成「已是最新」）。
    #[test]
    fn fixed_buttons_keep_their_text() {
        let mut seen = Vec::new();
        for sec in Section::ALL {
            for item in layout(sec, 1.0).items {
                let Item::Row(r) = item else { continue };
                if r.kind != super::super::controls::RowKind::Button {
                    continue;
                }
                let text = button_state(&r).0;
                match r.id {
                    RowId::ResetKeys => assert_eq!(text, "全部恢复默认", "快捷键页按钮文案错位"),
                    RowId::SaveOpenFolder => assert_eq!(text, "打开文件夹"),
                    RowId::OpenLogFolder => assert_eq!(text, "打开日志文件夹"),
                    RowId::VarHelp => assert_eq!(text, "变量说明"),
                    // 「版本更新」随状态变，允许是任何非空文案
                    _ => assert!(!text.is_empty(), "{:?} 按钮文案为空", r.id),
                }
                seen.push(r.id);
            }
        }
        assert!(seen.contains(&RowId::ResetKeys), "快捷键页应有「全部恢复默认」按钮");
        assert!(seen.contains(&RowId::SaveOpenFolder), "保存页应有「打开文件夹」按钮");
        assert!(seen.contains(&RowId::OpenLogFolder), "通用页应有「打开日志文件夹」按钮");
    }

    /// 按钮矩形不占满行（否则会出现「按钮外一点就触发」）：右对齐、比行矮、比控件区窄。
    #[test]
    fn button_rect_stays_inside_control() {
        // 标准行的控件区：260 宽 × 46 高，起点在行右侧
        let c = SelRect {
            x: 400,
            y: 0,
            w: 260,
            h: 46,
        };
        let r = btn_geom(c, 60.0, 1.0);
        assert_eq!(r.x + r.w, c.x + c.w, "按钮应与控件区右对齐");
        assert!(r.x > c.x, "按钮左缘不应贴到控件区左缘：{r:?}");
        assert!(r.h < c.h, "按钮应比行矮：{r:?}");
        // 文案极长时钳到控件区宽度（仍不越出控件区、更不占满整行）
        let wide = btn_geom(c, 999.0, 1.0);
        assert_eq!(wide.w, c.w);
        assert_eq!(wide.x + wide.w, c.x + c.w);
        // 高 DPI 下同样成立（控件区几何也跟着缩放）
        let hi_c = SelRect {
            x: 800,
            y: 0,
            w: 520,
            h: 92,
        };
        let hi = btn_geom(hi_c, 120.0, 2.0);
        assert_eq!(hi.x + hi.w, hi_c.x + hi_c.w);
        assert!(hi.x > hi_c.x && hi.h < hi_c.h);
    }

    /// 各状态的按钮文案在 100% 缩放下都能装进控件区（按钮左缘不会越过标签区）。
    #[test]
    fn button_text_fits_control_width() {
        let phases = [
            state::Phase::Idle,
            state::Phase::Checking,
            state::Phase::UpToDate,
            state::Phase::Available {
                version: "0.7.0".into(),
            },
            state::Phase::Downloading {
                version: "0.7.0".into(),
                percent: 100,
            },
            state::Phase::Installing {
                version: "0.7.0".into(),
            },
            state::Phase::Failed("x".into()),
        ];
        // 控件区宽 260 - 左右 padding 32（与 draw_button 的 pad 一致）
        let limit = 260.0 - 32.0;
        for ph in phases {
            let (text, _) = state::button(&ph);
            let (w, _) = dwrite::measure(&text, &dwrite::FontSpec::new(15.0));
            assert!(w <= limit, "{ph:?} 按钮文案过宽：{w} > {limit}");
        }
    }

    /// 「关于」行反馈按钮文案（SYS-8）也要能装进控件区，不能把版本文字挤掉。
    #[test]
    fn about_button_fits_control_width() {
        let c = SelRect {
            x: 400,
            y: 0,
            w: 260,
            h: 46,
        };
        let (tw, _) = dwrite::measure(about_btn_text(), &dwrite::FontSpec::new(15.0));
        let r = btn_geom(c, tw, 1.0);
        assert!(r.x > c.x && r.x + r.w <= c.x + c.w, "{r:?}");
        // 版本文字右对齐到按钮左缘前仍应留得下：`v0.12.0 · GPL-3.0`
        let (vw, _) = dwrite::measure("v0.12.0 · GPL-3.0", &dwrite::FontSpec::new(16.0));
        let left_after_text = r.x as f32 - 12.0 - vw;
        assert!(left_after_text > c.x as f32, "版本文字会越过控件区左缘");
    }
}
