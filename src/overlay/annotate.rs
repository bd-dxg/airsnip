// SPDX-License-Identifier: GPL-3.0-only
//! 标注状态与输出（EDT-1/2/7、OUT-1/2/3）：当前工具与样式、工具栏动作、
//! 撤销/重做、复制与保存。

mod cancel;
mod output;
pub(super) mod style;

use cancel::{CancelStep, cancel_step};
pub(in crate::overlay) use style::{fill_enabled_for, gradient_enabled_for, has_stroke, line_enabled_for};

// 画布 ↔ 屏幕坐标换算：贴图 / 长截图 / 快速选区（ADR 0040）共用一份
pub(in crate::overlay) use output::to_screen;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

use super::{geometry::SelRect, toolbar};
use crate::editor::Style;

impl super::Overlay {
    /// 推进到下一套渐变（EDT-19）：**落笔成对象**与**切换工具**都调用它。
    /// 用「基准 + 计数」生成，不拿上一套终点当起点（否则经 u8 往返逐套漂、步长越走越小）。
    pub(super) fn advance_gradient(&mut self) {
        self.gradient_count = self.gradient_count.wrapping_add(1);
        self.gradient = crate::editor::Gradient::nth(self.gradient_base, self.gradient_count);
    }

    /// 当前标注色（EDT-7）：渐变色块时用渐变中点作代表色（取色面板 / 提示 / 徽标对比度）。
    pub(super) fn current_color(&self) -> [u8; 4] {
        if self.color_index >= crate::editor::GRADIENT_SWATCH {
            self.gradient.representative()
        } else {
            self.colors[self.color_index]
        }
    }

