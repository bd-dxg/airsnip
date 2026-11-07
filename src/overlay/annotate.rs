// SPDX-License-Identifier: GPL-3.0-only
//! 标注状态与输出（EDT-1/2/7、OUT-1/2/3）：当前工具与样式、工具栏动作、
//! 撤销/重做、复制与保存。

mod output;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

use super::{geometry::SelRect, toolbar};
use crate::editor::{self, Style};

impl super::Overlay {
    /// 当前标注色（EDT-7）：预设按 `color_index` 取，自定义槽取 `custom_color`。
    pub(super) fn current_color(&self) -> [u8; 4] {
        editor::swatch(self.color_index, self.custom_color)
    }

    /// 当前样式（EDT-7：颜色 + 连续线宽）。
    pub(super) fn current_style(&self) -> Style {
        Style::new(self.current_color(), self.line_width)
    }

    /// 按下键对应的标注工具（工具切换键配置，M3）。
    pub(super) fn tool_by_key(&self, vk: u32) -> Option<crate::editor::Tool> {
        let k = &self.tool_keys;
        if vk == k.rect {
            Some(crate::editor::Tool::Rect)
        } else if vk == k.arrow {
            Some(crate::editor::Tool::Arrow)
        } else if vk == k.text {
            Some(crate::editor::Tool::Text)
        } else if vk == k.blur {
            Some(crate::editor::Tool::Blur)
        } else if vk == k.highlight {
            Some(crate::editor::Tool::Highlight)
        } else if vk == k.glow {
            Some(crate::editor::Tool::Glow)
        } else if vk == k.number {
            Some(crate::editor::Tool::Number)
        } else {
            None
        }
    }

    /// 鼠标键 / 仅在松键才发现的功能键（ADR 0019、#67）：落到长截图键或工具切换键时执行。
    /// 覆盖层的 PrtScn（只有 WM_KEYUP）与中键/侧键（只有鼠标消息）共用。
    pub(super) fn on_aux_key(&mut self, vk: u32) {
        // 贴图编辑模式（ADR 0028）：输出组（贴图 / 长截图 / OCR）不适用；工具切换仍生效
        let outputs = !self.locked_selection;
        if outputs && vk == self.pin_key {
            self.on_pin(); // PIN-1：贴选区
        } else if outputs && vk == self.longshot_key {
            self.on_longshot();
        } else if outputs && vk == self.ocr_key && self.ocr_enabled {
            self.on_ocr(); // OCR-3：OCR（功能关闭时不吃这个键，留给工具切换）
        } else if let Some(t) = self.tool_by_key(vk) {
            self.switch_tool(t);
        }
    }

    /// 切换到指定工具（经工具栏动作统一处理，使其重绘）。
    pub(super) fn switch_tool(&mut self, t: crate::editor::Tool) {
        self.apply_action(toolbar::Action::Tool(t));
    }

    pub(super) fn bar_state(&self) -> toolbar::State {
        let (font_family, font_size, bold, italic) = self.font_state();
        toolbar::State {
            tool: self.tool,
            color: self.color_index,
            custom_color: self.custom_color,
            filled: self.filled,
            can_undo: self.doc.can_undo(),
            can_redo: self.doc.can_redo(),
            font_family,
            bold,
            italic,
            font_size,
            font_open: self.font_list.is_some(),
            hover: self.tip.hover_button(),
        }
    }

    /// 执行工具栏动作（EDT-7）。
    pub(super) fn apply_action(&mut self, action: toolbar::Action) {
        // 除「自定义颜色」外，任何工具栏动作都收起取色面板（EDT-7，issue #73）
        if action != toolbar::Action::Color(editor::CUSTOM_INDEX) {
            self.close_color_panel();
        }
        match action {
            toolbar::Action::Tool(t) => self.tool = t,
            toolbar::Action::Color(i) => {
                // 自定义色槽（EDT-7，issue #73）：展开 / 收起取色面板，不走普通色块流程
                if i == editor::CUSTOM_INDEX {
                    return self.toggle_color_panel();
                }
                self.color_index = i;
                // EDT-7：颜色同时作用于选中的对象
                self.style_selected();
            }
            toolbar::Action::ToggleFill => {
                // 文本 / 序号工具无背景功能：填充开关仅对矩形有效，这两类下忽略
                if !matches!(self.tool, crate::editor::Tool::Text | crate::editor::Tool::Number) {
                    self.filled = !self.filled;
                    self.style_selected();
                }
            }
            // EDT-7：无删除/层级按钮（用户取舍：删除仅能靠 Ctrl+Z 撤销）
            toolbar::Action::Undo => return self.on_undo(),
            toolbar::Action::Redo => return self.on_redo(),
            // PIN-1：贴图按钮 → 选区贴图后覆盖层自行关闭（不复绘）
            toolbar::Action::Pin => return self.on_pin(),
            // M4：长截图按钮 → 选区作为视口进入拼接（不复绘）
            toolbar::Action::Longshot => return self.on_longshot(),
            // OCR-3：OCR 按钮 → 原图交给主线程识别（不复绘）
            toolbar::Action::Ocr => return self.on_ocr(),
            // OUT-1：复制按钮 → 复制到剪贴板并关闭覆盖层（不复绘）
            toolbar::Action::Copy => return self.on_copy(),
            // OUT-2：保存按钮 → 按配置保存并关闭覆盖层（不复绘）
            toolbar::Action::Save => return self.on_save(),
            // EDT-4：文本第二行（字体族 / 粗斜体 / 字号）见 `font`
            toolbar::Action::FontPick
            | toolbar::Action::ToggleBold
            | toolbar::Action::ToggleItalic
            | toolbar::Action::FontSmaller
            | toolbar::Action::FontLarger
            | toolbar::Action::FontSize => return self.on_font_action(action),
        }
        // 工具栏自身外观或第二行显隐可能变化
        self.refresh_toolbar();
        self.repaint_toolbar_chrome();
    }

