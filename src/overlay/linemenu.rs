// SPDX-License-Identifier: GPL-3.0-only
//! 线型下拉的交互（EDT-18，issue #168）：展开 / 收起、点选换线型、悬停高亮。
//! 面板的布局、命中与绘制见 `overlay::toolbar::linemenu`；
//! 线型落地走 `annotate::apply_action` 的 `LineStyle` 分支（同时作用于选中对象）。

use crate::overlay::Overlay;
use crate::overlay::geometry::{expand_rect, union_rect};
use crate::overlay::toolbar::{self, linemenu::LineMenu};

impl Overlay {
    /// 展开 / 收起线型下拉（与取色面板、字体列表互斥）。
    pub(in crate::overlay) fn toggle_line_menu(&mut self) {
        let old = self.line_menu.as_ref().map(|m| m.rect());
        if self.line_menu.is_some() {
            self.line_menu = None;
        } else {
            self.close_color_panel();
            self.close_font_list();
            self.line_menu = self.open_line_menu();
        }
        // 面板旧 / 新区 + 工具栏：格子自己要跟着 `line_open` 变底色（EDT-18）
        let dirty = union_rect(old, self.line_menu.as_ref().map(|m| m.rect()));
        if let Some(dirty) = union_rect(dirty, self.bar.as_ref().map(|b| b.rect)) {
            self.repaint(expand_rect(dirty, 2));
        }
    }

    /// 从线型格子位置弹出下拉（格子不存在或缺工具栏时不展开）。
    fn open_line_menu(&self) -> Option<LineMenu> {
        let bar = self.bar.as_ref()?;
        let btn = bar
            .hits
            .iter()
            .find(|(_, a)| *a == toolbar::Action::LineMenu)
            .map(|(r, _)| *r)?;
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        Some(LineMenu::open(btn, bar.rect, vw, vh, self.ui))
    }

    /// 下拉上的左键：命中某项即切换线型并收起，点面板外则收起并退回常规流程。
    /// 返回 true 表示点击已被面板消费。
    pub(in crate::overlay) fn on_line_menu_down(&mut self, x: i32, y: i32) -> bool {
        let Some(panel) = &self.line_menu else {
            return false;
        };
        if !panel.contains(x, y) {
            self.toggle_line_menu();
            return false;
        }
        let hit = panel.hit(x, y);
        let rect = panel.rect();
        self.line_menu = None;
        if let Some(style) = hit {
            self.apply_action(toolbar::Action::LineStyle(style));
        }
        self.repaint(expand_rect(rect, 2));
        true
    }

    /// 下拉悬停高亮（仅悬停项变化时重绘）。
    pub(in crate::overlay) fn on_line_menu_hover(&mut self, x: i32, y: i32) {
        let Some(panel) = self.line_menu.as_mut() else {
            return;
        };
        if panel.hover_at(x, y) {
            let rect = panel.rect();
            self.repaint(expand_rect(rect, 2));
        }
    }

    /// 下拉是否展开（Esc 分流用）。
    pub(in crate::overlay) fn line_menu_open(&self) -> bool {
        self.line_menu.is_some()
    }

    /// 鼠标是否在下拉上（光标形状用）。
    pub(in crate::overlay) fn over_line_menu(&self, x: i32, y: i32) -> bool {
        self.line_menu.as_ref().is_some_and(|m| m.contains(x, y))
    }

    /// 收起下拉（工具栏其它按钮、Esc、框选变动）；旧面板与工具栏一并重绘（格子底色随 `line_open` 变）。
    pub(in crate::overlay) fn close_line_menu(&mut self) {
        if let Some(panel) = self.line_menu.take() {
            let dirty = union_rect(Some(panel.rect()), self.bar.as_ref().map(|b| b.rect));
            if let Some(dirty) = dirty {
                self.repaint(expand_rect(dirty, 2));
            }
        }
    }
}
