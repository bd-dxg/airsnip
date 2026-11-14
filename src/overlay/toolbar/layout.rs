// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏布局计算（EDT-7 / EDT-16）：元素按**可见性**裁剪、宽度按可见项重算，
//! 全部元素都被隐藏时返回 `None`（工具栏整体不绘制）。
//! 从 `mod` 拆出，保证单文件不超过 300 行（类型定义留在父模块）。

use super::{
    Action, BTN_BASE, FONT_BTN_W_BASE, GAP_BASE, H_BASE, H2_BASE, PAD_BASE, ROW_GAP_BASE, SEP_BASE, SIZE_W_BASE,
    SW_BASE, Toolbar,
};
use crate::overlay::geometry::{SelRect, info_offset, info_text_below, info_text_h};
use crate::overlay::ui::Ui;
use crate::settings::{ToolbarConfig, ToolbarGroup, ToolbarSlot};

/// 布局游标：依次排布元素，返回元素左边界。
struct Cursor {
    x: i32,
    gap: i32,
    sep: i32,
}

impl Cursor {
    fn next(&mut self, w: i32) -> i32 {
        let x = self.x;
        self.x += w + self.gap;
        x
    }

    fn sep(&mut self) {
        self.x += self.sep - self.gap;
    }
}

/// 布局条件（EDT-16）：元素显隐来自设置，另有两个与之**叠加**的条件 ——
/// 文本第二行是否需要（文本工具 / 选中文本 / 正在输入）、贴图编辑模式下输出组不出现（ADR 0028）。
/// 任一条件不成立就不画。
pub struct Shown<'a> {
    /// 工具栏元素显隐（设置面板「工具栏」页）。
    pub vis: &'a ToolbarConfig,
    /// 是否要文本第二行：调用方给「文本工具 / 选中文本 / 正在输入」，此处再与显隐开关求与。
    pub text_row: bool,
    /// 贴图编辑模式（ADR 0028）：输出组（贴图 / 长截图）整体不出现。
    pub show_output: bool,
    /// 选区是否可移动（ADR 0036）：贴图编辑模式选区锁定，「移动」按钮不画。
    pub show_move: bool,
}

impl Shown<'_> {
    /// 该元素是否绘制：显隐开关 + 编辑模式叠加。
    fn draws(&self, slot: ToolbarSlot) -> bool {
        if slot == ToolbarSlot::Move {
            // ADR 0036：选区锁定（贴图编辑）时指针模式无意义，不画按钮
            return self.show_move && self.vis.visible(slot);
        }
        self.vis.visible(slot) && (self.show_output || slot.group() != ToolbarGroup::Output)
    }
}

/// 第一行元素 → (动作, 宽, 高)；第二行整行由 `TextRow` 表达，无单个元素。
fn item_of(slot: ToolbarSlot, ui: Ui) -> Option<(Action, i32, i32)> {
    let (btn, sw) = (ui.px(BTN_BASE), ui.px(SW_BASE));
    Some(match slot {
        ToolbarSlot::Move => (Action::Move, btn, btn),
        ToolbarSlot::Tool(t) => (Action::Tool(t), btn, btn),
        ToolbarSlot::Color(i) => (Action::Color(i), sw, sw),
        ToolbarSlot::Fill => (Action::ToggleFill, btn, btn),
        ToolbarSlot::Undo => (Action::Undo, btn, btn),
        ToolbarSlot::Redo => (Action::Redo, btn, btn),
        ToolbarSlot::Pin => (Action::Pin, btn, btn),
        ToolbarSlot::Longshot => (Action::Longshot, btn, btn),
        ToolbarSlot::Copy => (Action::Copy, btn, btn),
        ToolbarSlot::Save => (Action::Save, btn, btn),
        ToolbarSlot::TextRow => return None,
    })
}

