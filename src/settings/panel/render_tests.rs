// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板的**离屏渲染**测试（ADR 0041）：真把一页画到像素图上再断言，
//! 抓「控件画了但被后画的东西盖住」这类只有肉眼才能发现的错位。
//!
//! 背景：页签文字被分段控件的容器底盖掉过一次（绘制顺序写反），
//! 只查几何的测试全绿、界面却是空的。这里用颜色种类数 / 墨水量兜住这类回归。
//! 设 `AIRSNIP_PANEL_DUMP=1` 会把每页 PNG 落到临时目录，便于人眼核对。

use super::draw::render;
use super::{Panel, Section};
use crate::settings::panel::controls::{Item, KeyGroup, Layout, RowKind, W as PANEL_W};

/// 构造一个不依赖窗口的 `Panel`（仅用于离屏渲染）。
fn panel_for_test(section: Section, theme: crate::theme::Theme, group: KeyGroup) -> Panel {
    let mut p = Panel::for_test(section, theme);
    p.hotkey_group = group;
    p
}

/// 真渲染一页；`AIRSNIP_PANEL_DUMP` 时落盘 PNG。
fn paint(panel: &Panel) -> (tiny_skia::Pixmap, Layout) {
    let layout = panel.layout();
    let mut pm = tiny_skia::Pixmap::new(PANEL_W as u32, layout.height as u32).expect("画布");
    render(&mut pm, panel, &layout);
    if std::env::var_os("AIRSNIP_PANEL_DUMP").is_some() {
        let path = std::env::temp_dir().join(format!(
            "airsnip-panel-{:?}-{:?}.png",
            panel.section, panel.hotkey_group
        ));
        std::fs::write(&path, pm.encode_png().expect("png")).ok();
        eprintln!("已输出 {path:?}");
    }
    (pm, layout)
}

/// 与面板底色的「距离」（用来判墨水）。
fn ink_of(pm: &tiny_skia::Pixmap, rect: crate::overlay::geometry::SelRect, bg: [u8; 4]) -> usize {
    let mut count = 0;
    for y in rect.y.max(0)..(rect.y + rect.h).max(0) {
        for x in rect.x.max(0)..(rect.x + rect.w).max(0) {
            let (Ok(xu), Ok(yu)) = (u32::try_from(x), u32::try_from(y)) else {
                continue;
            };
            if xu >= pm.width() || yu >= pm.height() {
                continue;
            }
            let px = pm.pixel(xu, yu).expect("像素");
            let d = (px.red() as i32 - bg[0] as i32).abs()
                + (px.green() as i32 - bg[1] as i32).abs()
                + (px.blue() as i32 - bg[2] as i32).abs();
            if d > 30 {
                count += 1;
            }
        }
    }
    count
}

/// 回归：页签文字必须看得见——分段控件的容器底不能画在 `Tab` 之后。
/// 判据：页签内缩区域的**颜色种类数** ≥ 3（背景 + 胶囊 + 文字 + 抗锯齿）；
/// 被不透明底盖住时里面只剩一种颜色。
#[test]
fn hotkey_tabs_text_is_visible() {
    for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
        for group in KeyGroup::ALL {
            let panel = panel_for_test(Section::Hotkeys, theme, group);
            let (pm, layout) = paint(&panel);
            let mut checked = 0;
            for item in &layout.items {
                let Item::Tab(tab) = item else { continue };
                let inset = (crate::settings::panel::controls::TAB_INSET as f32 * panel.scale) as i32;
                let mut colors = std::collections::BTreeSet::new();
                for y in (tab.rect.y + inset)..(tab.rect.y + tab.rect.h - inset) {
                    for x in (tab.rect.x + inset)..(tab.rect.x + tab.rect.w - inset) {
                        let (Ok(xu), Ok(yu)) = (u32::try_from(x), u32::try_from(y)) else {
                            continue;
                        };
                        let px = pm.pixel(xu, yu).expect("像素");
                        colors.insert((px.red(), px.green(), px.blue()));
                    }
                }
                assert!(
                    colors.len() >= 3,
                    "{theme:?} 下 {:?} 页签看不到文字（区域内只有 {} 种颜色）",
                    tab.group,
                    colors.len()
                );
                checked += 1;
            }
            assert_eq!(checked, KeyGroup::ALL.len(), "应有三个页签");
        }
    }
}

