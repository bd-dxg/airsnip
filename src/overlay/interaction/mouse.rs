// SPDX-License-Identifier: GPL-3.0-only
//! 鼠标交互接线（CAP-2、EDT-1/2/3/7）：按下 / 移动 / 松开的命中分发与拖动状态推进。
//! 从 `interaction` 拆出，控制单文件规模；阶段与拖动上下文类型见 `super`，
//! 命中判定见 `pick`，草稿绘制见 `draft`。

use crate::overlay::geometry::SelRect;
use crate::overlay::objedit::ObjHandle;
use crate::overlay::pick::{Handle, PickTarget};

use super::{CLICK_SLOP, Drag, PendingClick, Phase};

impl super::super::Overlay {
    pub(in crate::overlay) fn on_lbutton_down(&mut self, x: i32, y: i32) {
        // 工具栏优先（EDT-7）：点在工具栏上只切换状态，不开始绘制
        let bar = self.bar.clone();
        if let Some(bar) = &bar {
            if crate::overlay::geometry::in_rect(bar.rect, x, y) {
                if let Some(action) = bar.hit(x, y) {
                    self.apply_action(action);
                }
                return;
            }
        }
        // 取色面板（EDT-7，issue #73）：不覆盖工具栏，排在工具栏之后；
        // 命中即取样，点面板之外则收起并退回常规流程
        if self.on_color_panel_down(x, y) {
            return;
        }
        // 字体面板（EDT-4）：不覆盖工具栏，故排在工具栏之后；命中行选字体，
        // 点面板之外则收起面板并退回常规流程
        if self.on_font_list_down(x, y) {
            return;
        }
        // 文本输入中：任何左键先提交当前输入（EDT-4），再按常规流程（Text 工具点击空白会开启新输入）
        if self.text_edit.is_some() {
            self.commit_text();
        }
        self.down_started_draw = false;
        let target = self.pick_target(x, y, crate::overlay::wndproc::ctrl_down());
        match target {
            PickTarget::ObjHandle(i, h) => self.begin_obj_drag(i, h, x, y),
            PickTarget::CurveCtrl(index) => self.begin_control_drag(index),
            PickTarget::SelHandle(h) => {
                self.drag = Some(Drag {
                    handle: h,
                    orig: sel_of(self.selection),
                    px: x,
                    py: y,
                });
                self.phase = Phase::Adjusting;
            }
            // 平移整个选区：按住空格拖（CAP-2），或移动态下直接拖选区内空白（ADR 0036）。
            // `down_started_draw` 也在这里置位：「双击空白完成截图」的判据（M3）靠它
            // 区分「双击空白」与「双击对象」，与绘制态的行为一致。
            PickTarget::MoveSelection => {
                self.down_started_draw = true;
                self.drag = Some(Drag {
                    handle: Handle::Move,
                    orig: sel_of(self.selection),
                    px: x,
                    py: y,
                });
                self.phase = Phase::Adjusting;
            }
            // 移动对象：无位移的松手退化为单击选中（不进撤销栈）
            PickTarget::ObjBody(i) => self.begin_obj_drag(i, ObjHandle::Move, x, y),
            // 绘制优先（ADR 0023）：对象内部按下不立即建草稿——
            // 聚光灯按下即暗化全选区，按下就建草稿会闪一下；也不在此改选中，
            // 否则按下瞬间旧手柄会跳一下。改选中与起笔都推到“确实要画”之后。
            PickTarget::Inside(i) => {
                // 文本工具是「点击即放置」（EDT-4），不能延迟：否则对象内部永远放不下文字。
                // 序号标记（EDT-12）不在此列：对象内部按下走常规「绘制优先」（ADR 0023），
                // 否则双击改编号时第一次按下会先落下一个多余徽标。
                if self.mode.tool() == Some(crate::editor::Tool::Text) {
                    self.set_selected(None);
                    self.start_draw(x, y);
                } else {
                    self.pending_click = Some(PendingClick { x, y, index: i });
                }
            }
            // 空白处按下就是绘制（含文本工具点击放置）：立即置位，
            // `down_started_draw` 兼作「双击完成截图」的判据（M3）
            PickTarget::InsideEmpty => {
                self.down_started_draw = true;
                self.set_selected(None);
                self.start_draw(x, y);
            }
            // 贴图编辑模式（ADR 0028）：选区锁定，点选区外不改选区
            PickTarget::None if self.locked_selection => {}
            PickTarget::None => {
                self.set_selected(None);
                let sel = self.begin_select(x, y);
                self.set_selection(Some(sel));
            }
        }
    }

