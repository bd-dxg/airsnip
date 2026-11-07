// SPDX-License-Identifier: GPL-3.0-only
//! 序号标记的编号二次编辑（EDT-12）：双击徽标进入编号编辑，复用文本输入状态机，只收数字字符。
//! 从 `textinput` 拆出，控制单文件规模；输入框绘制与普通文本共用 `textinput::draw_edit`。

use crate::editor::{Kind, Object, Point};
use crate::render;

use super::text_edit_state::TextEdit;

impl super::Overlay {
    /// 双击对象进入二次编辑（EDT-4 文本 / EDT-12 序号）；不可编辑的对象返回 false。
    pub(super) fn begin_edit_object(&mut self, i: usize) -> bool {
        match self.doc.objects().get(i).map(|o| &o.kind) {
            Some(Kind::Text { .. }) => self.begin_edit_text(i),
            Some(Kind::Number { .. }) => self.begin_edit_number(i),
            _ => false,
        }
    }

    /// 双击序号标记进入编号编辑：输入框直接摆在徽标圆心，原徽标隐藏（由输入框里的数字与光标代替）。
    pub(super) fn begin_edit_number(&mut self, i: usize) -> bool {
        let Some(obj) = self.doc.objects().get(i).cloned() else {
            return false;
        };
        let Kind::Number {
            center, font_size, n, ..
        } = obj.kind
        else {
            return false;
        };
        if self.text_edit.is_some() {
            self.commit_text();
        }
        self.selected = Some(i);
        let mut spec = render::text::FontSpec::new(font_size);
        spec.bold = true;
        let label = n.to_string();
        let (tw, th) = render::text::measure(&label, &spec);
        let chars: Vec<char> = label.chars().collect();
        let pos = Point::new(center.x - tw * 0.5, center.y - th * 0.5 + font_size * 0.08);
        self.text_edit = Some(TextEdit {
            pos,
            obj_index: Some(i),
            caret: chars.len(),
            chars,
            comp: Vec::new(),
            pending_high: None,
            font_size,
            font_family: std::sync::Arc::from(render::text::DEFAULT_FAMILY),
            bold: true,
            italic: false,
            number: true,
        });
        self.set_ime_enabled(true);
        self.repaint_text_edit();
        true
    }

    /// 提交编号编辑：只取数字字符，空 / 非法 / 未变化时不入撤销栈；提交后重算徽标半径。
    pub(super) fn finish_number_edit(&mut self, edit: TextEdit) {
        let Some(i) = edit.obj_index else {
            self.repaint_whole_selection();
            return;
        };
        let digits: String = edit.chars.iter().filter(|c| c.is_ascii_digit()).collect();
        let new_n = digits.parse::<u32>().ok().map(|n| n.max(1));
        let changed = match (new_n, self.doc.objects().get(i)) {
            (
                Some(n),
                Some(Object {
                    kind: Kind::Number { n: old, .. },
                    ..
                }),
            ) => *old != n,
            _ => false,
        };
        if changed && let Some(n) = new_n {
            self.doc.begin();
            if let Kind::Number { n: slot, .. } = &mut self.doc.objects_mut()[i].kind {
                *slot = n;
            }
            render::text::remeasure(&mut self.doc.objects_mut()[i].kind);
            self.doc.commit(true);
        }
        self.repaint_whole_selection();
    }
}