/// 回归：快捷键行「标签左、键帽右」两边都必须有墨迹（标签没画 / 键帽文字没画都要红）。
#[test]
fn keycap_cells_paint_label_and_caps() {
    let panel = panel_for_test(Section::Hotkeys, crate::theme::Theme::Dark, KeyGroup::Output);
    let (pm, layout) = paint(&panel);
    let bg = panel.palette.bg;
    let mut n = 0;
    for item in &layout.items {
        let Item::Row(r) = item else { continue };
        if r.kind != RowKind::KeyCap {
            continue;
        }
        n += 1;
        let label_rect = crate::overlay::geometry::SelRect {
            x: r.rect.x + r.label_pad,
            y: r.rect.y + r.label_pad / 2,
            w: r.control.x - (r.rect.x + r.label_pad),
            h: r.rect.h - r.label_pad,
        };
        assert!(
            ink_of(&pm, label_rect, bg) > 10,
            "{:?} 的标签没画出来（{label_rect:?}）",
            r.id
        );
        assert!(
            ink_of(&pm, r.control, bg) > 10,
            "{:?} 的键帽文字没画出来（{:?}）",
            r.id,
            r.control
        );
    }
    assert!(n > 0, "输出组应有键帽行");
}

/// 回归：下拉展开后列表真的画出来了，且盖在行之上（ADR 0041：
/// 「看着像下拉就得是下拉」，不能再是点击循环切换）。
#[test]
fn save_page_paints_open_dropdown() {
    use crate::settings::panel::controls::RowId;
    for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
        let mut panel = panel_for_test(Section::Save, theme, KeyGroup::Capture);
        panel.open_select = Some(RowId::QuickFormat);
        let (pm, layout) = paint(&panel);
        let list =
            crate::settings::panel::savepage::list_rect(&panel, &layout, RowId::QuickFormat).expect("应有列表矩形");
        // 列表里应有文字与选中项底色（不只是一种颜色）
        let bg = panel.palette.bg;
        assert!(ink_of(&pm, list, bg) > 100, "{theme:?} 下拉列表看不出内容：{list:?}");
        let mut colors = std::collections::BTreeSet::new();
        for y in list.y..(list.y + list.h) {
            for x in list.x..(list.x + list.w) {
                let (Ok(xu), Ok(yu)) = (u32::try_from(x), u32::try_from(y)) else {
                    continue;
                };
                let px = pm.pixel(xu, yu).expect("像素");
                colors.insert((px.red(), px.green(), px.blue()));
            }
        }
        assert!(
            colors.len() >= 4,
            "{theme:?} 列表应含底色/边框/文字/选中底：{} 种",
            colors.len()
        );
    }
}

