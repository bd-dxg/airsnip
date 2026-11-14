// SPDX-License-Identifier: GPL-3.0-only
//! 控件悬浮提示（EDT-10）：鼠标悬停工具栏按钮或圆角控制柄约 400ms 后，
//! 在其外侧弹出小气泡显示「名称 · 快捷键」；悬停立即给按钮底色，按下鼠标即隐藏。
//! 色块显示「#RRGGBB · 双击改色」（第一段色值、第二段操作方式，issue #122 / #123）。
//! 决策与取舍（不加开关、一次性定时器）见 `docs/adr/0020-control-tooltips.md`。
//! 气泡几何与绘制见 `paint` 子模块。

use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::editor::Tool;
use crate::overlay::Overlay;
use crate::overlay::geometry::{SelRect, expand_rect, union_rect};
use crate::overlay::objedit;
use crate::overlay::pick::pick_radius;
use crate::settings::{UNBOUND, hotkey::vk_name};

use super::Action;

mod paint;
pub(crate) use paint::{bubble_rect, draw};

/// 提示延迟（毫秒）。一次性定时器，只在悬停目标变化时重设（ADR 0020）。
pub(crate) const DELAY_MS: u32 = 400;
/// 定时器 ID（覆盖层窗口内唯一，与长截图执行窗口的定时器互不相干）。
pub(crate) const TIMER_ID: usize = 2;

/// 悬停命中（决定按钮底色与气泡目标）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Hit {
    /// 工具栏按钮。
    Button(Action),
    /// 圆角控制柄（拖拽型手柄，不加底色）。
    Radius,
}

/// 已弹出的气泡：矩形 + 文字。
pub(crate) struct Bubble {
    pub(crate) rect: SelRect,
    text: String,
}

/// 悬浮提示状态（默认无）。
#[derive(Default)]
pub(crate) struct Tip {
    /// 当前悬停目标。
    hover: Option<Hit>,
    /// 已弹出的气泡（延迟到时才有；悬停变化或按下鼠标即清空）。
    pub(crate) bubble: Option<Bubble>,
}

impl Tip {
    /// 悬停的按钮（供工具栏绘制底色）；手柄不参与底色。
    pub(crate) fn hover_button(&self) -> Option<Action> {
        match self.hover {
            Some(Hit::Button(a)) => Some(a),
            _ => None,
        }
    }
}

/// 按钮名称；返回 None 表示该控件不提示（第二行字体控件，ADR 0020）。
/// 色块例外：提示由 `hintable` 单独开启，文案见 `color_tip_text`（issue #122 / #123）。
fn action_label(action: Action) -> Option<&'static str> {
    match action {
        Action::Tool(Tool::Rect) => Some("矩形工具"),
        Action::Tool(Tool::Arrow) => Some("箭头工具"),
        Action::Tool(Tool::Text) => Some("文本工具"),
        Action::Tool(Tool::Highlight) => Some("荧光笔"),
        Action::Tool(Tool::Blur) => Some("高斯模糊"),
        Action::Tool(Tool::Glow) => Some("聚光灯"),
        Action::Tool(Tool::Number) => Some("序号标记"),
        Action::Tool(Tool::Line) => Some("直线工具"),
        Action::Tool(Tool::Ellipse) => Some("椭圆工具"),
        Action::Tool(Tool::Freehand) => Some("手绘"),
        // ADR 0036：移动态（指针）——拖选区内空白平移选区，拖对象移动该对象
        Action::Move => Some("移动选区"),
        Action::ToggleFill => Some("填充"),
        Action::Undo => Some("撤销"),
        Action::Redo => Some("重做"),
        Action::Pin => Some("贴图"),
        Action::Longshot => Some("长截图"),
        Action::Copy => Some("复制"),
        Action::Save => Some("保存"),
        _ => None,
    }
}

/// 控件是否显示悬停提示：`None` 表示不提示（第二行字体控件，ADR 0020）。
/// 色块也提示——双击改色不直观，显示色值 + 操作方式（issue #122 / #123）。
fn hintable(action: Action) -> bool {
    matches!(action, Action::Color(_)) || action_label(action).is_some()
}

/// 色块气泡文案：第一段为色块当前颜色的十六进制色值，第二段提示双击改色。
/// 色值取 `colors[i]`（与色块外观一致），改色后重新悬停即显示新值。
fn color_tip_text(c: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X} · 双击改色", c[0], c[1], c[2])
}

/// 气泡文案：「名称 · 快捷键」；无快捷键时只有名称。
fn compose(label: &str, key: Option<&str>) -> String {
    match key {
        Some(k) => format!("{label} · {k}"),
        None => label.to_string(),
    }
}

