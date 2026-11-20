// SPDX-License-Identifier: GPL-3.0-only
//! 反馈文本排版（SYS-8）：输出**一段紧凑文本**，用于反馈 issue 的「日志与补充信息」字段。
//!
//! issue 模板是 GitHub issue form——版本是独立输入框、「运行环境」是复选框组，
//! 粘贴一段带 `### 小节` 的 Markdown 既填不了输入框、也勾不上复选框（还容易被
//! 当成「模板内容」）。所以这里只生成一段纯文本：版本放首行，用户顺手抄一下就到
//! 「版本」字段；其余信息他自己看着勾复选框，不必被我们反过来教。

use crate::settings::{Config, DoubleClickAction, PinPosition};

use super::sys::{Monitor, SystemInfo};

/// 生成反馈文本（三行：系统与环境 / 显示器 / 开关）。
pub fn render(info: &SystemInfo, cfg: &Config, ui_scale: f32) -> String {
    format!(
        "{}\n{}\n{}\n",
        headline(info),
        monitors_line(&info.monitors),
        toggles_line(info.autostart, cfg, ui_scale)
    )
}

/// 首行：版本 + Windows 版本 + 显卡。版本放最前，方便顺手抄到「版本」字段。
fn headline(info: &SystemInfo) -> String {
    let mut parts = vec![format!("AirSnip v{}", env!("CARGO_PKG_VERSION"))];
    parts.push(info.os.clone());
    if !info.gpus.is_empty() {
        parts.push(format!("显卡 {}", info.gpus.join(" / ")));
    }
    parts.join(" · ")
}

/// 显示器行：块数 + 每块的分辨率 / 缩放 / 方位 / HDR 状态。
/// 单屏不写「主屏」（只有一块屏时称呼没有意义）。
fn monitors_line(monitors: &[Monitor]) -> String {
    if let [only] = monitors {
        return format!("显示器：1 块，{}", describe(only, ""));
    }
    let shown: Vec<String> = labels(monitors)
        .into_iter()
        .map(|(idx, label)| describe(&monitors[idx], &label))
        .collect();
    format!("显示器：{} 块，{}", monitors.len(), shown.join(" · "))
}

/// 开关行：会影响截图 / 贴图行为、排查时又必须知道的那几项。
fn toggles_line(autostart: bool, cfg: &Config, ui_scale: f32) -> String {
    let items = [
        format!("窗口吸附 {}", on_off(cfg.window_snap)),
        format!("控件级捕捉 {}", on_off(cfg.control_snap)),
        format!(
            "贴图位置 {}",
            match cfg.pin_position {
                PinPosition::Original => "截图原位置",
                PinPosition::Center => "屏幕中间",
            }
        ),
        format!(
            "双击选区空白 {}",
            match cfg.double_click_action {
                DoubleClickAction::Copy => "复制截图",
                DoubleClickAction::Pin => "贴到屏幕",
            }
        ),
        format!(
            "界面缩放 {}（{ui_scale:.2}×）",
            cfg.ui_scale.title().trim_start_matches("界面缩放·")
        ),
        format!("开机自启 {}", on_off(autostart)),
        // 长截图日志（SYS-9 / ADR 0046）：日志常开，这里告诉对方去哪拿。
        // 写 `%TEMP%` 而不是展开后的真实路径——后者含用户名，而这份文本刻意不带；
        // 目录名从 `debug` 取，避免两处字面量迟早漂。
        format!("长截图日志 %TEMP%\\{}", crate::longshot::debug::DIR_NAME),
    ];
    format!("开关：{}", items.join(" · "))
}

/// 每块屏的显示名：主屏、副屏 1、副屏 2…；副屏带相对主屏的方位。
/// 返回 (下标, 标签)，下标与传入的 `monitors` 一一对应。
fn labels(monitors: &[Monitor]) -> Vec<(usize, String)> {
    let Some(pi) = monitors.iter().position(|m| m.primary) else {
        return Vec::new();
    };
    let mut n = 0usize;
    monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let label = if i == pi {
                "主屏".to_string()
            } else {
                n += 1;
                format!("副屏{n}（{}）", position(m, &monitors[pi]))
            };
            (i, label)
        })
        .collect()
}

/// 相对主屏的方位；与主屏有重叠时写「重叠」。
fn position(m: &Monitor, primary: &Monitor) -> &'static str {
    if m.x + m.width <= primary.x {
        "左"
    } else if m.x >= primary.x + primary.width {
        "右"
    } else if m.y + m.height <= primary.y {
        "上"
    } else if m.y >= primary.y + primary.height {
        "下"
    } else {
        "重叠"
    }
}

/// `主屏 2560×1440 @150%（HDR 关）`：标签（可为空）+ 物理分辨率 + 缩放 + HDR 状态。
fn describe(m: &Monitor, label: &str) -> String {
    let prefix = if label.is_empty() {
        String::new()
    } else {
        format!("{label} ")
    };
    format!(
        "{prefix}{}×{} @{}%（HDR {}）",
        m.width,
        m.height,
        m.scale_percent(),
        on_off(m.hdr)
    )
}