/// 长截图日志那一行（SYS-9 / ADR 0046）渲染过一遍：标签与按钮都得有墨。
/// 「新加一行但忘了画」是 `RowKind` 分支漏掉的典型症状（整行空白）。
#[test]
fn general_page_paints_log_folder_row() {
    use crate::overlay::geometry::SelRect;
    use crate::settings::panel::controls::RowId;
    for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
        let panel = panel_for_test(Section::General, theme, KeyGroup::Capture);
        let (pm, layout) = paint(&panel);
        let bg = panel.palette.bg;
        let row = layout
            .items
            .iter()
            .find_map(|it| match it {
                Item::Row(r) if r.id == RowId::OpenLogFolder => Some(r),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{theme:?} 通用页应有「打开日志文件夹」行"));
        let label_rect = SelRect {
            x: row.rect.x + row.label_pad,
            y: row.rect.y,
            w: row.control.x - (row.rect.x + row.label_pad),
            h: row.rect.h,
        };
        assert!(
            ink_of(&pm, label_rect, bg) > 10,
            "{theme:?} 日志行标签没画出来：{label_rect:?}"
        );
        let btn = crate::settings::panel::link::button_rect(&panel, row);
        assert!(
            ink_of(&pm, btn, bg) > 10,
            "{theme:?}「打开日志文件夹」按钮没画出来：{btn:?}"
        );
    }
}

/// 关于页的「官网」行（唯一官方下载渠道）渲染过一遍：左侧标签与右侧域名链接都得有墨。
/// 回归背景：关于页是第二高的页（520 里用了 516），新加一行若算错高度会让它落到
/// 固定窗口底缘外——几何测试看得见、真界面被裁掉。
#[test]
fn about_page_paints_homepage_row() {
    use crate::overlay::geometry::SelRect;
    use crate::settings::panel::controls::RowId;
    for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
        let panel = panel_for_test(Section::About, theme, KeyGroup::Capture);
        let (pm, layout) = paint(&panel);
        let bg = panel.palette.bg;
        let row = layout
            .items
            .iter()
            .find_map(|it| match it {
                Item::Row(r) if r.id == RowId::Homepage => Some(r),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{theme:?} 关于页应有「官网」行"));
        let label_rect = SelRect {
            x: row.rect.x + row.label_pad,
            y: row.rect.y,
            w: row.control.x - (row.rect.x + row.label_pad),
            h: row.rect.h,
        };
        assert!(
            ink_of(&pm, label_rect, bg) > 10,
            "{theme:?} 「官网」标签没画出来：{label_rect:?}"
        );
        assert!(
            ink_of(&pm, row.control, bg) > 10,
            "{theme:?} 官网域名链接没画出来：{:?}",
            row.control
        );
    }
}

/// 保存页渲染过一遍（OUT-2 / ADR 0041）：**文件名输入框**里有模板文本、
/// **保存位置**行有目录文本、**预览**行有完整路径——三者都必须真有墨迹。
#[test]
fn save_page_paints_dir_name_and_preview() {
    use crate::settings::panel::controls::RowId;
    for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
        let panel = panel_for_test(Section::Save, theme, KeyGroup::Capture);
        let (pm, layout) = paint(&panel);
        let bg = panel.palette.bg;
        let control_of = |want: RowId| {
            layout
                .items
                .iter()
                .find_map(|it| match it {
                    Item::Row(r) if r.id == want => Some(r.control),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{want:?} 行不存在"))
        };
        let name_box = control_of(RowId::QuickName);
        assert!(
            ink_of(&pm, name_box, bg) > 50,
            "{theme:?} 文件名框里看不到模板文本：{name_box:?}"
        );
        let dir_box = control_of(RowId::QuickDir);
        assert!(
            ink_of(&pm, dir_box, bg) > 20,
            "{theme:?} 保存位置行看不到目录：{dir_box:?}"
        );
        let preview_box = control_of(RowId::QuickPreview);
        assert!(
            ink_of(&pm, preview_box, bg) > 20,
            "{theme:?} 预览行看不到路径：{preview_box:?}"
        );
    }
}

/// 通用页的「语言」行（I18N-1 / ADR 0047）必须真的画出下拉胶囊与当前语言名。
/// 回归背景：这一行原来是最右侧的只读文字，改成 `RowKind::Select` 后，
/// `savepage::draw` 的 match 漏了它 —— 整行右侧空白，几何测试却全绿。
#[test]
fn general_page_paints_language_select() {
    use crate::settings::panel::controls::RowId;
    for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
        let panel = panel_for_test(Section::General, theme, KeyGroup::Capture);
        let (pm, layout) = paint(&panel);
        let bg = panel.palette.bg;
        let row = layout
            .items
            .iter()
            .find_map(|it| match it {
                Item::Row(r) if r.id == RowId::Lang => Some(r),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{theme:?} 通用页应有「语言」行"));
        assert_eq!(row.kind, RowKind::Select, "语言行应走下拉");
        assert!(
            ink_of(&pm, row.control, bg) > 30,
            "{theme:?} 语言行右侧空白：控件区没画出下拉（{:?}）",
            row.control
        );
    }
}
