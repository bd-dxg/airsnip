// SPDX-License-Identifier: GPL-3.0-only
//! 标注状态与输出（EDT-1/2/7、OUT-1/2/3）：当前工具与样式、工具栏动作、
//! 撤销/重做、复制与保存。

mod output;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

use super::{geometry::SelRect, toolbar};
use crate::editor::{Kind, Object, Style};

/// 填充开关是否可用（EDT-1/EDT-14）：当前工具能画可填充形状，
/// **或**选中对象本身就是矩形 / 椭圆（此时按钮仍需能改它的填充）。
/// 绘制置灰（`toolbar::draw` 读 `State::fill_enabled`）与动作副作用（`apply_action`）
/// 共用这一判据，不会各自演化。
fn fill_enabled_for(tool: crate::editor::Tool, selected: Option<&Object>) -> bool {
    matches!(tool, crate::editor::Tool::Rect | crate::editor::Tool::Ellipse)
        || selected.is_some_and(|o| matches!(o.kind, Kind::Rect { .. } | Kind::Ellipse { .. }))
}

impl super::Overlay {
    /// 当前标注色（EDT-7）：当前色块的颜色。
    pub(super) fn current_color(&self) -> [u8; 4] {
        self.colors[self.color_index]
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
        } else if vk == k.line {
            Some(crate::editor::Tool::Line)
        } else if vk == k.ellipse {
            Some(crate::editor::Tool::Ellipse)
        } else if vk == k.freehand {
            Some(crate::editor::Tool::Freehand)
        } else {
            None
        }
    }

    /// 鼠标键 / 仅在松键才发现的功能键（ADR 0019、#67）：落到长截图键或工具切换键时执行。
    /// 覆盖层的 PrtScn（只有 WM_KEYUP）与中键/侧键（只有鼠标消息）共用。
    pub(super) fn on_aux_key(&mut self, vk: u32) {
        // 贴图编辑模式（ADR 0028）：输出组（贴图 / 长截图）不适用；工具切换仍生效
        let outputs = !self.locked_selection;
        if outputs && vk == self.pin_key {
            self.on_pin(); // PIN-1：贴选区
        } else if outputs && vk == self.longshot_key {
            self.on_longshot();
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
            colors: self.colors,
            filled: self.filled,
            fill_enabled: fill_enabled_for(self.tool, self.selected_object()),
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
    /// **这里不得查「工具栏可见性」**（EDT-16）：隐藏只是不绘制，能力必须照旧——
    /// 隐藏的工具仍可由工具切换键切过来，隐藏的按钮对应的快捷键也照旧生效。
    /// （可见性只有 `overlay::toolbar::layout` 与设置面板两个消费方）
    pub(super) fn apply_action(&mut self, action: toolbar::Action) {
        // 任何工具栏动作都收起取色面板（EDT-7，issue #73）；
        // 双击色块时第一击走这里（选用该色），第二击经 WM_LBUTTONDBLCLK 再展开面板
        self.close_color_panel();
        match action {
            toolbar::Action::Tool(t) => self.tool = t,
            toolbar::Action::Color(i) => {
                self.color_index = i;
                // EDT-7：颜色同时作用于选中的对象
                self.style_selected();
            }
            toolbar::Action::ToggleFill => {
                // 填充仅对矩形 / 椭圆有意义：文本、序号、直线、手绘无背景，忽略
                if fill_enabled_for(self.tool, self.selected_object()) {
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
    /// 二次确认是**同一键连按两次**（非模态提示，见 `cancel_hint`，issue #120），不再弹系统确认框；
    /// 设置里关掉「连按两次退出」后，有标注也直接退出。
    pub(super) fn on_cancel(&mut self, key: u32) {
        match cancel_step(
            self.color_panel_open(),
            self.font_list_open(),
            self.is_editing_text(),
            self.confirm_cancel && !self.doc.is_empty(),
        ) {
            CancelStep::ColorPanel => self.close_color_panel(),
            CancelStep::FontPanel => self.close_font_list(),
            // 文本 / 序号输入中右键 = 完成本次输入（保留文字），而非丢弃；
            // Esc 取消输入走 `on_text_key`（键盘路径），不经过这里。
            CancelStep::CommitText => self.commit_text(),
            // 有未保存标注：第一次弹出「再按一次」提示，同键在时间窗内再按才退出（issue #120）
            CancelStep::Confirm => {
                if self.arm_cancel(key) {
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
    /// 有未保存标注，进入二次确认（同键连按两次，见 `cancel_hint`）。
    Confirm,
    /// 直接退出。
    Exit,
}

/// 取消分层决策（纯函数，便于单测）：面板优先于输入，输入优先于退出；
/// 有标注时进入二次确认。
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

    fn obj(kind: Kind) -> Object {
        Object {
            kind,
            style: Style::new([255, 0, 0, 255], 4.0),
        }
    }

    /// 填充开关可用性（EDT-1/EDT-14）：当前工具或选中对象任一是矩形 / 椭圆即可用。
    /// 重点守住「工具是箭头、但选中的是矩形」——这里曾经只能看工具，导致按钮置灰、
    /// 选中的矩形改不了填充。
    #[test]
    fn fill_enabled_for_tool_or_selection() {
        use crate::editor::{Point, Tool};
        let rect = obj(Kind::Rect {
            a: Point::new(0.0, 0.0),
            b: Point::new(10.0, 10.0),
            radius: 0.0,
            filled: false,
        });
        let ellipse = obj(Kind::Ellipse {
            a: Point::new(0.0, 0.0),
            b: Point::new(10.0, 10.0),
            filled: false,
        });
        let text = obj(Kind::Text {
            pos: Point::new(0.0, 0.0),
            text: "a".into(),
            size: (8.0, 8.0),
            font_size: 16.0,
            font_family: "x".into(),
            bold: false,
            italic: false,
        });

        // 工具本身可填充
        assert!(fill_enabled_for(Tool::Rect, None));
        assert!(fill_enabled_for(Tool::Ellipse, None));
        // 无填充概念的工具（未选中任何对象）
        for t in [
            Tool::Arrow,
            Tool::Text,
            Tool::Highlight,
            Tool::Number,
            Tool::Line,
            Tool::Freehand,
        ] {
            assert!(!fill_enabled_for(t, None), "{t:?} 无填充概念");
        }
        // 工具不是矩形 / 椭圆，但选中了可填充对象：仍应可用（否则选中的对象没法改填充）
        assert!(fill_enabled_for(Tool::Arrow, Some(&rect)), "选中矩形时填充应可用");
        assert!(fill_enabled_for(Tool::Line, Some(&ellipse)), "选中椭圆时填充应可用");
        assert!(fill_enabled_for(Tool::Highlight, Some(&rect)));
        // 选中对象不可填充时仍返回不可用
        assert!(!fill_enabled_for(Tool::Arrow, Some(&text)));
    }
}