    /// 当前样式（EDT-7：颜色 + 连续线宽；EDT-18：线型；EDT-19：渐变）。
    /// 渐变的**推进不在这里**：只在草稿真的提交成对象时推进（见 `commit_draft`），
    /// 否则「点一下只选中对象」也会白吐一套（ADR 0023 的退化丢弃）。
    pub(super) fn current_style(&self) -> Style {
        let style = Style::new(self.current_color(), self.line_width).with_line(self.line_style);
        if self.color_index >= crate::editor::GRADIENT_SWATCH {
            style.with_gradient(self.gradient)
        } else {
            style
        }
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
        } else if outputs && vk == self.fullscreen_key {
            // ADR 0040：F 全屏（鼠标键 / PrtScn 绑上去时也走这里）
            self.on_select_fullscreen();
        } else if outputs && vk == self.last_area_key {
            self.on_select_last_area();
        } else if let Some(m) = self.mode_by_key(vk) {
            self.switch_mode(m);
        }
    }

    /// 切换交互模式（工具栏动作统一入口，使其重绘）。
    pub(super) fn switch_mode(&mut self, m: crate::editor::Mode) {
        let action = match m {
            crate::editor::Mode::Move => toolbar::Action::Move,
            crate::editor::Mode::Draw(t) => toolbar::Action::Tool(t),
        };
        self.apply_action(action);
    }

    /// 按下键对应的交互模式（移动键 / 工具切换键，ADR 0036）。
    /// 移动键优先匹配：它是「回到指针」，不应被工具键抢走。
    pub(super) fn mode_by_key(&self, vk: u32) -> Option<crate::editor::Mode> {
        if vk == self.tool_keys.move_key {
            return Some(crate::editor::Mode::Move);
        }
        self.tool_by_key(vk).map(crate::editor::Mode::Draw)
    }

    pub(super) fn bar_state(&self) -> toolbar::State {
        let (font_family, font_size, bold, italic) = self.font_state();
        let tool = self.mode.tool();
        toolbar::State {
            tool,
            color: self.color_index,
            colors: self.colors,
            gradient: self.gradient,
            gradient_enabled: gradient_enabled_for(tool, self.selected_object()),
            filled: self.filled,
            fill_enabled: fill_enabled_for(tool, self.selected_object()),
            line_style: self.line_style,
            line_enabled: line_enabled_for(tool, self.selected_object(), self.filled),
            line_open: self.line_menu.is_some(),
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
        // 任何工具栏动作都收起取色面板（EDT-7，issue #73）与线型下拉（EDT-18）；
        // 双击色块时第一击走这里（选用该色），第二击经 WM_LBUTTONDBLCLK 再展开面板。
        // 线型格子自己走 toggle，不能被这里先关掉（否则永远只能打开、无法收起）。
        self.close_color_panel();
        if action != toolbar::Action::LineMenu {
            self.close_line_menu();
        }
        match action {
            toolbar::Action::Tool(t) => {
                self.mode = crate::editor::Mode::Draw(t);
                // 换工具也换一套：用户口径「换成箭头时就应该改变颜色」
                if self.color_index >= crate::editor::GRADIENT_SWATCH {
                    self.advance_gradient();
                }
            }
            // ADR 0036：回到指针（移动选区）态；贴图编辑模式选区锁定，指针模式无意义，忽略
            toolbar::Action::Move if self.locked_selection => {}
            toolbar::Action::Move => self.mode = crate::editor::Mode::Move,
            toolbar::Action::Color(i) => {
                // EDT-19：渐变色块置灰时点了不生效（文本 / 荧光笔 / 效果类不支持渐变），
                // 不能把 `color_index` 悄悄改成渐变而对象毫无变化
                if i >= crate::editor::GRADIENT_SWATCH
                    && !gradient_enabled_for(self.mode.tool(), self.selected_object())
                {
                    return;
                }
                self.color_index = i;
                // 记住选中的色块（含渐变），下次截图沿用（EDT-19）
                crate::editor::color_index_store(i);
                // EDT-7：颜色（或渐变）同时作用于选中的对象
                self.style_selected();
            }
            toolbar::Action::ToggleFill => {
                // 填充仅对矩形 / 椭圆有意义：文本、序号、直线、手绘无背景，忽略
                if fill_enabled_for(self.mode.tool(), self.selected_object()) {
                    self.filled = !self.filled;
                    self.style_selected();
                }
            }
            // EDT-18：线型格子只是展开 / 收起下拉，取值在 `LineStyle` 分支（由下拉点击分发）
            toolbar::Action::LineMenu => return self.toggle_line_menu(),
            toolbar::Action::LineStyle(style) => {
                // 只有存在可见描边时才有意义（填充矩形 / 文本 / 效果类对象置灰，见 `line_enabled_for`）
                if line_enabled_for(self.mode.tool(), self.selected_object(), self.filled) {
                    self.line_style = style;
                    // 跨截图记忆（EDT-18）：主线程在 WM_OVERLAY_DONE 写回配置
                    crate::editor::line_store(style);
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
            // OUT-2：保存按钮 → 弹另存为对话框并关闭覆盖层（不复绘）
            toolbar::Action::Save => return self.on_save(),
            // OUT-2 / ADR 0041：快速保存按钮 → 按模板路径直接落盘（不复绘）
            toolbar::Action::QuickSave => return self.on_quick_save(),
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
        // 快速保存在途（OUT-2 / ADR 0041）：编码在后台线程里跑，这时关掉覆盖层只会让
        // 文件照旧落盘、回包没人接——「我取消了，但文件还是出现了」。直接不响应，
        // 几十毫秒后就出结果（成功 → 提示或直接收尾；失败 → 错误框）。
        if self.quick.pending {
            return;
        }
        match cancel_step(
            self.color_panel_open(),
            self.line_menu_open(),
            self.font_list_open(),
            self.is_editing_text(),
            self.confirm_cancel && !self.doc.is_empty(),
        ) {
            CancelStep::ColorPanel => self.close_color_panel(),
            CancelStep::LineMenu => self.close_line_menu(),
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

/// 取消截图的分层步骤与决策见 `cancel`（OUT-3 / ADR 0027）。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Kind, Object};

    /// 渐变可用性（EDT-19）：移动态无选中时**仍可用**（只是设定下次绘制的样式），
    /// 只有当前工具 / 选中对象本身不支持时才灰。
    #[test]
    fn gradient_available_in_move_mode() {
        use crate::editor::{Point, Tool};
        let text = obj(Kind::Text {
            pos: Point::new(0.0, 0.0),
            text: "a".into(),
            size: (8.0, 8.0),
            font_size: 16.0,
            font_family: "x".into(),
            bold: false,
            italic: false,
        });
        assert!(gradient_enabled_for(None, None), "移动态无选中时渐变色块应彩色可用");
        assert!(gradient_enabled_for(Some(Tool::Rect), None));
        assert!(!gradient_enabled_for(Some(Tool::Text), None), "文本不支持渐变");
        assert!(!gradient_enabled_for(None, Some(&text)));
    }

    /// 线型可用性（EDT-18）：有可见描边才能切换——填充矩形 / 椭圆、文本、效果类都不行。
    #[test]
    fn line_enabled_needs_visible_stroke() {
        use crate::editor::{Point, Tool};
        let mk = |filled: bool| {
            obj(Kind::Rect {
                a: Point::new(0.0, 0.0),
                b: Point::new(10.0, 10.0),
                radius: 0.0,
                filled,
            })
        };
        let text = obj(Kind::Text {
            pos: Point::new(0.0, 0.0),
            text: "a".into(),
            size: (8.0, 8.0),
            font_size: 16.0,
            font_family: "x".into(),
            bold: false,
            italic: false,
        });
        let arrow = obj(Kind::arrow(Point::new(0.0, 0.0), Point::new(10.0, 10.0)));

        // 工具本身会画出描边
        for t in [
            Tool::Rect,
            Tool::Arrow,
            Tool::Line,
            Tool::Ellipse,
            Tool::Freehand,
            Tool::Number,
        ] {
            assert!(line_enabled_for(Some(t), None, false), "{t:?} 应有描边");
        }
        // 无描边工具：置灰
        for t in [Tool::Text, Tool::Highlight, Tool::Blur, Tool::Glow] {
            assert!(!line_enabled_for(Some(t), None, false), "{t:?} 无描边");
        }
        // 填充的矩形 / 椭圆不描边，工具开着填充时线型无处可见
        assert!(!line_enabled_for(Some(Tool::Rect), None, true));
        assert!(!line_enabled_for(Some(Tool::Ellipse), None, true));
        assert!(!line_enabled_for(None, None, false), "移动态无选中对象时置灰");
        // 选中对象自己有没有描边（与当前工具无关）
        assert!(line_enabled_for(Some(Tool::Arrow), Some(&mk(false)), false));
        assert!(
            !line_enabled_for(Some(Tool::Arrow), Some(&mk(true)), false),
            "填充矩形无描边"
        );
        assert!(!line_enabled_for(None, Some(&text), false));
        assert!(line_enabled_for(None, Some(&arrow), false));
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
        assert!(fill_enabled_for(Some(Tool::Rect), None));
        assert!(fill_enabled_for(Some(Tool::Ellipse), None));
        // 无填充概念的工具（未选中任何对象）
        for t in [
            Tool::Arrow,
            Tool::Text,
            Tool::Highlight,
            Tool::Number,
            Tool::Line,
            Tool::Freehand,
        ] {
            assert!(!fill_enabled_for(Some(t), None), "{t:?} 无填充概念");
        }
        // 移动态（ADR 0036）没有待用工具：空白处一律不可用
        assert!(!fill_enabled_for(None, None), "移动态下无选中对象时填充应置灰");
        // 工具不是矩形 / 椭圆，但选中了可填充对象：仍应可用（否则选中的对象没法改填充）
        assert!(fill_enabled_for(Some(Tool::Arrow), Some(&rect)), "选中矩形时填充应可用");
        assert!(
            fill_enabled_for(Some(Tool::Line), Some(&ellipse)),
            "选中椭圆时填充应可用"
        );
        assert!(fill_enabled_for(Some(Tool::Highlight), Some(&rect)));
        assert!(fill_enabled_for(None, Some(&ellipse)), "移动态下选中的椭圆仍应能改填充");
        // 选中对象不可填充时仍返回不可用
        assert!(!fill_enabled_for(Some(Tool::Arrow), Some(&text)));
    }
}
