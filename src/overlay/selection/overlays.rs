// SPDX-License-Identifier: GPL-3.0-only
//! 屏幕显示层的「悬浮元素」（EDT-10 提示气泡 / OUT-3 二次确认提示 / OUT-2 快速保存轻提示）：
//! 三者都不进导出烘焙、都画在标注与工具栏之上。从 `selection` 拆出（ADR 0041 把那边撑过 300 行）。

use crate::overlay::geometry::{SelRect, expand_rect, intersect_rect};

use super::super::{Overlay, cancel_hint, toast, toolbar};

impl Overlay {
    /// 按脏区绘制全部悬浮元素（谁可见画谁；都不在脏区内时什么也不做）。
    pub(super) fn draw_floaters(&mut self, region: SelRect) {
        // 7) 悬浮提示气泡（EDT-10）：屏幕显示层、画在最上层，不进导出烘焙
        if let Some(bubble) = &self.tip.bubble
            && intersect_rect(expand_rect(bubble.rect, 2), region).is_some()
        {
            toolbar::tip::draw(&mut self.display, bubble, self.palette, self.ui);
        }
        // 7b) 取消二次确认提示（OUT-3 / issue #120）：同样只在屏幕显示层
        if let Some(hint) = &self.cancel_hint
            && intersect_rect(expand_rect(hint.rect, 2), region).is_some()
        {
            cancel_hint::draw(&mut self.display, hint, self.ui);
        }
        // 7c) 快速保存提示（OUT-2 / ADR 0041）：屏幕显示层、画在最上层
        if let Some(t) = &self.quick.toast
            && intersect_rect(expand_rect(t.rect, 2), region).is_some()
        {
            toast::draw(&mut self.display, t, self.ui);
        }
    }
}