    pub(in crate::overlay) fn on_mouse_move(&mut self, x: i32, y: i32) {
        // 取色面板拖动 / 悬停（EDT-7，issue #73）：覆盖面板期间不改动选区与草稿
        if self.on_color_panel_move(x, y) {
            return;
        }
        // Shift 约束（EDT-1/EDT-2/EDT-7）：新建与二次编辑共用同一状态
        let shift = crate::overlay::wndproc::shift_down();
        // 对象内部按下后拖出阈值：转为绘制新标注（ADR 0023）
        if let Some(p) = self.pending_click {
            if (x - p.x).abs() > CLICK_SLOP || (y - p.y).abs() > CLICK_SLOP {
                self.pending_click = None;
                self.begin_pending_draw(p, x, y);
            }
            return;
        }
        // 拖动选中对象（EDT-7）
        if self.obj_drag.is_some() {
            self.drag_obj(x, y, shift);
            return;
        }
        // 拖动曲线控制柄（EDT-3）
        if self.ctrl_drag.is_some() {
            self.drag_control(x, y);
            return;
        }
        // 绘制中：草稿终点跟随鼠标（限制在选区内）；Shift 施加正方形/方向约束（EDT-1/EDT-2）
        if self.draft.is_some() {
            self.update_draft(x, y, shift);
            return;
        }
        let Some(drag) = &self.drag else {
            return;
        };
        let (dx, dy) = (x - drag.px, y - drag.py);
        let handle = drag.handle;
        let orig = drag.orig;
        let phase = self.phase;
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let max_x = vw.saturating_sub(1);
        let max_y = vh.saturating_sub(1);
        let x = x.clamp(0, max_x);
        let y = y.clamp(0, max_y);

        let new = if phase == Phase::Selecting {
            // 框选阶段：以按下点为锚点拉伸矩形
            SelRect::normalized(orig.x, orig.y, x, y)
        } else {
            match handle {
                Handle::Move => {
                    let lim_x = (max_x - orig.w).max(0);
                    let lim_y = (max_y - orig.h).max(0);
                    SelRect {
                        x: (orig.x + dx).clamp(0, lim_x),
                        y: (orig.y + dy).clamp(0, lim_y),
                        w: orig.w,
                        h: orig.h,
                    }
                }
                Handle::N => SelRect::normalized(orig.x, y, orig.x + orig.w, orig.y + orig.h),
                Handle::S => SelRect {
                    x: orig.x,
                    y: orig.y,
                    w: orig.w,
                    h: (y - orig.y).max(1),
                },
                Handle::W => SelRect::normalized(x, orig.y, orig.x + orig.w, orig.y + orig.h),
                Handle::E => SelRect {
                    x: orig.x,
                    y: orig.y,
                    w: (x - orig.x).max(1),
                    h: orig.h,
                },
                Handle::NE => SelRect::normalized(orig.x, y, orig.x + orig.w, orig.y + orig.h),
                Handle::NW => SelRect::normalized(x, y, orig.x + orig.w, orig.y + orig.h),
                Handle::SE => SelRect::normalized(orig.x, orig.y, x.max(orig.x + 1), y.max(orig.y + 1)),
                Handle::SW => SelRect::normalized(x, orig.y, orig.x + orig.w, orig.y + orig.h),
            }
        };
        self.set_selection(Some(new));
    }

    pub(in crate::overlay) fn on_lbutton_up(&mut self) {
        // 取色面板取样结束（EDT-7，issue #73）：提交并消费
        if self.on_color_panel_up() {
            return;
        }
        // 对象内部「单击」（未拖出阈值）：退化为选中，不画任何东西（ADR 0023 §3）
        if let Some(p) = self.pending_click.take() {
            self.set_selected(Some(p.index));
            return;
        }
        // 结束对象拖动（EDT-7）：无位移视为单击（仅选中），不进撤销栈
        if self.obj_drag.is_some() {
            self.end_obj_drag();
            return;
        }
        // 结束控制柄拖动（EDT-3）
        if self.ctrl_drag.is_some() {
            self.end_control_drag();
            return;
        }
        // 结束绘制：过短视为误触，丢弃；提交后进入撤销栈
        if self.draft.is_some() {
            self.commit_draft();
            return;
        }
        self.drag = None;
        let new = match self.selection {
            Some(sel) if sel.w < 2 || sel.h < 2 => None, // 误触，放弃
            other => {
                if other.is_some() {
                    self.phase = Phase::Adjusting;
                }
                other
            }
        };
        self.set_selection(new);
    }

    /// 双击（EDT-3 / EDT-4 / M3）：命中对象 → 编辑文本 / 恢复直线；命中色块 → 改色面板；
    /// 否则选区空白内双击 = 完成截图（复制，替代回车）。
    pub(in crate::overlay) fn on_lbutton_dblclick(&mut self, x: i32, y: i32) {
        // 双击也是用户新动作，解除取消待命（OUT-3 / issue #120）
        self.clear_cancel_arm();
        // 双击色块：展开取色面板改写该色块（EDT-7，issue #73 后续）；第一击已选用该色
        if self.on_swatch_dblclick(x, y) {
            return;
        }
        let edited = self
            .hit_object(x, y)
            .map(|i| self.begin_edit_object(i))
            .unwrap_or(false);
        let handled = if edited { true } else { self.reset_curve_at(x, y) };
        if handled {
            return;
        }
        // 双击第一击在选区内空白处按下（start_draw），作废草稿并完成截图
        if let Some(sel) = self.selection
            && sel.contains(x, y)
            && self.down_started_draw
        {
            self.draft = None;
            self.finish_output();
        }
    }

    /// 开始新的框选（返回初始 1px 选区）。
    fn begin_select(&mut self, x: i32, y: i32) -> SelRect {
        let sel = SelRect { x, y, w: 1, h: 1 };
        self.phase = Phase::Selecting;
        self.drag = Some(Drag {
            handle: Handle::Move,
            orig: sel,
            px: x,
            py: y,
        });
        sel
    }

    /// 对象内部按下后拖出阈值：在按下点起笔，再按当前鼠标位置走一帧草稿（ADR 0023）。
    /// 注意不能设 `down_started_draw`：它兼作「双击选区空白完成截图」的判据，
    /// 在这里置位会让“双击对象”误触发完成截图。
    fn begin_pending_draw(&mut self, p: PendingClick, x: i32, y: i32) {
        self.set_selected(None);
        self.start_draw(p.x, p.y);
        self.on_mouse_move(x, y);
    }
}

/// 选区存在性已在 `pick_target` 里保证，这里取默认值只为避免 `Option` 字段传染。
fn sel_of(sel: Option<SelRect>) -> SelRect {
    sel.unwrap_or(SelRect { x: 0, y: 0, w: 0, h: 0 })
}
