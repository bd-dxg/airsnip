// SPDX-License-Identifier: GPL-3.0-only
//! 文本编辑状态（EDT-4）：「输入中」的纯数据与字符级编辑逻辑。
//! IME 消息的读取与提交见 `textinput`（涉及 HWND/消息循环）。

use crate::editor::Point;
use crate::render;

use super::geometry::{SelRect, union_rect};

/// 输入期的轻提示（issue #220）：「已复制 12 字」这类一次性反馈。
/// 矩形在产生时就算好（那边有画布尺寸与 UI 缩放），绘制只负责贴字；
/// 它只活到下一次按键，所以不需要定时器（与 `toast` / `cancel_hint` 不同）。
pub(super) struct TextHint {
    pub rect: SelRect,
    pub text: String,
}

/// 输入中状态（EDT-4）。
pub(super) struct TextEdit {
    /// 文字左上角（覆盖层坐标，不含内边距）。
    pub pos: Point,
    /// 编辑已有对象的索引；None 表示新建。
    pub obj_index: Option<usize>,
    /// 已确认文本（字符单位）。
    pub chars: Vec<char>,
    /// IME 组合串（候选预览，显示在光标处）。
    pub comp: Vec<char>,
    /// 光标位置（`chars` 中的索引）。
    pub caret: usize,
    /// 正在积攒的高位代理项（WM_CHAR 补充平面字符）。
    pub pending_high: Option<u16>,
    /// 字号（物理像素）。
    pub font_size: f32,
    /// 字体族名、粗体、斜体（EDT-4）。
    pub font_family: std::sync::Arc<str>,
    pub bold: bool,
    pub italic: bool,
    /// EDT-12：本次输入是在改序号标记的编号（只收数字字符，提交时写回 `n`）。
    pub number: bool,
    /// 全选状态（issue #220）：`Ctrl+A` 置位，任何编辑动作清掉。
    /// 本批只有「全选 / 无」两态，没有任意区间选（见 ADR 0049）。
    pub select_all: bool,
    /// 剪贴板操作的轻提示（issue #220），不落文档、不进导出。
    pub hint: Option<TextHint>,
    /// 自动折行的可用宽度（issue #221）：`None` = 不折行（徽标编辑、选区过窄时）。
    pub wrap: Option<f32>,
}

impl TextEdit {
    /// 渲染用字体规格（EDT-4）。
    pub fn spec(&self) -> render::text::FontSpec {
        render::text::FontSpec::from_parts(&self.font_family, self.font_size, self.bold, self.italic)
    }

    /// 完整显示文本（已确认 + 组合串）。
    pub fn display(&self) -> String {
        self.chars.iter().chain(self.comp.iter()).collect()
    }

    /// 光标所在位置（相对文字左上角的 `(dx, top)`，issue #221）。
    /// 走 DirectWrite 的 `HitTestTextPosition`：软换行后「第几行」不是数 `\n` 能算出来的，
    /// 而自己按字符累加宽度又与字距 / 合字 / 标点禁则对不齐。
    pub fn caret_pos(&self) -> (f32, f32) {
        render::text::caret_offset(&self.display(), &self.spec(), self.wrap, self.caret_index())
    }

    /// 光标在文本串里的下标：IME 组合串排在光标处（末尾），组合期间光标贴在组合串之后。
    fn caret_index(&self) -> usize {
        if self.caret == self.chars.len() {
            self.caret + self.comp.len()
        } else {
            self.caret
        }
    }

    /// 单行高度（测量空串，渲染与光标共用）。
    pub fn line_height(&self) -> f32 {
        let (_, h) = render::text::measure("", &self.spec());
        h.max(self.font_size * 1.2)
    }

    /// 输入框占据的像素区域（含内边距与轻提示，供脏区与绘制使用）。
    pub fn region(&self) -> SelRect {
        let (w, h) = render::text::measure_wrapped(&self.display(), &self.spec(), self.wrap);
        let h = h.max(self.line_height());
        let pad = crate::editor::text::pad(self.font_size);
        let x0 = (self.pos.x - pad).floor() as i32;
        let y0 = (self.pos.y - pad).floor() as i32;
        let text = SelRect {
            x: x0,
            y: y0,
            w: (self.pos.x + w + pad).ceil() as i32 - x0,
            h: (self.pos.y + h + pad).ceil() as i32 - y0,
        };
        // 轻提示画在输入框**下方**（issue #220）：并进区域，绘制裁剪与脏区都跟着它走。
        match &self.hint {
            Some(h) => union_rect(Some(text), Some(h.rect)).unwrap_or(text),
            None => text,
        }
    }