    /// 撤销（Ctrl+Z）。
    pub(super) fn on_undo(&mut self) {
        if self.doc.undo() {
            self.selected = None; // 索引已变，控制柄不再对应
            self.refresh_toolbar(); // 选中文本对象消失时第二行随之收起（EDT-4）
            self.repaint_after_history();
        }
    }

    /// 重做（Ctrl+Y / Ctrl+Shift+Z）。
    pub(super) fn on_redo(&mut self) {
        if self.doc.redo() {
            self.selected = None;
            self.refresh_toolbar();
            self.repaint_after_history();
        }
    }

    /// 撤销/重做后重绘：被撤销的对象可能位于选区之外，而另一份历史快照里的
    /// 对象包围盒不可得，故整屏重绘（仅按键/按钮触发，不在拖拽路径上）。
    fn repaint_after_history(&mut self) {
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        self.repaint(SelRect {
            x: 0,
            y: 0,
            w: vw,
            h: vh,
        });
    }

    /// 取消截图（OUT-3 / ADR 0027）：`Esc` 与右键共用。分层回退，
    /// 只有会丢失已完成的标注时才二次确认，避免一次误触清空全部工作（issue #114）。
    pub(super) fn on_cancel(&mut self) {
        match cancel_step(
            self.color_panel_open(),
            self.font_list_open(),
            self.is_editing_text(),
            !self.doc.is_empty(),
        ) {
            CancelStep::ColorPanel => self.close_color_panel(),
            CancelStep::FontPanel => self.close_font_list(),
            // 文本 / 序号输入中右键 = 完成本次输入（保留文字），而非丢弃；
            // Esc 取消输入走 `on_text_key`（键盘路径），不经过这里。
            CancelStep::CommitText => self.commit_text(),
            CancelStep::Confirm => {
                // 贴图编辑模式（ADR 0028）文案改为「退出编辑」
                let msg = if self.locked_selection {
                    "当前标注尚未保存，确定放弃并退出编辑？"
                } else {
                    "当前标注尚未保存，确定放弃并退出截图？"
                };
                if crate::app::confirm(Some(self.hwnd), msg) {
                    self.exit_cancel();
                }
            }
            CancelStep::Exit => self.exit_cancel(),
        }
    }

    /// 直接取消：置标志并投递关闭消息，由消息循环退出覆盖层。
    fn exit_cancel(&mut self) {
        self.cancelled = true;
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    /// 结束覆盖层：取消标志已由调用方设置，这里只投递关闭消息。
    pub(super) fn close(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// 取消截图的分层步骤（OUT-3 / ADR 0027）：由内向外逐层回退，
/// 只有会丢标注时才需确认。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum CancelStep {
    /// 收起取色面板。
    ColorPanel,
    /// 收起字体面板。
    FontPanel,
    /// 完成当前文本 / 序号输入（右键语义：保留已输入内容）。
    CommitText,
    /// 有未保存标注，二次确认。
    Confirm,
    /// 直接退出。
    Exit,
}

/// 取消分层决策（纯函数，便于单测）：面板优先于输入，输入优先于退出；
/// 有标注时改为确认。
pub(super) fn cancel_step(color_panel: bool, font_panel: bool, editing: bool, has_annotations: bool) -> CancelStep {
    if color_panel {
        CancelStep::ColorPanel
    } else if font_panel {
        CancelStep::FontPanel
    } else if editing {
        CancelStep::CommitText
    } else if has_annotations {
        CancelStep::Confirm
    } else {
        CancelStep::Exit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 分层优先级：取色面板 > 字体面板 > 文本输入 > （有标注？确认）> 退出。
    #[test]
    fn cancel_step_recedes_layer_by_layer() {
        assert_eq!(cancel_step(true, true, true, true), CancelStep::ColorPanel);
        assert_eq!(cancel_step(false, true, true, true), CancelStep::FontPanel);
        assert_eq!(cancel_step(false, false, true, true), CancelStep::CommitText);
        // 无面板 / 无输入：有标注需确认，否则直接退出
        assert_eq!(cancel_step(false, false, false, true), CancelStep::Confirm);
        assert_eq!(cancel_step(false, false, false, false), CancelStep::Exit);
        // 有标注不会绕过前面的层
        assert_eq!(cancel_step(false, true, false, true), CancelStep::FontPanel);
    }
}
