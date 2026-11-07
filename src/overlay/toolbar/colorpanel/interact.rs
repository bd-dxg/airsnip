// SPDX-License-Identifier: GPL-3.0-only
//! 自定义颜色面板的交互（EDT-7，issue #73）：展开 / 收起、取样拖动、快捷取色、悬停高亮。
//! 布局与命中见 `super`，绘制见 `paint`；颜色变更最终经 `style_selected` 落到选中对象。

use crate::editor::{self, CUSTOM_INDEX, Kind};
use crate::overlay::Overlay;
use crate::overlay::geometry::{expand_rect, union_rect};
use crate::overlay::toolbar::Action;

use super::{ColorPanel, Hit};

impl Overlay {
    /// 展开 / 收起取色面板（工具栏「自定义颜色」色块）。与字体面板互斥。
    pub(in crate::overlay) fn toggle_color_panel(&mut self) {
        if self.color_panel.is_some() {
            self.close_color_panel();
            return;
        }
        self.close_font_list();
        let Some(bar) = self.bar.as_ref() else {
            return;
        };
        let Some(anchor) = bar
            .hits
            .iter()
            .find(|(_, a)| *a == Action::Color(CUSTOM_INDEX))
            .map(|(r, _)| *r)
        else {
            return;
        };
        let (bar_rect, color, ui) = (bar.rect, self.custom_color, self.ui);
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        self.color_panel = Some(ColorPanel::open(anchor, bar_rect, color, vw, vh, ui));
        self.repaint_panel();
    }

    /// 收起取色面板（工具栏其它按钮、Esc、点击面板之外）。
    pub(in crate::overlay) fn close_color_panel(&mut self) {
        if let Some(panel) = self.color_panel.take() {
            self.repaint(expand_rect(panel.rect, 2));
        }
    }

    /// 取色面板是否展开（Esc 分流用）。
    pub(in crate::overlay) fn color_panel_open(&self) -> bool {
        self.color_panel.is_some()
    }

    /// 鼠标是否在取色面板上（光标形状用）。
    pub(in crate::overlay) fn over_color_panel(&self, x: i32, y: i32) -> bool {
        self.color_panel.as_ref().is_some_and(|p| p.contains(x, y))
    }

    /// 面板上的左键：命中即开始取样 / 取快捷色；面板内空白消费但不改状态；
    /// 面板外则收起并把点击交回常规流程。返回 true 表示点击已被面板消费。
    pub(in crate::overlay) fn on_color_panel_down(&mut self, x: i32, y: i32) -> bool {
        let Some(panel) = self.color_panel.as_ref() else {
            return false;
        };
        if !panel.contains(x, y) {
            self.close_color_panel();
            return false;
        }
        // 面板内空白（内边距）：消费点击，但不切换当前色（避免误触改状态）
        let Some(hit) = panel.hit(x, y) else {
            return true;
        };
        let slider = matches!(hit, Hit::Hue | Hit::Sv);
        if let Some(panel) = self.color_panel.as_mut() {
            panel.press(hit, x, y);
        }
        if let Some(panel) = self.color_panel.as_ref() {
            self.custom_color = panel.color();
            self.color_index = CUSTOM_INDEX;
        }
        if slider {
            // 拖动滑块：开一次撤销快照，拖动中实时预览选中对象，松手归档
            self.begin_color_edit();
            self.preview_color(self.custom_color);
            self.repaint_panel();
        } else {
            // 快捷色块：单击成型，直接进撤销栈
            self.commit_custom_color();
            self.repaint_toolbar_chrome();
        }
        true
    }

