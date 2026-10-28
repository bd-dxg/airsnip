// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层工具栏（EDT-7）：布局与命中测试；绘制见 `draw` 子模块。

mod draw;
pub(crate) mod fontlist;
mod label;

use std::sync::Arc;

use tiny_skia::Pixmap;

use crate::editor::{PALETTE, Tool};
use crate::overlay::geometry::{INFO_OFFSET, INFO_TEXT_H, SelRect};
use crate::theme::Palette;

/// 第一行高度（工具 / 颜色 / 开关 / 输出按钮）。
pub const H: i32 = 34;
/// 文本第二行高度（EDT-4：字体族 / 粗体 / 斜体 / 字号）。
pub const H2: i32 = 30;
/// 两行之间的间距。
pub const ROW_GAP: i32 = 3;
/// 字体族按钮宽度（超出时截断加省略号）。
pub const FONT_BTN_W: i32 = 132;
/// 字号显示格宽度。
pub const SIZE_W: i32 = 34;
/// 方形按钮边长（工具、撤销、开关）。
pub(crate) const BTN: i32 = 26;
/// 色块边长。
pub(crate) const SW: i32 = 22;
/// 元素间距。
pub(crate) const GAP: i32 = 4;
/// 组分隔区宽度。
pub(crate) const SEP: i32 = 9;
/// 与选区的最小间隔。
pub(crate) const OFFSET: i32 = INFO_OFFSET;
/// 内边距。
pub(crate) const PAD: i32 = 5;

/// 工具栏当前状态（决定高亮与可用性）。
#[derive(Clone, PartialEq)]
pub struct State {
    pub tool: Tool,
    pub color: usize,
    /// 矩形是否填充（EDT-1）。
    pub filled: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// 第二行（EDT-4）：当前字体族、粗体/斜体、字号与字体面板是否展开。
    pub font_family: Arc<str>,
    pub bold: bool,
    pub italic: bool,
    pub font_size: f32,
    pub font_open: bool,
}

/// 工具栏动作。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Tool(Tool),
    Color(usize),
    ToggleFill,
    Undo,
    Redo,
    /// PIN-1：把选区（含标注）贴为置顶窗口。
    Pin,
    /// M4：把当前选区作为滚动视口，进入长截图拼接（长截图键同效，默认 S）。
    Longshot,
    /// OUT-1：复制选区到剪贴板（Enter / Ctrl+C 同效）。
    Copy,
    /// OUT-2：按配置的目录与格式保存（Ctrl+S 同效）。
    Save,
    /// EDT-4：展开/收起字体列表面板。
    FontPick,
    /// EDT-4：粗体开关。
    ToggleBold,
    /// EDT-4：斜体开关。
    ToggleItalic,
    /// EDT-4：字号减小 / 增大（每步 2px）。
    FontSmaller,
    FontLarger,
    /// EDT-4：字号数值格（仅展示，不响应点击）。
    FontSize,
}

/// 工具栏布局：整体矩形（含第二行）+ 各元素命中区。
#[derive(Clone, Debug)]
pub struct Toolbar {
    /// 整个工具栏（两行的并集，绘制背景与脏区共用）。
    pub rect: SelRect,
    /// 第一行矩形。
    pub first: SelRect,
    /// 第二行矩形（仅文本工具 / 选中文本对象时存在）。
    pub second: Option<SelRect>,
    /// 命中区（draw 子模块读取所有元素）。
    pub(crate) hits: Vec<(SelRect, Action)>,
}

impl Toolbar {
    /// 命中测试：返回落在哪个元素上。
    pub fn hit(&self, x: i32, y: i32) -> Option<Action> {
        self.hits
            .iter()
            .find(|(r, _)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
            .map(|(_, a)| *a)
    }
}

/// 布局游标：依次排布元素，返回元素左边界。
struct Cursor {
    x: i32,
}

impl Cursor {
    fn next(&mut self, w: i32) -> i32 {
        let x = self.x;
        self.x += w + GAP;
        x
    }