    /// 全部已确认文本（不含 IME 组合串：组合串还没落定，复制它等于复制尚未输入的东西）。
    pub fn all_text(&self) -> String {
        self.chars.iter().collect()
    }

    /// `Ctrl+A` 全选（issue #220）：本批只有「全选 / 无」两态。
    pub fn select_all_text(&mut self) {
        self.select_all = true;
        self.hint = None;
    }

    /// 有全选时清空全部内容（标准「选区被输入替换」语义）。
    fn replace_all(&mut self) {
        if self.select_all {
            self.chars.clear();
            self.caret = 0;
            self.select_all = false;
        }
    }

    /// 在光标处插入一串字符（`Ctrl+V` 粘贴，issue #220）；返回插入的字符数。
    pub fn insert_str(&mut self, text: &str) -> usize {
        self.replace_all();
        let incoming: Vec<char> = text.chars().collect();
        let start = self.caret;
        self.chars.splice(start..start, incoming.iter().copied());
        self.caret = start + incoming.len();
        self.hint = None;
        incoming.len()
    }

    /// WM_CHAR：单个 UTF-16 码元入缓冲（补充平面字符按代理对组合）。
    pub fn push_code_unit(&mut self, unit: u16) {
        if let Some(high) = self.pending_high.take() {
            if let Some(c) = char::from_u32(((high as u32) << 10) + (unit as u32) + 0x10000 - (0xD800 << 10) - 0xDC00) {
                self.insert_char(c);
                return;
            }
        }
        if (0xD800..=0xDBFF).contains(&unit) {
            self.pending_high = Some(unit);
            return;
        }
        if let Some(c) = char::from_u32(unit as u32) {
            self.insert_char(c);
        }
    }

    /// 在光标处插入字符。
    pub fn insert_char(&mut self, c: char) {
        self.replace_all();
        self.chars.insert(self.caret, c);
        self.caret += 1;
    }