/// 计算工具栏布局：优先放在选区下方，空间不足则放上方。
/// 元素按可见性裁剪、宽度按可见项重算；**全部元素都被隐藏时返回 `None`**（R4：工具栏整体不绘制，
/// 尺寸提示、截图与出图不受影响）。
pub fn layout_ex(sel: SelRect, screen_w: i32, screen_h: i32, shown: &Shown, ui: Ui) -> Option<Toolbar> {
    let (row_h, h2) = (ui.px(H_BASE), ui.px(H2_BASE));
    let btn = ui.px(BTN_BASE);
    let (gap, sep, pad) = (ui.px(GAP_BASE), ui.px(SEP_BASE), ui.px(PAD_BASE));
    let row_gap = ui.px(ROW_GAP_BASE);
    let margin = ui.px(4.0);

    let mut c = Cursor { x: pad, gap, sep };
    let mut hits: Vec<(SelRect, Action)> = Vec::new();
    let mut dividers: Vec<i32> = Vec::new();
    // 按组排布：组内元素全被隐藏时整组消失，组分隔线也随之消失（不留空档）
    let mut groups_drawn = 0usize;
    for group in ToolbarGroup::ALL {
        let mut first_x = None;
        for slot in group.slots() {
            if !shown.draws(*slot) {
                continue;
            }
            let Some((action, w, h)) = item_of(*slot, ui) else {
                continue;
            };
            if first_x.is_none() {
                if groups_drawn > 0 {
                    c.sep();
                }
                first_x = Some(c.x);
            }
            let x = c.next(w);
            let y = (row_h - h) / 2;
            hits.push((SelRect { x, y, w, h }, action));
        }
        if let Some(x) = first_x {
            if groups_drawn > 0 {
                dividers.push(x);
            }
            groups_drawn += 1;
        }
    }
    let row1_empty = hits.is_empty();
    let w1 = if row1_empty { 0 } else { c.x - gap + pad };

    // 第二行（EDT-4）：字体族按钮 + B / I + 字号 − 数值 +
    // 整行一个显隐开关（EDT-16）：隐藏后即使文本工具激活也不出现
    let text_row = shown.text_row && shown.draws(ToolbarSlot::TextRow);
    let mut c2 = Cursor { x: pad, gap, sep };
    let mut hits2: Vec<(SelRect, Action)> = Vec::new();
    let mut item2 = |c: &mut Cursor, w: i32, action: Action| {
        let x = c.next(w);
        let y = (h2 - btn) / 2;
        hits2.push((SelRect { x, y, w, h: btn }, action));
    };
    if text_row {
        item2(&mut c2, ui.px(FONT_BTN_W_BASE), Action::FontPick);
        c2.sep();
        item2(&mut c2, btn, Action::ToggleBold);
        item2(&mut c2, btn, Action::ToggleItalic);
        c2.sep();
        item2(&mut c2, btn, Action::FontSmaller);
        item2(&mut c2, ui.px(SIZE_W_BASE), Action::FontSize);
        item2(&mut c2, btn, Action::FontLarger);
    }
    let w2 = if text_row { c2.x - gap + pad } else { 0 };

    // R4：一个元素都不画就不存在工具栏（连尺寸提示的避让也不需要）
    let total_h = match (row1_empty, text_row) {
        (true, false) => return None,
        (false, false) => row_h,
        (true, true) => h2,
        (false, true) => row_h + row_gap + h2,
    };
    // 第二行相对工具栏顶部的偏移：第一行不存在时不留行间距
    let dy2 = total_h - h2;
    let w = w1.max(w2);
    let x = sel.x.clamp(margin, (screen_w - w - margin).max(margin));
    // 信息文字占位（见 overlay::text_rect）：默认贴选区下方，与文字同侧时再让出一个文字高度；
    // 下方塞不下就翻到上方，且避开文字块（见 geometry::info_text_below）。
    // 贴图编辑模式（`show_output == false`）不画信息文字，也不为它预留空间。
    let show_output = shown.show_output;
    let offset = info_offset(ui);
    let text_below = show_output && info_text_below(sel, screen_h, total_h, ui);
    let below = sel.y + sel.h + offset + if text_below { info_text_h(ui) } else { 0 };
    let y = if below + total_h + margin <= screen_h {
        below
    } else if show_output && text_below {
        (sel.y - offset - total_h).max(ui.px(2.0))
    } else if show_output {
        (sel.y - info_text_h(ui) - offset - offset - total_h).max(ui.px(2.0))
    } else {
        (sel.y - offset - total_h).max(ui.px(2.0))
    };
    // 元素全被隐藏（只剩第二行或什么都没有）时，`first` 是**零尺寸哨兵**：
    // 绘制侧只在 `dividers` 非空时读 `first.y` / `first.h`，而 `dividers` 非空 ⟺ 第一行有元素。
    // 新增读取点请判 `w`/`h`，不要假定 `first` 一定有高度。
    let first = SelRect {
        x,
        y,
        w: w1,
        h: if row1_empty { 0 } else { row_h },
    };
    let second = text_row.then(|| SelRect {
        x,
        y: y + dy2,
        w: w2,
        h: h2,
    });
    let rect = SelRect { x, y, w, h: total_h };

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
        .chain(hits2.into_iter().map(|(r, a)| (offset(r, dy2), a)));
    Some(Toolbar {
        rect,
        first,
        second,
        ui,
        gap,
        hits: hits.collect(),
        dividers: dividers.into_iter().map(|dx| dx + x).collect(),
    })
}
