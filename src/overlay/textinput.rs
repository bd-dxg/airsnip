// SPDX-License-Identifier: GPL-3.0-only
//! 文本输入接线（EDT-4）：「输入中」状态的开始/提交/取消、IME 消息读取与输入框绘制。
//! 编辑状态数据与字符级逻辑见 `text_edit_state`；绘制在覆盖层像素图上，不创建系统控件（ADR 0004）。

mod clip;
mod draw;
pub(super) use draw::draw_edit;

use windows::Win32::UI::Input::Ime::{GCS_COMPSTR, GCS_RESULTSTR};

use crate::editor::{Kind, Object, Point};
use crate::render;

use super::geometry::{expand_rect, union_rect};
use super::text_edit_state::TextEdit;
use super::wndproc::ime_string;

impl super::Overlay {
    /// 折行可用宽度（issue #221）：选区右边界 − 文字起点，**至少 1 个字宽**。
    /// 选区窄得装不下一个字时仍给一个字号宽 —— 否则每行只有一个字，文字堆成竖列。
    /// 没有选区时返回 `None`（不折行）。
    fn text_wrap_at(&self, x: f32) -> Option<f32> {
        let sel = self.selection?;
        Some(((sel.x + sel.w) as f32 - x).max(self.font_size))
    }

    /// 进入文本输入（Text 工具下点击选区内空白，EDT-4）。
    /// 撤销快照不在此时建立：输入期间其它操作（如滚轮微调对象）可能抢先 begin，
    /// 覆盖 pending 导致提交时快照丢失，故统一在 `commit_text` 修改文档前建立。
    pub(super) fn begin_text_input(&mut self, x: i32, y: i32) {
        let edit = TextEdit {
            pos: Point::new(x as f32, y as f32),
            obj_index: None,
            chars: Vec::new(),
            comp: Vec::new(),
            caret: 0,
            pending_high: None,
            font_size: self.font_size,
            font_family: self.font_family.clone(),
            bold: self.bold,
            italic: self.italic,
            number: false,
            select_all: false,
            hint: None,
            wrap: self.text_wrap_at(x as f32),
        };
        self.text_edit = Some(edit);
        self.set_ime_enabled(true);
        self.sync_ime_window_pos(); // 候选框先锚到首光标（issue #139）
        self.repaint_text_edit();
    }

    /// 双击已提交的文本对象进入再编辑（EDT-4）；非文本对象时返回 false。
    pub(super) fn begin_edit_text(&mut self, i: usize) -> bool {
        let obj = self.doc.objects().get(i).cloned();
        let Some(obj) = obj else { return false };
        let Kind::Text {
            pos,
            text,
            font_size,
            font_family,
            bold,
            italic,
            wrap,
            ..
        } = obj.kind
        else {
            return false;
        };
        // 先收尾上一次输入（提交，不回滚）
        if self.text_edit.is_some() {
            self.commit_text();
        }
        self.selected = Some(i);
        self.text_edit = Some(TextEdit {
            pos,
            obj_index: Some(i),
            chars: text.chars().collect(),
            comp: Vec::new(),
            caret: text.chars().count(),
            pending_high: None,
            font_size,
            font_family,
            bold,
            italic,
            number: false,
            select_all: false,
            hint: None,
            // 重编辑按**当前选区**重算折行宽（issue #221）：改完选区想重排，双击进来重新提交即可；
            // 算出来的宽度小到离谱（对象已被拖到选区右侧外）则沿用原来的，而不是压成一字一列。
            wrap: self.text_wrap_at(pos.x).filter(|w| *w >= font_size * 2.0).or(wrap),
        });
        self.set_ime_enabled(true);
        self.sync_ime_window_pos(); // 再编辑从对象末尾的光标起（issue #139）
        // 原对象隐藏、输入框登场，立即重绘
        self.repaint_text_edit();
        true
    }

    /// 是否正在文本输入。
    pub(super) fn is_editing_text(&self) -> bool {
        self.text_edit.is_some()
    }

    /// 提交当前输入（EDT-4）：空文本不创建（编辑时清空 = 删除对象）；Enter / 点击别处触发。
    /// 修改文档前才建撤销快照（begin），避免输入期间其它 begin 覆盖；结束后重绘整个选区。
    pub(super) fn commit_text(&mut self) {
        let Some(edit) = self.text_edit.take() else { return };
        self.set_ime_enabled(false);
        // EDT-12：改徽标内容（编号 / 对勾 / 叉号）走单独分支，空 / 非法则保持原样
        if edit.number {
            self.finish_number_edit(edit);
            return;
        }
        if edit.chars.is_empty() {
            // 编辑已有对象时全部删掉 = 删除该对象；新建时空文本直接丢弃
            if let Some(i) = edit.obj_index {
                self.doc.begin();
                self.doc.remove(i);
                self.doc.commit(true);
                self.selected = None;
            }
            self.repaint_whole_selection();
            return;
        }
        let text: String = edit.chars.iter().collect();
        let size = render::text::measure_wrapped(&text, &edit.spec(), edit.wrap);
        let kind = Kind::Text {
            pos: edit.pos,
            text: std::sync::Arc::from(text),
            font_size: edit.font_size,
            font_family: edit.font_family,
            bold: edit.bold,
            italic: edit.italic,
            wrap: edit.wrap,
            size,
        };
        match edit.obj_index {
            None => {
                self.doc.begin();
                self.doc.push(Object {
                    kind,
                    // 文本不支持渐变（EDT-19）：即使色块停在渐变上，也用代表色写成纯色
                    style: self.current_style().with_solid(self.current_color()),
                });
                self.doc.commit(true);
                self.selected = Some(self.doc.objects().len() - 1);
            }
            Some(i) => {
                self.doc.begin();
                self.doc.objects_mut()[i].kind = kind;
                self.doc.commit(true);
            }
        }
        self.repaint_whole_selection();
    }