fn on_off(b: bool) -> &'static str {
    if b { "开" } else { "关" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Config, DoubleClickAction, PinPosition, UiScale};

    fn monitor(primary: bool, x: i32, y: i32, w: i32, h: i32, dpi: u32, hdr: bool) -> Monitor {
        Monitor {
            name: "\\\\.\\DISPLAY1".into(),
            x,
            y,
            width: w,
            height: h,
            dpi,
            primary,
            hdr,
        }
    }

    fn info(monitors: Vec<Monitor>, autostart: bool) -> SystemInfo {
        SystemInfo {
            os: "Windows 11 专业版 24H2 26100.1742".into(),
            monitors,
            gpus: vec!["NVIDIA GeForce RTX 3060".into()],
            autostart,
        }
    }

    /// 固定三行结构：粘贴到「日志与补充信息」后不会散成好几段。
    #[test]
    fn render_is_three_lines() {
        let info = info(vec![monitor(true, 0, 0, 1920, 1080, 96, false)], false);
        let text = render(&info, &Config::default(), 1.0);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(
            lines[0].starts_with(&format!("AirSnip v{}", env!("CARGO_PKG_VERSION"))),
            "{text}"
        );
        assert!(lines[1].starts_with("显示器："), "{text}");
        assert!(lines[2].starts_with("开关："), "{text}");
        // 不带 issue form 的小节标题（模板字段是分开的，粘不了）
        assert!(!text.contains("###"), "{text}");
    }

    /// 单屏：不写「主屏」、不写多显示器，HDR 状态明确写「关」。
    #[test]
    fn single_monitor_is_plain() {
        let info = info(vec![monitor(true, 0, 0, 1920, 1080, 96, false)], false);
        let text = render(&info, &Config::default(), 1.0);
        assert!(text.contains("显示器：1 块，1920×1080 @100%（HDR 关）"), "{text}");
        assert!(!text.contains("主屏"), "{text}");
        assert!(!text.contains("多显示器"), "{text}");
        assert!(text.contains("显卡 NVIDIA GeForce RTX 3060"), "{text}");
    }

    /// 双屏（副屏在左、HDR 开、缩放不同）：方位与每屏缩放都要写清楚。
    #[test]
    fn dual_monitor_reports_layout_and_hdr() {
        let info = info(
            vec![
                monitor(true, 0, 0, 2560, 1440, 144, false),
                monitor(false, -1920, 0, 1920, 1080, 120, true),
            ],
            true,
        );
        let text = render(&info, &Config::default(), 1.0);
        assert!(
            text.contains("显示器：2 块，主屏 2560×1440 @150%（HDR 关） · 副屏1（左） 1920×1080 @125%（HDR 开）"),
            "{text}"
        );
    }

    /// 副屏在主屏上方 / 主屏不在左上的负坐标排布，方位仍按相对主屏判断。
    #[test]
    fn position_is_relative_to_primary() {
        let primary = monitor(true, 1920, 0, 1920, 1080, 96, false);
        let above = monitor(false, 1920, -1080, 1920, 1080, 96, false);
        assert_eq!(position(&above, &primary), "上");
        let left = monitor(false, 0, 0, 1920, 1080, 96, false);
        assert_eq!(position(&left, &primary), "左");
        // 与主屏完全重合（镜像）算重叠，不硬报某个方向
        let mirror = monitor(false, 1920, 0, 1920, 1080, 96, false);
        assert_eq!(position(&mirror, &primary), "重叠");
    }

    /// 开关行必须反映当前设置（用户改完开关再复制，内容要跟着变）。
    #[test]
    fn toggles_follow_config() {
        let mut cfg = Config::default();
        let info = info(vec![monitor(true, 0, 0, 1920, 1080, 96, false)], false);
        let text = render(&info, &cfg, 1.0);
        assert!(text.contains("窗口吸附 开"), "{text}");
        assert!(text.contains("贴图位置 截图原位置"), "{text}");
        assert!(text.contains("双击选区空白 复制截图"), "{text}");
        assert!(text.contains("界面缩放 自动（1.00×）"), "{text}");
        assert!(text.contains("开机自启 关"), "{text}");
        assert!(text.contains("长截图日志 %TEMP%\\airsnip"), "{text}");

        cfg.window_snap = false;
        cfg.control_snap = false;
        cfg.pin_position = PinPosition::Center;
        cfg.double_click_action = DoubleClickAction::Pin;
        cfg.ui_scale = UiScale::P150;
        let text = render(&info, &cfg, 1.5);
        assert!(text.contains("窗口吸附 关"), "{text}");
        assert!(text.contains("控件级捕捉 关"), "{text}");
        assert!(text.contains("贴图位置 屏幕中间"), "{text}");
        assert!(text.contains("双击选区空白 贴到屏幕"), "{text}");
        assert!(text.contains("界面缩放 150%（1.50×）"), "{text}");
    }
}
