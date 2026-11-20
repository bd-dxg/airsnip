// SPDX-License-Identifier: GPL-3.0-only
//! 序号徽标的内容二次编辑（EDT-12 / EDT-20）：双击分流——
//! **数字徽标**进编号编辑（复用文本输入状态机），**符号徽标**（✓ / ×）直接互相切换
//! （只有两种值，切换比开输入框少三步，且一次撤销可回退）。
//! 编号编辑框接受数字、`v` / `√`（对勾）、`x` / `×`（叉号），**不用输入法**就能在批注时随手改。

use crate::editor::{Badge, Kind, Object, Point};
use crate::overlay::geometry::expand_rect;
use crate::render;

use super::text_edit_state::TextEdit;

impl super::Overlay {
    /// 双击对象进入二次编辑（EDT-4 文本 / EDT-12 序号 / EDT-20 徽标）；不可编辑的对象返回 false。
    /// 符号徽标双击不走输入框，而是直接切换形态（见 `toggle_badge_mark`）。
    pub(super) fn begin_edit_object(&mut self, i: usize) -> bool {
        match self.doc.objects().get(i).map(|o| &o.kind) {
            Some(Kind::Text { .. }) => self.begin_edit_text(i),
            Some(Kind::Number {
                mark: Badge::Check | Badge::Cross,
                ..
            }) => self.toggle_badge_mark(i),
            Some(Kind::Number { .. }) => self.begin_edit_number(i),
            _ => false,
        }
    }

    /// 双击符号徽标 = 在 ✓ / × 之间切换（EDT-20）：只有两种值，切换比开输入框快，
    /// 一次 `Ctrl+Z` 可回退。想把它改成编号：选中后点工具栏序号格子右下角的 ▾ 选「数字」。
    fn toggle_badge_mark(&mut self, i: usize) -> bool {
        let next = match self.doc.objects().get(i).map(|o| &o.kind) {
            Some(Kind::Number { mark, .. }) => mark.toggled(),
            _ => None,
        };
        let Some(next) = next else { return false };
        self.selected = Some(i);
        // 半径由内容宽度决定：对勾与叉号同宽，所以切换后几何不变，旧脏区就够
        let clip = self.selected_clip();
        self.doc.begin();
        if let Kind::Number { mark: slot, .. } = &mut self.doc.objects_mut()[i].kind {
            *slot = next;
        }
        self.doc.commit(true);
        if let Some(d) = clip {
            self.repaint(expand_rect(d, 10));
        }
        self.refresh_toolbar();
        self.repaint_toolbar_chrome();
        true
    }

    /// 双击徽标进入内容编辑：输入框直接摆在徽标圆心，原徽标隐藏（由输入框里的内容与光标代替）。
    pub(super) fn begin_edit_number(&mut self, i: usize) -> bool {
        let Some(obj) = self.doc.objects().get(i).cloned() else {
            return false;
        };
        let Kind::Number {
            center,
            font_size,
            mark,
            ..
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
        let label = mark.edit_text();
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

    /// 提交徽标编辑：三路映射（数字 = 编号，`v` = 对勾，`x` = 叉号）；非法 / 未变化时不入撤销栈，
    /// 提交后重算徽标半径。
    pub(super) fn finish_number_edit(&mut self, edit: TextEdit) {
        let Some(i) = edit.obj_index else {
            self.repaint_whole_selection();
            return;
        };
        let new = Badge::parse_edit(&edit.chars);
        let changed = match (new, self.doc.objects().get(i)) {
            (
                Some(mark),
                Some(Object {
                    kind: Kind::Number { mark: old, .. },
                    ..
                }),
            ) => *old != mark,
            _ => false,
        };
        if changed && let Some(mark) = new {
            self.doc.begin();
            if let Kind::Number { mark: slot, .. } = &mut self.doc.objects_mut()[i].kind {
                *slot = mark;
            }
            render::text::remeasure(&mut self.doc.objects_mut()[i].kind);
            self.doc.commit(true);
        }
        self.repaint_whole_selection();
    }
}
