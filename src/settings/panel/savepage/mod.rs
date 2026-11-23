// SPDX-License-Identifier: GPL-3.0-only
//! 「保存」页（OUT-2 / ADR 0041）：快速保存设置的行级逻辑——预览文本、行绘制分发、
//! 点击分发（开关 / 目录选择 / 输入框 / 下拉 / 按钮）。
//! 三个真下拉在 `select`，变量说明浮层在 `help`，输入框本体在 `super::textbox`。

use std::path::Path;

use tiny_skia::Pixmap;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK};

use crate::i18n::SettingsKey as K;
use crate::output::quick;
use crate::render::text as dwrite;

use super::Panel;
use super::controls::{Row, RowId};
use super::draw::draw_text;

mod help;
mod select;

pub(super) use help::draw_help;
pub(super) use select::{draw_list, list_hover_at, on_list_click};
// 只给 `render_tests` 算下拉列表矩形用（绘制 / 命中共用同一份几何）
#[cfg(test)]
pub(super) use select::list_rect;

/// 保存目录（展开 `%VAR%` 后的显示文本；目录段支持变量是给迁移来的旧配置用的）。
pub(super) fn dir_text(panel: &Panel) -> String {
    quick::expand_env(&panel.config.quick_save.dir_or_default())
}

/// 文件名模板原文（不含扩展名，未渲染）。
pub(super) fn file_name(panel: &Panel) -> String {
    panel.config.quick_save.name_or_default()
}

/// 预览：此刻渲染出的完整路径（目录 + 文件名 + 扩展名）。与真正保存共用同一套替换器。
pub(super) fn preview(panel: &Panel) -> String {
    quick::render(
        &panel.config.quick_save.template(),
        &panel.preview_time,
        panel.config.quick_save.format.ext(),
    )
}

/// 下拉 / 只读值（保存位置、预览）的绘制（输入框与按钮各有自己的模块）。
pub(super) fn draw(pixmap: &mut Pixmap, panel: &Panel, row: &Row) {
    match row.id {
        // 下拉行：保存页的格式 / 同名策略，与通用页的界面语言（I18N-1 / ADR 0047）
        RowId::QuickFormat | RowId::QuickOnExist | RowId::Lang => {
            let open = panel.open_select == Some(row.id);
            select::draw_select(pixmap, panel, row, select::select_value(panel, row.id), open);
        }
        // 保存位置：目录靠资源管理器选，这里只显示（尾部截断，保住末级目录名）
        RowId::QuickDir => draw_tail_value(pixmap, panel, row, &dir_text(panel)),
        RowId::QuickPreview => draw_tail_value(pixmap, panel, row, &preview(panel)),
        _ => {}
    }
}

/// 只读行右对齐的尾部截断文本（目录与预览共用同一套排版）。
fn draw_tail_value(pixmap: &mut Pixmap, panel: &Panel, row: &Row, text: &str) {
    let size = 15.0 * panel.scale;
    let spec = dwrite::FontSpec::new(size);
    let text = fit_tail(text, row.control.w as f32, &spec);
    let (w, _) = dwrite::measure(&text, &spec);
    let x = row.control.x + row.control.w - w as i32;
    let y = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    draw_text(pixmap, x as f32, y, &text, size, panel.palette.text_dim);
}

/// 尾部保留的截断（路径的目录头被省略、文件名保住；「预览」首先要能认出存成什么名）。
pub(super) fn fit_tail(text: &str, max_w: f32, spec: &dwrite::FontSpec) -> String {
    if dwrite::measure(text, spec).0 <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut keep = chars.len();
    while keep > 1 {
        keep -= 1;
        let tail: String = chars[chars.len() - keep..].iter().collect();
        let cand = format!("…{tail}");
        if dwrite::measure(&cand, spec).0 <= max_w {
            return cand;
        }
    }
    "…".to_string()
}

