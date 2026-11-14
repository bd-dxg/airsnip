// SPDX-License-Identifier: GPL-3.0-only
//! 绘制草稿（EDT-1/2/3）：在选区内拖拽时的临时对象与提交。
//! 文本工具不产生草稿（走输入状态机，见 `textinput`）。

use crate::editor::{Kind, Object, Point, Tool};
use crate::render;

use super::super::geometry::{SelRect, expand_rect, union_rect};
use super::super::pick::is_degenerate;

/// 手绘采样最小间距（物理像素，EDT-15）：相邻点距离超过它才追加，
/// 避免同一像素内高频率移动产生海量点。
const FREEHAND_MIN_STEP: f32 = 2.0;

/// 手绘增量脏区（EDT-15）：只覆盖「上一采样点 → 新采样点」这一段，外扩半个线宽。
/// 笔画只追加，已画好的段不会变，无需每帧重绘整笔包围盒。
/// 取整与 `render::bounds_to_clip` 一致（floor / ceil 后再多 1px）。
fn segment_region(a: Point, b: Point, width: f32) -> SelRect {
    let pad = width * 0.5 + 1.0;
    let x0 = (a.x.min(b.x) - pad).floor() as i32;
    let y0 = (a.y.min(b.y) - pad).floor() as i32;
    let x1 = (a.x.max(b.x) + pad).ceil() as i32 + 1;
    let y1 = (a.y.max(b.y) + pad).ceil() as i32 + 1;
    SelRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

impl super::super::Overlay {
    /// 在选区内开始拖拽绘制标注（EDT-1/EDT-2）；文本工具转为点击放置（EDT-4）。
    /// 移动态（ADR 0036）不可绘制：调用方已按模式分流，这里再守一道，免得丢工具后默默画错东西。
    pub(super) fn start_draw(&mut self, x: i32, y: i32) {
        let p = Point::new(x as f32, y as f32);
        let Some(tool) = self.mode.tool() else {
            return;
        };
        let kind = match tool {
            Tool::Text => {
                self.begin_text_input(x, y);
                return;
            }
            Tool::Rect => Kind::Rect {
                a: p,
                b: p,
                // 圆角不再由工具栏开关控制：新建为直角，选中后拖内侧角的控制柄调（EDT-1）
                radius: 0.0,
                filled: self.filled,
            },
            Tool::Arrow => Kind::arrow(p, p),
            Tool::Highlight => Kind::Highlight {
                a: p,
                b: p,
                // 新建默认直角，选中后拖内侧角手柄调（EDT-1 同一套）
                corner: 0.0,
            },
            Tool::Blur => Kind::Blur {
                a: p,
                b: p,
                radius: self.blur_radius,
                corner: 0.0,
            },
            Tool::Glow => Kind::Glow {
                a: p,
                b: p,
                corner: 0.0,
            },
            // EDT-12 序号标记：按下即在 p 处放徽标（tip 与圆心同点，拖动时才出现引线箭头）
            Tool::Number => {
                let n = crate::editor::next_number(self.doc.objects());
                let font_size = self.number_font_size;
                Kind::Number {
                    center: p,
                    tip: p,
                    n,
                    font_size,
                    radius: render::number_radius(font_size, n),
                }
            }
            // EDT-13 直线：两端点同点，拖动时终点跟随
            Tool::Line => Kind::Line { from: p, to: p },
            // EDT-14 椭圆：外接矩形对角同点，拖动时更新 b；填充跟当前开关
            Tool::Ellipse => Kind::Ellipse {
                a: p,
                b: p,
                filled: self.filled,
            },
            // EDT-15 手绘：从按下点开始采样
            Tool::Freehand => Kind::Freehand {
                pts: std::sync::Arc::new(vec![p]),
            },
        };
        self.doc.begin();
        self.draft = Some(Object {
            kind,
            style: self.current_style(),
        });
        // 按下即需立即重绘的情形：
        // 聚光灯——进入暗化态（选区其余区域变暗、本区域亮）；
        // 序号标记——徽标应随按下立刻出现，不等鼠标移动。
        if matches!(tool, Tool::Glow | Tool::Number)
            && let Some(sel) = self.selection
        {
            self.repaint(sel);
        }
    }

    /// 拖动中：草稿终点跟随鼠标（限制在选区内）并重绘。
    /// `shift` 为真时施加绘制约束：矩形 → 正方形（EDT-1），箭头 → 水平 / 垂直 / 45° 吸附（EDT-2）。
    pub(super) fn update_draft(&mut self, x: i32, y: i32, shift: bool) {
        let Some(sel) = self.selection else {
            return;
        };
        let cx = x.clamp(sel.x, (sel.x + sel.w - 1).max(sel.x));
        let cy = y.clamp(sel.y, (sel.y + sel.h - 1).max(sel.y));
        let end = self.constrained_end(cx, cy, shift, sel);
        let before = self.draft_region();
        // 手绘追加段（起点, 终点）：只重绘这一段，不重绘整笔包围盒（见下方 `dirty`）
        let mut appended: Option<(Point, Point)> = None;
        if let Some(draft) = self.draft.as_mut() {
            // 手绘：点列可能很长，**原地追加**避免每帧整列克隆（交互帧路径，EDT-15）
            if let Kind::Freehand { pts } = &mut draft.kind {
                let last = pts.last().copied();
                let append = match last {
                    Some(l) => {
                        let (dx, dy) = (end.x - l.x, end.y - l.y);
                        dx * dx + dy * dy >= FREEHAND_MIN_STEP * FREEHAND_MIN_STEP
                    }
                    None => true,
                };
                if !append {
                    // 步距未到：笔画没有变化，不必重绘
                    return;
                }
                // 点列 `Arc` 共享（撤销快照靠引用计数），只有真正追加时才拷贝
                std::sync::Arc::make_mut(pts).push(end);
                appended = last.map(|l| (l, end));
            } else {
                draft.kind = match &draft.kind {
                    Kind::Rect { a, radius, filled, .. } => Kind::Rect {
                        a: *a,
                        b: end,
                        radius: *radius,
                        filled: *filled,
                    },
                    // 拖动中重建箭头：控制点保持在 1/3、2/3 处（EDT-3 从直线开始）
                    Kind::Arrow { from, .. } => Kind::arrow(*from, end),
                    // 区域类（高亮/模糊）：只更新对角点，半径/圆角/样式不变
                    Kind::Highlight { a, corner, .. } => Kind::Highlight {
                        a: *a,
                        b: end,
                        corner: *corner,
                    },
                    Kind::Blur { a, radius, corner, .. } => Kind::Blur {
                        a: *a,
                        b: end,
                        radius: *radius,
                        corner: *corner,
                    },
                    // 聚光：同区域类，只更新对角点
                    Kind::Glow { a, corner, .. } => Kind::Glow {
                        a: *a,
                        b: end,
                        corner: *corner,
                    },
                    // 序号标记：只更新引线尖端（圆心 / 编号 / 字号 / 半径不变）
                    Kind::Number {
                        center,
                        n,
                        font_size,
                        radius,
                        ..
                    } => Kind::Number {
                        center: *center,
                        tip: end,
                        n: *n,
                        font_size: *font_size,
                        radius: *radius,
                    },
                    // EDT-13 直线：只更新终点（起点不变）
                    Kind::Line { from, .. } => Kind::Line { from: *from, to: end },
                    // EDT-14 椭圆：只更新对角 b（a 不变）
                    Kind::Ellipse { a, filled, .. } => Kind::Ellipse {
                        a: *a,
                        b: end,
                        filled: *filled,
                    },
                    // 手绘已在上面的原地追加分支处理；文本不经草稿拖拽（Text 工具走输入状态机）
                    Kind::Freehand { .. } | Kind::Text { .. } => return,
                };
            }
        }
        // 手绘只追加：脏区取新增段的局部，避免长笔画每帧重绘整笔包围盒；
        // 其余草稿形状会整体变（矩形 / 箭头 / 椭圆…），仍按整块包围盒重绘。
        let dirty = match appended {
            Some((a, b)) => self.draft.as_ref().map(|d| segment_region(a, b, d.style.width)),
            None => union_rect(before, self.draft_region()),
        };
        if let Some(d) = dirty {
            self.repaint(expand_rect(d, 2));
        }
    }

    /// 草稿终点：鼠标点已夹在选区内，再按 Shift 施加几何约束（EDT-1/EDT-2/EDT-12）；
    /// 约束结果仍要落在选区内，故再夹一次（贴边时正方形/45° 会被裁到边界）。
    fn constrained_end(&self, cx: i32, cy: i32, shift: bool, sel: SelRect) -> Point {
        let end = Point::new(cx as f32, cy as f32);
        if !shift {
            return end;
        }
        let snapped = match self.draft.as_ref().map(|d| &d.kind) {
            Some(Kind::Rect { a, .. }) => crate::editor::square_corner(*a, end),
            // 箭头与序号引线共用方向吸附（起点 / 徽标圆心为锚点）
            Some(Kind::Arrow { from, .. }) => crate::editor::snap_direction(*from, end),
            Some(Kind::Number { center, .. }) => crate::editor::snap_direction(*center, end),
            // EDT-13 直线：Shift 吸附水平 / 垂直 / 45°（与箭头同款）
            Some(Kind::Line { from, .. }) => crate::editor::snap_direction(*from, end),
            // EDT-14 椭圆：Shift 画正圆（外接矩形为正方形，与矩形同款）
            Some(Kind::Ellipse { a, .. }) => crate::editor::square_corner(*a, end),
            // EDT-15 手绘：自由笔画，不施加方向约束
            _ => return end,
        };
        Point::new(
            snapped.x.clamp(sel.x as f32, (sel.x + sel.w - 1).max(sel.x) as f32),
            snapped.y.clamp(sel.y as f32, (sel.y + sel.h - 1).max(sel.y) as f32),
        )
    }

    /// 放开：过短视为误触丢弃；保存并选中，入撤销栈。
    pub(super) fn commit_draft(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        // 先取几何（对象非 Copy，push 会移动），再决定是否保留
        let draft_bounds = render::bounds_to_clip(&draft.bounds());
        let kept = !is_degenerate(&draft);
        if kept {
            self.doc.push(draft);
            // 新建的箭头随即选中，控制柄落在 1/3、2/3 处（EDT-3）
            self.selected = Some(self.doc.objects().len() - 1);
        }
        self.doc.commit(kept);
        let c = draft_bounds;
        let mut region = SelRect {
            x: c.0,
            y: c.1,
            w: c.2 - c.0,
            h: c.3 - c.1,
        };
        if let Some(bar) = self.bar.as_ref() {
            // 撤销/重做按钮的可用性随之变化
            region = union_rect(Some(region), Some(expand_rect(bar.rect, 2))).unwrap_or(region);
        }
        self.repaint(expand_rect(region, 2));
    }

    /// 草稿对象当前占据的像素区域。
    pub(super) fn draft_region(&self) -> Option<SelRect> {
        let c = render::bounds_to_clip(&self.draft.as_ref()?.bounds());
        Some(SelRect {
            x: c.0,
            y: c.1,
            w: c.2 - c.0,
            h: c.3 - c.1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手绘增量脏区（EDT-15）：只覆盖新采样段 + 半个线宽，不覆盖整笔历史，
    /// 否则长笔画每帧都要回贴 + 重绘整个笔画包围盒（4K 选区下等于每帧全屏重绘）。
    #[test]
    fn freehand_dirty_region_covers_only_new_segment() {
        let (a, b) = (Point::new(100.0, 100.0), Point::new(110.0, 100.0));
        let r = segment_region(a, b, 4.0);
        // 段两端 + 半个线宽（2px）都要落在脏区内，否则笔画会被截断
        assert!(r.x <= 98 && r.x + r.w >= 113, "应覆盖整段含端帽：{r:?}");
        assert!(r.y <= 98 && r.y + r.h >= 103, "纵向应覆盖线宽：{r:?}");
        assert!(r.w <= 20 && r.h <= 10, "脏区不应扩到整笔包围盒：{r:?}");

        // 与整笔包围盒对比：长笔画的脏区面积必须远小于整笔
        let whole = segment_region(Point::new(0.0, 0.0), Point::new(1000.0, 800.0), 4.0);
        assert!(r.w * r.h * 100 < whole.w * whole.h, "增量脏区应远小于整笔包围盒");
    }
}