    /// 取消当前输入（Esc，EDT-4）：编辑已有对象时不入撤销栈，原对象恢复显示。
    pub(super) fn cancel_text(&mut self) {
        let Some(_edit) = self.text_edit.take() else { return };
        self.set_ime_enabled(false);
        self.doc.commit(false);
        self.repaint_whole_selection();
    }

    /// 文本输入变化后的重绘：覆盖整个选区，彻底避免文本变短/光标位移时的尾部残留。
    /// `pub(super)`：编号编辑（`number_edit`）提交后也复用。
    pub(super) fn repaint_whole_selection(&mut self) {
        // 输入态的开始/结束会影响第二行显隐（EDT-4）
        self.refresh_toolbar();
        // 脏区取「选区 + 输入框」的并集：输入框可能超出选区（#221 自动换行尚未做），
        // 只重绘选区会把框外那段文字与「已复制」提示（issue #220）的残影留在屏上。
        let sel = self.selection.map(|s| expand_rect(s, 2));
        let edit = self.text_edit.as_ref().map(|e| expand_rect(e.region(), 2));
        if let Some(r) = union_rect(sel, edit) {
            self.repaint(r);
        }
    }

    /// WM_CHAR：普通字符（含标点/数字），跳过控制字符。
    pub(super) fn on_text_char(&mut self, unit: u16) {
        if (unit as u32) < 0x20 {
            return;
        }
        let Some(edit) = self.text_edit.as_mut() else { return };
        // EDT-12 / EDT-20：徽标内容编辑只收 `Badge::accepts_char` 认可的字符
        // （数字 = 编号，`v` / `x` 与勾叉 = 符号），最多 4 个字符（避免荒谬的大数与长串）
        if edit.number {
            let ok = char::from_u32(unit as u32).is_some_and(crate::editor::Badge::accepts_char);
            if !ok || edit.chars.len() >= 4 {
                return;
            }
        }
        edit.push_code_unit(unit);
        self.repaint_text_edit();
    }

    /// WM_IME_COMPOSITION：更新组合串或读取结果串。`flags` 为 lparam 位标志。
    pub(super) fn on_text_composition(&mut self, flags: u32) {
        let Some(edit) = self.text_edit.as_mut() else { return };
        let changed = if flags & GCS_RESULTSTR.0 != 0 {
            let s = ime_string(self.hwnd, GCS_RESULTSTR);
            edit.commit_composition(s);
            true
        } else if flags & GCS_COMPSTR.0 != 0 {
            let s = ime_string(self.hwnd, GCS_COMPSTR);
            edit.set_composition(s);
            true
        } else {
            false
        };
        if changed {
            // 组合串变长会让光标右移，候选框跟着走（issue #139）
            self.repaint_text_edit();
            self.sync_ime_window_pos();
        }
    }

    /// IME 组合结束：清空候选。
    pub(super) fn on_text_composition_end(&mut self) {
        let Some(edit) = self.text_edit.as_mut() else { return };
        if !edit.comp.is_empty() {
            edit.comp.clear();
            self.repaint_text_edit();
        }
    }

    /// 输入中的按键（WM_KEYDOWN）；返回是否已处理。`shift`/`ctrl`：
    /// Enter + Shift/Ctrl 换行（EDT-4），单独 Enter 提交。
    pub(super) fn on_text_key(&mut self, vk: u32, shift: bool, ctrl: bool) -> bool {
        if !self.is_editing_text() {
            return false;
        }
        // issue #220：剪贴板组合键（全选 / 复制 / 粘贴）在取 `edit` 借用之前处理，
        // 那一侧要碰剪贴板与提示几何（`&mut self`），不能同时握着 `edit`。
        if let Some(action) = clip::action(vk, ctrl) {
            self.handle_clip_key(action);
            return true;
        }
        let Some(edit) = self.text_edit.as_mut() else {
            return false;
        };
        let mut handled = true;
        match vk {
            0x08 => edit.backspace(),                 // Backspace
            0x2E => edit.delete_forward(),            // Delete
            0x25 => edit.move_caret(-1),              // ←
            0x27 => edit.move_caret(1),               // →
            0x24 => edit.set_caret(0),                // Home
            0x23 => edit.set_caret(edit.chars.len()), // End
            // Enter：Shift/Ctrl 组合换行（编号编辑不换行），单独 Enter 提交
            0x0D if (shift || ctrl) && !edit.number => edit.insert_char('\n'),
            0x0D => self.commit_text(), // Enter：提交
            // Esc：字体面板展开时先收面板（保留输入态），否则取消输入（EDT-4）
            0x1B => {
                if self.font_list_open() {
                    self.close_font_list();
                } else {
                    self.cancel_text();
                }
            }
            // Ctrl+Z：放弃本次未提交输入（已提交文本走全局撤销 Ctrl+Z）
            0x5A if ctrl => self.cancel_text(),
            _ => handled = false,
        }
        if handled {
            self.repaint_text_edit();
        }
        handled
    }

    /// 输入框重绘：整个选区重绘（文本变短/光标位移时旧像素可能落在新 region 外，
    /// 局部重绘必然残留；文本输入是低频按键事件，选区级重绘成本可接受）。
    /// 轻提示（issue #220）只活到下一次按键，所以先把它收掉。
    pub(super) fn repaint_text_edit(&mut self) {
        self.drop_text_hint();
        self.repaint_whole_selection();
    }
}