/// 「保存」页的点击：返回 `true` 表示已处理（其余行交给 `interact` 的通用分支）。
pub(super) fn on_click(panel: &mut Panel, id: RowId) -> bool {
    match id {
        RowId::SaveNotify => {
            panel.config.quick_save.notify = !panel.config.quick_save.notify;
            panel.config_dirty = true;
        }
        RowId::SaveCopy => {
            panel.config.quick_save.also_copy = !panel.config.quick_save.also_copy;
            panel.config_dirty = true;
        }
        // 文件名模板：点进去编辑（可含变量，不含扩展名）
        RowId::QuickName => super::textbox::begin(panel),
        // 保存位置：用资源管理器选目录（ADR 0041：路径不靠手打）
        RowId::QuickDir => {
            let current = Path::new(&dir_text(panel)).to_path_buf();
            if let Some(dir) = crate::app::filedlg::pick_folder(panel.hwnd, Some(&current)) {
                panel.config.quick_save.dir = Some(dir.display().to_string());
                panel.config_dirty = true;
            }
        }
        // 下拉：点开列表（再点一次收起），选项由 `on_list_click` 写回
        RowId::QuickFormat | RowId::QuickOnExist | RowId::Lang => {
            panel.open_select = if panel.open_select == Some(id) { None } else { Some(id) };
            panel.select_hover = None;
        }
        // 预览行：**纯展示**，点击什么也不做（弹资源管理器只走「打开文件夹」按钮；
        // ADR 0041 修订：曾经点预览会打开目录/定位上一张，误点代价大且不是用户预期）。
        RowId::QuickPreview => return false,
        RowId::SaveOpenFolder => {
            // 目录还没被创建时 `ShellExecuteW` 静默失败，用户点了会以为按钮坏了
            let dir = quick::rendered_dir(&preview(panel));
            if dir.is_dir() {
                quick::open_folder(&dir);
            } else {
                crate::app::message_box(
                    Some(panel.hwnd),
                    &crate::i18n::t_with(
                        crate::i18n::Key::Settings(K::SaveDirMissing),
                        &[("dir", &dir.display().to_string())],
                    ),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
        }
        RowId::VarHelp => panel.var_help = !panel.var_help,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SaveFormat;
    use crate::settings::panel::controls::Section;

    /// 目录与文件名分开后，预览只由这两段与格式决定（换格式时扩展名跟着变）。
    #[test]
    fn preview_follows_dir_name_and_format() {
        let mut p = Panel::for_test(Section::Save, crate::theme::Theme::Light);
        p.config.quick_save.dir = Some(r"D:\shots".into());
        p.config.quick_save.name = Some("shot_$yyyy$".into());
        p.config.quick_save.format = SaveFormat::Webp;
        let rendered = quick::render(
            &p.config.quick_save.template(),
            &quick::TimeParts {
                y: 2026,
                mo: 1,
                d: 2,
                h: 3,
                mi: 4,
                s: 5,
                unix_ms: 0,
            },
            p.config.quick_save.format.ext(),
        );
        assert_eq!(rendered, r"D:\shots\shot_2026.webp");
        assert!(preview(&p).ends_with(".webp"), "预览行应同源：{}", preview(&p));
    }

    /// 预览尾部截断：超宽时保留结尾（文件名），且不超宽；不超宽时原样返回。
    #[test]
    fn fit_tail_keeps_file_name() {
        let spec = dwrite::FontSpec::new(15.0);
        let long = r"C:\Users\someone\Pictures\airsnip\airsnip_2026-11-16_07-49-46.png";
        let (w, _) = dwrite::measure(long, &spec);
        let out = fit_tail(long, w / 2.0, &spec);
        assert!(out.starts_with('…'), "{out}");
        assert!(out.ends_with(".png"), "{out}");
        assert!(dwrite::measure(&out, &spec).0 <= w / 2.0);
        assert_eq!(fit_tail("a.png", 500.0, &spec), "a.png");
    }
}
