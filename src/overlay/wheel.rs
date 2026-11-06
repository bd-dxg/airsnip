// SPDX-License-Identifier: GPL-3.0-only
//! 滚轮调整（EDT-4/EDT-5/EDT-7）：悬停对象上时连续调整该对象的线宽 / 模糊半径 / 字号，
//! 悬停空白时只改下一次绘制的默认值（`Ctrl` 3 倍步进）。
//! 命中用整块命中（`pick::hit_object`）：与「画还是拖」无关，不参与 ADR 0023 的通道分流。
//! 从 `pick` 拆出：那里是命中测试，这里是属性调整。

use crate::editor::Kind;

use super::geometry::{expand_rect, union_rect};

impl super::Overlay {
    /// 鼠标滚轮（EDT-7/EDT-4）：悬停在线条/矩形边框 / 模糊区域 / 文本上时，
    /// 滚轮连续调整线宽、模糊半径或字号（Ctrl 3 倍步进）；无悬停对象时调整当前工具默认值。
    /// `sx`/`sy` 为屏幕坐标，需减虚拟屏幕原点转覆盖层坐标。
    pub(super) fn on_mouse_wheel(&mut self, sx: i32, sy: i32, delta: i16, ctrl: bool) {
        let (x, y) = (sx - self.capture.origin_x, sy - self.capture.origin_y);
        let step = if ctrl { 3.0 } else { 1.0 } * if delta > 0 { 1.0 } else { -1.0 };
        // 文本输入中：滚轮调整输入字号（EDT-4）
        if let Some(edit) = self.text_edit.as_mut() {
            edit.font_size = (edit.font_size + step).clamp(10.0, 96.0);
            self.font_size = edit.font_size;
            self.repaint_text_edit();
            return;
        }
        if let Some(i) = self.hit_object(x, y) {
            self.adjust_object(i, step);
        } else {
            // 空白处滚动：只改默认值，供下一次绘制沿用
            match self.tool {
                crate::editor::Tool::Rect | crate::editor::Tool::Arrow => {
                    self.line_width = (self.line_width + step).clamp(1.0, 32.0);
                }
                crate::editor::Tool::Text => {
                    self.font_size = (self.font_size + step).clamp(10.0, 96.0);
                }
                crate::editor::Tool::Blur => self.blur_radius = (self.blur_radius + step).clamp(4.0, 64.0),
                crate::editor::Tool::Highlight | crate::editor::Tool::Glow => {}
            }
        }
    }

    /// 调整指定对象的线宽 / 模糊半径 / 字号（差分入撤销栈），并同步全局默认值。
    fn adjust_object(&mut self, i: usize, step: f32) {
        let before = self.selected_clip();
        self.doc.begin();
        let changed = {
            let obj = &mut self.doc.objects_mut()[i];
            let changed = match &mut obj.kind {
                Kind::Rect { .. } | Kind::Arrow { .. } => {
                    let w = (obj.style.width + step).clamp(1.0, 32.0);
                    let changed = (w - obj.style.width).abs() > 0.01;
                    if changed {
                        obj.style.width = w;
                        self.line_width = w;
                    }
                    changed
                }
                Kind::Blur { radius, .. } => {
                    let r = (*radius + step).clamp(4.0, 64.0);
                    let changed = (r - *radius).abs() > 0.01;
                    if changed {
                        *radius = r;
                        self.blur_radius = r;
                    }
                    changed
                }
                // 高亮/聚光：无可调属性
                Kind::Highlight { .. } | Kind::Glow { .. } => false,
                // 文本：字号连续可调，同步重建测量缓存（EDT-4）
                Kind::Text { font_size, .. } => {
                    let fs =
                        (*font_size + step).clamp(crate::editor::FONT_SIZE_RANGE.0, crate::editor::FONT_SIZE_RANGE.1);
                    let changed = (*font_size - fs).abs() > 0.01;
                    if changed {
                        *font_size = fs;
                        self.font_size = fs;
                    }
                    changed
                }
            };
            // 非文本对象与未变字号下 remeasure 无操作
            if changed {
                crate::render::text::remeasure(&mut obj.kind);
            }
            changed
        };
        self.doc.commit(changed);
        if !changed {
            return;
        }
        let after = self.selected_clip();
        let dirty = union_rect(before, after);
        if let Some(d) = dirty {
            self.repaint(expand_rect(d, 4));
        }
        if let Some(bar) = &self.bar {
            // 撤销可用性与字号显示可能变化
            self.repaint(expand_rect(bar.rect, 2));
        }
    }
}