impl Overlay {
    /// 鼠标移动（EDT-10）：更新悬停目标；变化时重设一次性定时器并撤掉旧气泡。
    pub(in crate::overlay) fn on_tip_hover(&mut self, x: i32, y: i32) {
        // 拖拽 / 绘制中不提示：按下鼠标已隐藏一次，这里兜住按住之后的移动
        if self.dragging() {
            self.hide_tip();
            return;
        }
        let hit = self.tip_target(x, y);
        if hit == self.tip.hover {
            return; // 同一目标内移动不重置计时，避免定时器抖动（ADR 0020）
        }
        let old_hover = self.hover_rect(self.tip.hover);
        self.tip.hover = hit;
        let old_bubble = self.tip.bubble.take().map(|b| b.rect);
        unsafe {
            if hit.is_some() {
                // 同 ID 再次 SetTimer 会替换旧定时器（MSDN），无需先 Kill
                let _ = SetTimer(Some(self.hwnd), TIMER_ID, DELAY_MS, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_ID);
            }
        }
        self.repaint_tip(old_bubble, old_hover);
    }

    /// 悬停延迟到时（EDT-10）：一次性定时器先 Kill，再按当前悬停目标弹气泡。
    pub(in crate::overlay) fn on_tip_timer(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
        }
        let Some(hit) = self.tip.hover else {
            return;
        };
        let Some(text) = self.tip_text(hit) else {
            return;
        };
        let Some((anchor, vertical)) = self.tip_anchor(hit) else {
            return;
        };
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let rect = bubble_rect(anchor, vertical, &text, vw, vh, self.ui);
        self.tip.bubble = Some(Bubble { rect, text });
        self.repaint(expand_rect(rect, 2));
    }

    /// 气泡的锚点（水平对齐）与垂直参照（贴它的外缘）。
    fn tip_anchor(&self, hit: Hit) -> Option<(SelRect, SelRect)> {
        match hit {
            // 按钮：水平对齐按钮，垂直贴整条工具栏外缘（避开第二行）
            Hit::Button(action) => {
                let bar = self.bar.as_ref()?;
                let r = bar.hits.iter().find(|(_, a)| *a == action).map(|(r, _)| *r)?;
                Some((r, bar.rect))
            }
            // 圆角控制柄：没有工具栏，两个方向都贴手柄自身
            Hit::Radius => {
                let obj = self.selected_object()?;
                let (hx, hy) = objedit::radius_handle_point(obj, self.ui)?;
                let pick = pick_radius(self.ui);
                let r = SelRect {
                    x: hx as i32 - pick,
                    y: hy as i32 - pick,
                    w: pick * 2,
                    h: pick * 2,
                };
                Some((r, r))
            }
        }
    }

    /// 隐藏提示（鼠标按下 / 取消，EDT-10）。
    pub(in crate::overlay) fn hide_tip(&mut self) {
        let old_hover = self.hover_rect(self.tip.hover);
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
        }
        let old_bubble = self.tip.bubble.take().map(|b| b.rect);
        self.tip.hover = None;
        if old_hover.is_none() && old_bubble.is_none() {
            return;
        }
        self.repaint_tip(old_bubble, old_hover);
    }

    /// 工具栏重建（选区变化 / 第二行显隐）：作废悬停目标与气泡，返回旧气泡矩形。
    /// 不自行重绘：调用方本就要重画工具栏与选区装饰，只需夹住旧气泡区域。
    pub(in crate::overlay) fn reset_tip(&mut self) -> Option<SelRect> {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
        }
        self.tip.hover = None;
        self.tip.bubble.take().map(|b| b.rect)
    }

    /// 气泡文案：「名称 · 快捷键」；不提示的控件返回 None。色块显示色值 + 双击改色。
    fn tip_text(&self, hit: Hit) -> Option<String> {
        match hit {
            // 色块：显示该色块的十六进制色值（第一段）与操作方式（第二段），issue #122 / #123
            Hit::Button(Action::Color(i)) => Some(color_tip_text(self.colors[i])),
            Hit::Button(a) => Some(compose(action_label(a)?, self.key_suffix(a).as_deref())),
            // 圆角控制柄（EDT-1/ADR 0010）没有快捷键
            Hit::Radius => Some("圆角 · 拖动调节".into()),
        }
    }

    /// 按钮的快捷键后缀；无快捷键或已被清空（`UNBOUND`）时返回 None。
    fn key_suffix(&self, action: Action) -> Option<String> {
        let vk = match action {
            Action::Move => self.tool_keys.move_key,
            Action::Tool(Tool::Rect) => self.tool_keys.rect,
            Action::Tool(Tool::Arrow) => self.tool_keys.arrow,
            Action::Tool(Tool::Text) => self.tool_keys.text,
            Action::Tool(Tool::Blur) => self.tool_keys.blur,
            Action::Tool(Tool::Highlight) => self.tool_keys.highlight,
            Action::Tool(Tool::Glow) => self.tool_keys.glow,
            Action::Tool(Tool::Number) => self.tool_keys.number,
            Action::Tool(Tool::Line) => self.tool_keys.line,
            Action::Tool(Tool::Ellipse) => self.tool_keys.ellipse,
            Action::Tool(Tool::Freehand) => self.tool_keys.freehand,
            Action::Longshot => self.longshot_key,
            Action::Pin => self.pin_key,
            // 硬编码快捷键：与 `wndproc` 的按键分支保持一致
            Action::Undo => return Some("Ctrl+Z".into()),
            Action::Redo => return Some("Ctrl+Y".into()),
            Action::Copy => return Some("Ctrl+C".into()),
            Action::Save => return Some("Ctrl+S".into()),
            _ => return None,
        };
        (vk != UNBOUND).then(|| vk_name(vk))
    }

    /// 鼠标位置的提示目标：工具栏按钮优先，其次圆角控制柄。
    fn tip_target(&self, x: i32, y: i32) -> Option<Hit> {
        if let Some(bar) = self.bar.as_ref()
            && let Some(a) = bar.hit(x, y)
        {
            // 命中工具栏即定案：第二行控件返回 None（不提示）
            return hintable(a).then(|| Hit::Button(a));
        }
        let obj = self.selected_object()?;
        let (hx, hy) = objedit::radius_handle_point(obj, self.ui)?;
        let pick = pick_radius(self.ui) as f32;
        ((hx - x as f32).abs() <= pick && (hy - y as f32).abs() <= pick).then_some(Hit::Radius)
    }

    /// 是否正在拖拽 / 绘制（此期间不提示）。
    fn dragging(&self) -> bool {
        self.drag.is_some() || self.draft.is_some() || self.obj_drag.is_some() || self.ctrl_drag.is_some()
    }

    /// 重绘提示相关区域：悬停按钮（底色变化）与旧气泡矩形。
    /// 只重绘按钮而非整条工具栏：鼠标扫过一排按钮时，每次重绘整条会平白多出几十倍像素量。
    fn repaint_tip(&mut self, old_bubble: Option<SelRect>, old_hover: Option<SelRect>) {
        let new_hover = self.hover_rect(self.tip.hover);
        let dirty = union_rect(
            union_rect(
                old_hover.map(|r| expand_rect(r, 1)),
                new_hover.map(|r| expand_rect(r, 1)),
            ),
            old_bubble.map(|r| expand_rect(r, 2)),
        );
        if let Some(r) = dirty {
            self.repaint(r);
        }
    }

    /// 悬停目标的按钮矩形（脏区用）；手柄与无命中返回 None。
    fn hover_rect(&self, hit: Option<Hit>) -> Option<SelRect> {
        let Hit::Button(action) = hit? else {
            return None;
        };
        self.bar
            .as_ref()?
            .hits
            .iter()
            .find(|(_, a)| *a == action)
            .map(|(r, _)| *r)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Toolbar;
    use super::*;
    use crate::overlay::ui::Ui;

    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    fn bar_at(sel: SelRect, vw: i32, vh: i32) -> Toolbar {
        let vis = crate::settings::ToolbarConfig::default();
        super::super::layout_ex(
            sel,
            vw,
            vh,
            &super::super::Shown {
                vis: &vis,
                text_row: false,
                show_output: true,
                show_move: true,
            },
            base(),
        )
        .expect("元素未全隐藏时应有工具栏")
    }

    fn sel(x: i32, y: i32, w: i32, h: i32) -> SelRect {
        SelRect { x, y, w, h }
    }

    /// 第一行按钮与色块都有提示；第二行字体控件明确不提示（ADR 0020）。
    #[test]
    fn labels_cover_first_row_only() {
        let first_row = Tool::ALL.map(Action::Tool).into_iter().chain([
            Action::ToggleFill,
            Action::Undo,
            Action::Redo,
            Action::Pin,
            Action::Longshot,
            Action::Copy,
            Action::Save,
            Action::Color(0),
        ]);
        for a in first_row {
            assert!(hintable(a), "{a:?} 应有提示");
        }
        for a in [
            Action::FontPick,
            Action::ToggleBold,
            Action::ToggleItalic,
            Action::FontSmaller,
            Action::FontLarger,
            Action::FontSize,
        ] {
            assert!(action_label(a).is_none(), "{a:?} 不应提示");
        }
    }

    #[test]
    fn compose_appends_hotkey() {
        assert_eq!(compose("矩形工具", Some("1")), "矩形工具 · 1");
        assert_eq!(compose("填充", None), "填充");
    }

    /// 色块气泡：第一段为十六进制色值（与色块颜色一致），第二段为「双击改色」；
    /// 改色后输入不同颜色，文案随之更新（issue #122 / #123）。
    #[test]
    fn color_tip_shows_hex_then_doubleclick_hint() {
        assert_eq!(color_tip_text([255, 0, 0, 255]), "#FF0000 · 双击改色");
        assert_eq!(color_tip_text([30, 136, 229, 255]), "#1E88E5 · 双击改色");
        assert_eq!(color_tip_text([0, 0, 0, 255]), "#000000 · 双击改色");
    }

    /// 常规位置：气泡贴在工具栏下方、屏幕内，且不与工具栏重叠。
    #[test]
    fn bubble_sits_below_toolbar() {
        let (vw, vh) = (2560, 1440);
        let bar = bar_at(sel(600, 500, 800, 400), vw, vh);
        let r = bubble_rect(bar.hits[0].0, bar.rect, "矩形工具 · 1", vw, vh, base());
        assert!(
            r.y >= bar.rect.y + bar.rect.h,
            "气泡应在工具栏下方：{r:?} / {:?}",
            bar.rect
        );
        assert!(r.y + r.h <= vh, "气泡应在屏幕内：{r:?}");
        assert!(r.x >= 0 && r.x + r.w <= vw, "气泡应在屏幕内：{r:?}");
    }

    /// 水平方向跟随悬停的按钮：左边按钮的气泡靠左、右边靠右，未被屏边钳制时中心对齐按钮。
    #[test]
    fn bubble_follows_hovered_button() {
        let (vw, vh) = (2560, 1440);
        let bar = bar_at(sel(500, 500, 900, 400), vw, vh);
        let (left, right) = (bar.hits[0].0, bar.hits[bar.hits.len() - 1].0);
        let a = bubble_rect(left, bar.rect, "矩形工具 · 1", vw, vh, base());
        let b = bubble_rect(right, bar.rect, "保存 · Ctrl+S", vw, vh, base());
        assert!(b.x > a.x, "右侧按钮的气泡应更靠右：{a:?} / {b:?}");
        let center = |r: SelRect| r.x + r.w / 2;
        assert!((center(a) - center(left)).abs() <= 1, "气泡 {a:?} 未对齐按钮 {left:?}");
        assert!(
            (center(b) - center(right)).abs() <= 1,
            "气泡 {b:?} 未对齐按钮 {right:?}"
        );
    }

    /// 工具栏贴底时气泡翻到上方；贴顶 + 贴底（两侧都塞不下）也不能越出屏幕。
    #[test]
    fn bubble_flips_above_and_stays_inside() {
        let (vw, vh) = (1366, 768);
        // 工具栏自身贴在屏幕底部（下方只剩不到气泡高）→ 气泡应翻到它的上方
        let bottom = bar_at(sel(100, 400, 292, 300), vw, vh);
        let r = bubble_rect(bottom.hits[0].0, bottom.rect, "高斯模糊 · 4", vw, vh, base());
        assert!(
            r.y + r.h <= bottom.rect.y,
            "下方无空间时应翻到上方：{r:?} / {:?}",
            bottom.rect
        );
        assert!(r.y >= 0);

        for s in [sel(4, 4, 400, 300), sel(4, 4, 900, 760), sel(900, 2, 400, 760)] {
            let bar = bar_at(s, vw, vh);
            for ui in [base(), Ui::from_dpi(192)] {
                let r = bubble_rect(bar.hits[0].0, bar.rect, "长截图 · S", vw, vh, ui);
                assert!(r.x >= 0 && r.x + r.w <= vw, "气泡越出屏幕：{r:?}（选区 {s:?}）");
                assert!(r.y >= 0 && r.y + r.h <= vh, "气泡越出屏幕：{r:?}（选区 {s:?}）");
            }
        }
    }

    /// 高 DPI 下气泡同步放大（CAP-5）。
    #[test]
    fn bubble_scales_with_dpi() {
        let (vw, vh) = (3840, 2160);
        let s = sel(1000, 700, 900, 500);
        let base_r = bubble_rect(
            bar_at(s, 2560, 1440).hits[0].0,
            bar_at(s, 2560, 1440).rect,
            "保存 · Ctrl+S",
            2560,
            1440,
            base(),
        );
        let hidpi_bar = bar_at(s, vw, vh);
        let hidpi = bubble_rect(
            hidpi_bar.hits[0].0,
            hidpi_bar.rect,
            "保存 · Ctrl+S",
            vw,
            vh,
            Ui::from_dpi(192),
        );
        assert!(hidpi.h > base_r.h, "200% 下气泡应更高：{hidpi:?} / {base_r:?}");
        assert!(hidpi.w > base_r.w, "200% 下气泡应更宽");
    }
}