    pub fn backspace(&mut self) {
        if self.select_all {
            self.replace_all();
            return;
        }
        if self.caret > 0 {
            self.caret -= 1;
            self.chars.remove(self.caret);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.select_all {
            self.replace_all();
            return;
        }
        if self.caret < self.chars.len() {
            self.chars.remove(self.caret);
        }
    }

    /// 移动光标（`Shift` 不扩选：本批没有区间选区，只把全选收掉）。
    pub fn move_caret(&mut self, delta: i32) {
        self.select_all = false;
        self.caret = (self.caret as i32 + delta).clamp(0, self.chars.len() as i32) as usize;
    }

    /// 光标跳到行首 / 行尾（`Home` / `End`）：同样只收掉全选，不扩选。
    pub fn set_caret(&mut self, caret: usize) {
        self.select_all = false;
        self.caret = caret.min(self.chars.len());
    }

    /// IME 组合串更新（GCS_COMPSTR）。
    pub fn set_composition(&mut self, comp: String) {
        self.comp = comp.chars().collect();
    }

    /// IME 结果串（GCS_RESULTSTR）：在光标处插入结果字符并清空组合预览。
    pub fn commit_composition(&mut self, result: String) {
        self.replace_all();
        let out: Vec<char> = result.chars().collect();
        let start = self.caret;
        self.chars.splice(start..start, out.iter().copied());
        self.caret = start + out.len();
        self.comp.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit() -> TextEdit {
        TextEdit {
            pos: Point::new(10.0, 10.0),
            obj_index: None,
            chars: "你好".chars().collect(),
            comp: Vec::new(),
            caret: 2,
            pending_high: None,
            font_size: 24.0,
            font_family: std::sync::Arc::from(crate::editor::DEFAULT_FONT_FAMILY),
            bold: false,
            italic: false,
            number: false,
            select_all: false,
            hint: None,
            wrap: None,
        }
    }

    /// 光标所在行号（`chars[..caret]` 里的硬换行数）。
    /// 只用于「换行符导航」的断言（软换行改写位置后，视觉行不再等于它）。
    fn hard_row(e: &TextEdit) -> usize {
        e.chars[..e.caret].iter().filter(|&&c| c == '\n').count()
    }

    #[test]
    fn insert_and_move_caret() {
        let mut e = edit();
        e.insert_char('a');
        assert_eq!(e.chars.iter().collect::<String>(), "你好a");
        assert_eq!(e.caret, 3);
        e.move_caret(-2);
        e.insert_char('b');
        assert_eq!(e.chars.iter().collect::<String>(), "你b好a");
        e.move_caret(99);
        assert_eq!(e.caret, e.chars.len());
        e.move_caret(-99);
        assert_eq!(e.caret, 0);
    }

    #[test]
    fn backspace_and_delete() {
        let mut e = edit();
        e.caret = 1;
        e.backspace();
        assert_eq!(e.chars.iter().collect::<String>(), "好");
        assert_eq!(e.caret, 0);
        e.insert_char('x');
        e.delete_forward();
        assert_eq!(e.chars.iter().collect::<String>(), "x");
    }

    #[test]
    fn composition_commit_inserts_result_at_caret() {
        let mut e = edit();
        e.caret = 2;
        e.set_composition("nihao".to_string());
        assert_eq!(e.display(), "你好nihao");
        e.commit_composition("你".to_string());
        assert_eq!(e.chars.iter().collect::<String>(), "你好你");
        assert!(e.comp.is_empty());
        assert_eq!(e.caret, 3);
    }

    #[test]
    fn surrogate_pair_assembles_emoji() {
        let mut e = edit();
        e.push_code_unit(0xD83D); // 🙏 高位代理
        e.push_code_unit(0xDE4F); // 低位代理
        assert_eq!(e.chars.last(), Some(&'🙏'));
        assert_eq!(e.caret, 3);
    }

    #[test]
    fn region_covers_text_with_padding() {
        let r = edit().region();
        assert!(r.w >= 24 && r.h >= 24, "输入框区域应覆盖文字：{r:?}");
        assert!(r.x <= 10 && r.y <= 10);
    }

    #[test]
    fn multiline_insert_and_caret_positioning() {
        let mut e = edit(); // "你好" caret=2
        e.insert_char('\n'); // "你好\n" caret=3
        assert_eq!(hard_row(&e), 1, "换行后光标应在第二行");
        e.insert_char('x'); // caret=4
        e.insert_char('y'); // caret=5
        e.move_caret(-2); // caret=3，位于换行符之后（第二行行首）
        assert_eq!(hard_row(&e), 1);
        e.move_caret(-1); // caret=2（跨过换行符回到第一行末尾）
        assert_eq!(hard_row(&e), 0);
        e.move_caret(1); // caret=3（回到换行符之后），退格删除换行符
        e.backspace();
        assert_eq!(e.chars.iter().collect::<String>(), "你好xy");
        assert_eq!(hard_row(&e), 0, "删除换行后回到第一行");
    }

    #[test]
    fn multiline_region_is_taller() {
        let single = edit();
        let mut multi = edit();
        multi.insert_char('\n');
        multi.insert_char('a');
        multi.insert_char('b');
        let h_single = single.region().h;
        let h_multi = multi.region().h;
        assert!(h_multi > h_single, "多行输入框应更高：{h_single} vs {h_multi}");
    }

    #[test]
    fn multiline_caret_x_resets_at_line_start() {
        let mut multi = edit();
        multi.insert_char('\n'); // caret=3
        multi.insert_char('a'); // caret=4
        multi.insert_char('b'); // caret=5
        let x_mid = multi.caret_pos().0;
        assert!(x_mid > 5.0, "第二行有字符时光标 x 应大于 0：{x_mid}");
        multi.move_caret(-2); // 回到第二行行首
        assert!(
            multi.caret_pos().0 < 1.0,
            "第二行行首 x 应≈0（不累计第一行宽度）：{}",
            multi.caret_pos().0
        );
    }

    /// issue #220：全选后输入字符 = 替换全部（标准编辑器的选区语义）。
    #[test]
    fn typing_over_select_all_replaces_everything() {
        let mut e = edit(); // "你好"
        e.select_all_text();
        assert!(e.select_all);
        e.insert_char('a');
        assert_eq!(e.chars.iter().collect::<String>(), "a");
        assert_eq!(e.caret, 1, "光标应在替换后的字符之后");
        assert!(!e.select_all, "输入后全选应解除");
    }

    /// issue #220：全选后粘贴 / 退格同样是「替换 / 删除全部」。
    #[test]
    fn paste_and_backspace_respect_select_all() {
        let mut e = edit();
        e.select_all_text();
        assert_eq!(e.insert_str("粘贴"), 2);
        assert_eq!(e.chars.iter().collect::<String>(), "粘贴");
        assert_eq!(e.caret, 2);
        // 再全选后粘贴：替换成新内容
        e.select_all_text();
        e.insert_str("x");
        assert_eq!(e.all_text(), "x");
        // 全选后退格：清空
        e.select_all_text();
        e.backspace();
        assert!(e.chars.is_empty());
        assert_eq!(e.caret, 0);
        assert!(!e.select_all);
    }

    /// issue #220：粘贴插在光标处，不动两侧原内容。
    #[test]
    fn paste_inserts_at_caret() {
        let mut e = edit(); // "你好"
        e.caret = 1;
        assert_eq!(e.insert_str("ab"), 2);
        assert_eq!(e.all_text(), "你ab好");
        assert_eq!(e.caret, 3);
    }

    /// issue #220：方向键 / Home / End 只收掉全选，**不删字**。
    #[test]
    fn caret_move_cancels_select_all_without_deleting() {
        let mut e = edit();
        e.select_all_text();
        e.move_caret(-1);
        assert_eq!(e.all_text(), "你好");
        assert!(!e.select_all);
        e.select_all_text();
        e.set_caret(0);
        assert_eq!(e.caret, 0);
        assert!(!e.select_all);
        // 越界光标钳回末尾（不 panic）
        e.set_caret(99);
        assert_eq!(e.caret, e.chars.len());
    }

    /// issue #220：轻提示并进输入框区域（框外那段也要进脏区，否则提示消失后留残影）。    #[test]
    fn region_covers_hint_below_box() {
        let mut e = edit();
        let base = e.region();
        e.hint = Some(TextHint {
            rect: SelRect {
                x: base.x,
                y: base.y + base.h + 4,
                w: base.w + 10,
                h: 20,
            },
            text: "已复制 2 字".into(),
        });
        let with_hint = e.region();
        assert!(with_hint.h > base.h, "提示应拉高区域：{base:?} → {with_hint:?}");
        assert!(with_hint.w >= base.w);
    }

    /// issue #221：编辑态给定折行宽度后，光标能落到第二行、输入框变高。
    #[test]
    fn edit_box_wraps_and_caret_moves_down() {
        let mut e = edit();
        e.font_size = 20.0;
        e.chars = "一二三四五六七八".chars().collect();
        e.caret = e.chars.len();
        // 不折行：一行到底，光标 top 为 0
        e.wrap = None;
        let free_h = e.region().h;
        assert_eq!(e.caret_pos().1, 0.0, "不折行时光标应在第一行");
        // 每行 3 个字：第 8 个字落到第三行
        e.wrap = Some(62.0);
        let (x, top) = e.caret_pos();
        assert!(top > 0.0, "折行后光标应下沉：top={top}");
        assert!(x <= 62.0, "折行后光标 x 不应越出可用宽：{x}");
        assert!(e.region().h > free_h, "折行后输入框应变高：{free_h} → {}", e.region().h);
    }

    /// issue #221：折行宽度不影响单行高度与光标行内语义（回归旧行为）。
    #[test]
    fn wrap_does_not_change_line_height() {
        let e = edit();
        let a = e.line_height();
        let mut w = edit();
        w.wrap = Some(60.0);
        assert_eq!(a, w.line_height(), "行高与折行宽度无关");
    }
}