    /// 面板上的鼠标移动：拖动取样继续（鼠标移出面板也不断），否则更新快捷色块悬停。
    /// 返回 true 表示移动已被面板消费（不再走绘制流程）。
    pub(in crate::overlay) fn on_color_panel_move(&mut self, x: i32, y: i32) -> bool {
        if self.color_panel.is_none() {
            return false;
        }
        if self.color_panel.as_ref().is_some_and(|p| p.dragging()) {
            let before = self.color_panel.as_ref().map(|p| p.hsv());
            if let Some(panel) = self.color_panel.as_mut() {
                panel.drag_to(x, y);
            }
            let Some(after) = self.color_panel.as_ref().map(|p| p.hsv()) else {
                return true;
            };
            // HSV 未变（同一像素内移动）：跳过重绘，避免高频鼠标事件空跑。
            // 注意看 HSV 而非结果 RGB：灰度区拖色相条时颜色不变，但取样标记仍应移动。
            if Some(after) == before {
                return true;
            }
            if let Some(panel) = self.color_panel.as_ref() {
                self.custom_color = panel.color();
                self.color_index = CUSTOM_INDEX;
            }
            let color = self.custom_color;
            // 选中对象实时跟随；仅面板区域重绘（不重画整条工具栏，避免拖动卡顿）
            self.preview_color(color);
            self.repaint_panel();
            return true;
        }
        if !self.color_panel.as_ref().is_some_and(|p| p.contains(x, y)) {
            return false;
        }
        if self.color_panel.as_mut().is_some_and(|p| p.hover_at(x, y)) {
            self.repaint_panel();
        }
        true
    }

    /// 松开左键：结束取样、提交撤销、写回配置并更新工具栏色块。返回是否消费。
    pub(in crate::overlay) fn on_color_panel_up(&mut self) -> bool {
        let dragging = self.color_panel.as_ref().is_some_and(|p| p.dragging());
        if !dragging && !self.color_edit {
            return false;
        }
        if let Some(panel) = self.color_panel.as_mut() {
            panel.end_drag();
        }
        editor::custom_color_store(self.custom_color);
        self.finish_color_edit();
        // 拖动期间未更新工具栏色块：松手时补齐
        self.repaint_toolbar_chrome();
        true
    }

    /// 快捷色块：写回配置并把当前色作用于选中对象（走常规撤销路径）。
    fn commit_custom_color(&mut self) {
        editor::custom_color_store(self.custom_color);
        self.style_selected();
    }

    /// 开始一次取色拖动：若有选中对象，先开撤销快照（松手时归档为一步）。
    fn begin_color_edit(&mut self) {
        self.color_edit_before = self.selected.map(|i| self.doc.objects()[i].style.color);
        self.color_edit = self.color_edit_before.is_some();
        if self.color_edit {
            self.doc.begin();
        }
    }

    /// 拖动中实时更新选中对象的颜色（不写历史，仅重绘其脏区）；无选中则无操作。
    fn preview_color(&mut self, color: [u8; 4]) {
        let Some(i) = self.selected else {
            return;
        };
        // 效果类（模糊 / 聚光）不画描边，颜色无意义（与 `style_selected` 一致）
        if matches!(self.doc.objects()[i].kind, Kind::Blur { .. } | Kind::Glow { .. }) {
            return;
        }
        let before = self.selected_clip();
        {
            let obj = &mut self.doc.objects_mut()[i];
            if obj.style.color[..3] != color[..3] {
                obj.style.color = [color[0], color[1], color[2], obj.style.color[3]];
            }
        }
        if let Some(d) = union_rect(before, self.selected_clip()) {
            self.repaint(expand_rect(d, 10));
        }
    }

    /// 结束取色拖动：把 `begin_color_edit` 的快照按“是否真的变色”归档为一步撤销。
    fn finish_color_edit(&mut self) {
        if !self.color_edit {
            return;
        }
        let changed = match (self.selected, self.color_edit_before) {
            (Some(i), Some(before)) => self.doc.objects()[i].style.color != before,
            _ => false,
        };
        self.doc.commit(changed);
        self.color_edit = false;
        self.color_edit_before = None;
    }

    /// 重绘取色面板区域。
    fn repaint_panel(&mut self) {
        let rect = self.color_panel.as_ref().map(|p| p.rect);
        if let Some(r) = rect {
            self.repaint(expand_rect(r, 2));
        }
    }
}
