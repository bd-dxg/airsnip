// SPDX-License-Identifier: GPL-3.0-only
//! 徽标形态下拉的交互（EDT-20，issue #186）：展开 / 收起、点选换形态、悬停高亮。
//! 面板的布局、命中与绘制见 `overlay::toolbar::badgemenu`；
//! 形态落地走 `annotate::apply_action` 的 `BadgeMark` 分支（只影响下一次绘制）。

use crate::overlay::Overlay;
use crate::overlay::geometry::{expand_rect, union_rect};
use crate::overlay::toolbar::{self, badgemenu::BadgeMenu};

impl Overlay {
    /// 展开 / 收起徽标形态下拉（与取色面板、字体列表、线型下拉互斥）。
    pub(in crate::overlay) fn toggle_badge_menu(&mut self) {
        let old = self.badge_menu.as_ref().map(|m| m.rect());
        if self.badge_menu.is_some() {
            self.badge_menu = None;
        } else {
            self.close_color_panel();
            self.close_font_list();
            self.close_line_menu();
            self.badge_menu = self.open_badge_menu();
        }
        // 面板旧 / 新区 + 工具栏：格子自己要跟着展开态变底色（EDT-20）
        let dirty = union_rect(old, self.badge_menu.as_ref().map(|m| m.rect()));
        if let Some(dirty) = union_rect(dirty, self.bar.as_ref().map(|b| b.rect)) {
            self.repaint(expand_rect(dirty, 2));
        }
    }

    /// 从序号格子位置弹出下拉（格子不存在或缺工具栏时不展开）。
    fn open_badge_menu(&self) -> Option<BadgeMenu> {
        let bar = self.bar.as_ref()?;
        let btn = bar
            .hits
            .iter()
            .find(|(_, a)| *a == toolbar::Action::Tool(crate::editor::Tool::Number))
            .map(|(r, _)| *r)?;
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        Some(BadgeMenu::open(btn, bar.rect, vw, vh, self.ui))
    }

    /// 下拉上的左键：命中某项即切换形态并收起，点面板外则收起并退回常规流程。
    /// 返回 true 表示点击已被面板消费。
    pub(in crate::overlay) fn on_badge_menu_down(&mut self, x: i32, y: i32) -> bool {
        let Some(panel) = &self.badge_menu else {
            return false;
        };
        if !panel.contains(x, y) {
            self.toggle_badge_menu();
            return false;
        }
        let hit = panel.hit(x, y);
        let rect = panel.rect();
        self.badge_menu = None;
        if let Some(mark) = hit {
            self.apply_action(toolbar::Action::BadgeMark(mark));
        }
        self.repaint(expand_rect(rect, 2));
        true
    }

    /// 下拉悬停高亮（仅悬停项变化时重绘）。
    pub(in crate::overlay) fn on_badge_menu_hover(&mut self, x: i32, y: i32) {
        let Some(panel) = self.badge_menu.as_mut() else {
            return;
        };
        if panel.hover_at(x, y) {
            let rect = panel.rect();
            self.repaint(expand_rect(rect, 2));
        }
    }

    /// 下拉是否展开（Esc 分流用）。
    pub(in crate::overlay) fn badge_menu_open(&self) -> bool {
        self.badge_menu.is_some()
    }

    /// 鼠标是否在下拉上（光标形状用）。
    pub(in crate::overlay) fn over_badge_menu(&self, x: i32, y: i32) -> bool {
        self.badge_menu.as_ref().is_some_and(|m| m.contains(x, y))
    }

    /// 收起下拉（工具栏其它按钮、Esc、框选变动）；旧面板与工具栏一并重绘。
    pub(in crate::overlay) fn close_badge_menu(&mut self) {
        if let Some(panel) = self.badge_menu.take() {
            let dirty = union_rect(Some(panel.rect()), self.bar.as_ref().map(|b| b.rect));
            if let Some(dirty) = dirty {
                self.repaint(expand_rect(dirty, 2));
            }
        }
    }
}