    fn sep(&mut self) {
        self.x += SEP - GAP;
    }
}

/// 计算工具栏布局：优先放在选区下方，空间不足则放上方。
/// `text_row` 为真时附上第二行（文本工具或选中文本对象，EDT-4）。
pub fn layout(sel: SelRect, screen_w: i32, screen_h: i32, text_row: bool) -> Toolbar {
    let mut c = Cursor { x: PAD };
    let mut hits: Vec<(SelRect, Action)> = Vec::new();
    let mut item = |c: &mut Cursor, w: i32, h: i32, action: Action| {
        let x = c.next(w);
        let y = (H - h) / 2;
        hits.push((SelRect { x, y, w, h }, action));
    };

    for tool in Tool::ALL {
        item(&mut c, BTN, BTN, Action::Tool(tool));
    }
    c.sep();
    for i in 0..PALETTE.len() {
        item(&mut c, SW, SW, Action::Color(i));
    }
    c.sep();
    item(&mut c, BTN, BTN, Action::ToggleFill);
    c.sep();
    item(&mut c, BTN, BTN, Action::Undo);
    item(&mut c, BTN, BTN, Action::Redo);
    c.sep();
    item(&mut c, BTN, BTN, Action::Pin);
    item(&mut c, BTN, BTN, Action::Longshot);
    c.sep();
    item(&mut c, BTN, BTN, Action::Copy);
    item(&mut c, BTN, BTN, Action::Save);
    let w1 = c.x - GAP + PAD;

    // 第二行（EDT-4）：字体族按钮 + B / I + 字号 − 数值 +
    let mut c2 = Cursor { x: PAD };
    let mut hits2: Vec<(SelRect, Action)> = Vec::new();
    let mut item2 = |c: &mut Cursor, w: i32, action: Action| {
        let x = c.next(w);
        let y = (H2 - BTN) / 2;
        hits2.push((SelRect { x, y, w, h: BTN }, action));
    };
    if text_row {
        item2(&mut c2, FONT_BTN_W, Action::FontPick);
        c2.sep();
        item2(&mut c2, BTN, Action::ToggleBold);
        item2(&mut c2, BTN, Action::ToggleItalic);
        c2.sep();
        item2(&mut c2, BTN, Action::FontSmaller);
        item2(&mut c2, SIZE_W, Action::FontSize);
        item2(&mut c2, BTN, Action::FontLarger);
    }
    let w2 = if text_row { c2.x - GAP + PAD } else { 0 };

    let w = w1.max(w2);
    let h = if text_row { H + ROW_GAP + H2 } else { H };
    let x = sel.x.clamp(4, (screen_w - w - 4).max(4));
    // 信息文字占位（见 overlay::text_rect）：与文字同侧时需再让出一个文字高度，避免重叠
    let text_above = sel.y > INFO_TEXT_H + INFO_OFFSET;
    let below = sel.y + sel.h + OFFSET + if text_above { 0 } else { INFO_TEXT_H };
    let y = if below + h + 4 <= screen_h {
        below
    } else if text_above {
        // 上方优先，但屏幕空间不足时允许贴到屏幕顶部（信息文字优先不重叠，
        // 极端小屏 + 大选区下只能重叠；此处只保证不退到负坐标）
        (sel.y - INFO_OFFSET - INFO_TEXT_H - OFFSET - h).max(2)
    } else {
        (sel.y - OFFSET - h).max(2)
    };
    let first = SelRect { x, y, w: w1, h: H };
    let second = text_row.then(|| SelRect {
        x,
        y: y + H + ROW_GAP,
        w: w2,
        h: H2,
    });
    let rect = SelRect { x, y, w, h };

    // 相对坐标 → 屏幕坐标
    let offset = |r: SelRect, dy: i32| SelRect {
        x: r.x + x,
        y: r.y + y + dy,
        w: r.w,
        h: r.h,
    };
    let hits = hits
        .into_iter()
        .map(|(r, a)| (offset(r, 0), a))
        .chain(hits2.into_iter().map(|(r, a)| (offset(r, H + ROW_GAP), a)));
    Toolbar {
        rect,
        first,
        second,
        hits: hits.collect(),
    }
}

/// 把工具栏绘制到覆盖层像素图（委托给 draw 子模块）。
pub fn draw(pixmap: &mut Pixmap, bar: &Toolbar, state: &State, palette: &Palette) {
    draw::render(pixmap, bar, state, palette);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::overlay::geometry::{intersect_rect, text_rect};

    fn sel(x: i32, y: i32, w: i32, h: i32) -> SelRect {
        SelRect { x, y, w, h }
    }

    /// 工具栏不能与信息文字重叠，否则 GDI 文字会盖在工具栏上（见 overlay::text_rect）。
    #[test]
    fn toolbar_never_overlaps_info_text() {
        let (vw, vh) = (2560, 1440);
        let cases = [
            sel(100, 100, 400, 300),   // 常规：文字在上、工具栏在下
            sel(100, 4, 400, 300),     // 贴顶：文字在下，工具栏再往下让一个文字高度
            sel(100, 100, 900, 1300),  // 贴底：工具栏翻到文字上方
            sel(100, 4, 900, 1420),    // 贴顶贴底：工具栏翻到上方、文字在下方
            sel(2400, 100, 150, 1200), // 靠右：工具栏贴右边
        ];
        for s in cases {
            let bar = layout(s, vw, vh, false).rect;
            let text = text_rect(s, vw);
            assert!(
                intersect_rect(bar, text).is_none(),
                "工具栏 {bar:?} 与信息文字 {text:?} 重叠（选区 {s:?}）"
            );
            assert!(bar.y >= 0 && bar.y + bar.h <= vh, "工具栏应在屏幕内：{bar:?}");
            assert!(bar.x >= 0 && bar.x + bar.w <= vw, "工具栏应在屏幕内：{bar:?}");
        }
    }

    /// 第二行（EDT-4）同样不能与信息文字重叠，且两行不越出屏幕。
    #[test]
    fn text_row_stays_inside_screen() {
        let (vw, vh) = (1366, 768);
        let cases = [
            sel(100, 100, 400, 300),
            sel(100, 4, 400, 300),
            sel(100, 100, 900, 600),
            sel(100, 4, 900, 700),
            sel(1000, 300, 300, 400),
        ];
        for s in cases {
            let bar = layout(s, vw, vh, true);
            let text = text_rect(s, vw);
            assert!(
                intersect_rect(bar.rect, text).is_none(),
                "工具栏 {bar:?} 与信息文字 {text:?} 重叠"
            );
            let second = bar.second.expect("文本工具应有第二行");
            assert_eq!(second.y, bar.first.y + H + ROW_GAP);
            assert!(
                bar.rect.y >= 0 && bar.rect.y + bar.rect.h <= vh,
                "工具栏应在屏幕内：{bar:?}"
            );
            assert!(
                bar.rect.x >= 0 && bar.rect.x + bar.rect.w <= vw,
                "工具栏应在屏幕内：{bar:?}"
            );
            // 第二行元素全在第二行矩形内
            for (r, _) in bar.hits.iter().filter(|(r, _)| r.y >= second.y) {
                assert!(
                    r.x >= second.x && r.x + r.w <= second.x + second.w,
                    "第二行元素 {r:?} 超出第二行 {second:?}"
                );
            }
        }
    }

    #[test]
    fn toolbar_hit_maps_each_element() {
        let bar = layout(sel(100, 100, 400, 300), 2560, 1440, true);
        // 每个元素的中心点必须命中它自己
        for (r, action) in &bar.hits {
            let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
            assert_eq!(bar.hit(cx, cy), Some(*action), "元素 {action:?} 中心未命中自己");
        }
        // 元素不重叠（命中区连续递增；第二行另起一排列，只比较同行）
        let second_y = bar.second.expect("应有第二行").y;
        for w in bar.hits.windows(2) {
            if w[0].0.y >= second_y && w[1].0.y >= second_y {
                assert!(w[0].0.x + w[0].0.w <= w[1].0.x, "元素重叠：{:?} / {:?}", w[0].0, w[1].0);
            }
        }
        // 元素顺序与数量（EDT-7 工具栏结构 + EDT-4 第二行）
        let actions: Vec<Action> = bar.hits.iter().map(|(_, a)| *a).collect();
        assert_eq!(actions.len(), 24);
        assert_eq!(actions[..6], Tool::ALL.map(Action::Tool));
        assert_eq!(actions[12], Action::Undo);
        assert_eq!(actions[13], Action::Redo);
        assert_eq!(actions[14], Action::Pin);
        assert_eq!(actions[15], Action::Longshot);
        assert_eq!(actions[16], Action::Copy);
        assert_eq!(actions[17], Action::Save);
        assert_eq!(
            actions[18..],
            [
                Action::FontPick,
                Action::ToggleBold,
                Action::ToggleItalic,
                Action::FontSmaller,
                Action::FontSize,
                Action::FontLarger
            ]
        );
    }

    /// 非文本场景第二行不应出现（避免占地与多余命中区）。
    #[test]
    fn second_row_only_when_text_row_requested() {
        let bar = layout(sel(100, 100, 400, 300), 2560, 1440, false);
        assert!(bar.second.is_none());
        assert_eq!(bar.rect.h, H);
        assert_eq!(bar.hits.len(), 18);
    }
}
