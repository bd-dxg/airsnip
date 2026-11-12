// SPDX-License-Identifier: GPL-3.0-only
//! 标注对象的新建与提交（EDT-1…EDT-15）：在选区内按下 → 拖动 → 松手提交。
//!
//! 对象模型、撤销栈、曲线几何全部来自 `airsnip_core::editor`，这里只负责
//! 「鼠标手势 → `Kind`」与「手势结束 → 进历史」。文本工具走输入法状态机
//! （text-input-v3），S3 接入，不在这里。

use std::sync::Arc;

use airsnip_core::editor::{Kind, Object, Point, Tool, next_number, snap_direction, square_corner};

use super::state::Overlay;

/// 手绘采样最小间距（物理像素，EDT-15）。
const FREEHAND_MIN_STEP: f32 = 2.0;

/// 按下：按当前工具建草稿对象。文本工具暂不建（等输入法）。
pub fn begin(ov: &mut Overlay, p: (i32, i32)) {
    let (x, y) = p;
    let pt = Point::new(x as f32, y as f32);
    let kind = match ov.tool {
        Tool::Text => {
            super::text_input::begin(ov, p);
            return;
        }
        Tool::Rect => Kind::Rect {
            a: pt,
            b: pt,
            radius: 0.0,
            filled: ov.filled,
        },
        Tool::Arrow => Kind::arrow(pt, pt),
        Tool::Highlight => Kind::Highlight {
            a: pt,
            b: pt,
            corner: 0.0,
        },
        Tool::Blur => Kind::Blur {
            a: pt,
            b: pt,
            radius: ov.blur_radius,
            corner: 0.0,
        },
        Tool::Glow => Kind::Glow {
            a: pt,
            b: pt,
            corner: 0.0,
        },
        Tool::Number => {
            let n = next_number(ov.doc.objects());
            let fs = ov.number_font_size;
            Kind::Number {
                center: pt,
                tip: pt,
                n,
                font_size: fs,
                radius: crate::draw::text::with_engine(|sink| airsnip_core::draw::objects::number_radius(fs, n, sink)),
            }
        }
        Tool::Line => Kind::Line { from: pt, to: pt },
        Tool::Ellipse => Kind::Ellipse {
            a: pt,
            b: pt,
            filled: ov.filled,
        },
        Tool::Freehand => Kind::Freehand {
            pts: Arc::new(vec![pt]),
        },
    };
    ov.doc.begin();
    ov.draft = Some(Object { kind, style: ov.style });
}

/// 拖动：更新草稿终点（限制在选区内）。
pub fn update(ov: &mut Overlay, p: (i32, i32), shift: bool) {
    let Some(sel) = ov.sel else { return };
    let (x, y) = sel.clamp_point(p.0, p.1);
    let raw = Point::new(x as f32, y as f32);
    // Shift 约束（与 Windows 版一致）：矩形/椭圆 → 正方形；箭头/直线 → 水平/垂直/45° 吸附
    let end = if shift {
        match ov.tool {
            Tool::Arrow | Tool::Line => {
                let from = match ov.draft.as_ref().map(|d| &d.kind) {
                    Some(Kind::Arrow { from, .. }) => *from,
                    Some(Kind::Line { from, .. }) => *from,
                    _ => raw,
                };
                snap_direction(from, raw)
            }
            Tool::Rect | Tool::Highlight | Tool::Blur | Tool::Glow | Tool::Ellipse => {
                let anchor = match ov.draft.as_ref().map(|d| &d.kind) {
                    Some(Kind::Rect { a, .. })
                    | Some(Kind::Highlight { a, .. })
                    | Some(Kind::Blur { a, .. })
                    | Some(Kind::Glow { a, .. })
                    | Some(Kind::Ellipse { a, .. }) => *a,
                    _ => raw,
                };
                square_corner(anchor, raw)
            }
            _ => raw,
        }
    } else {
        raw
    };
    // 约束后可能跑出选区，再钳一次
    let (cx, cy) = sel.clamp_point(end.x.round() as i32, end.y.round() as i32);
    let end = Point::new(cx as f32, cy as f32);
    let Some(draft) = ov.draft.as_mut() else {
        return;
    };
    match &mut draft.kind {
        Kind::Rect { b, .. }
        | Kind::Highlight { b, .. }
        | Kind::Blur { b, .. }
        | Kind::Glow { b, .. }
        | Kind::Ellipse { b, .. } => *b = end,
        Kind::Arrow { from, c1, c2, to } => {
            // 直线箭头：控制点落在 1/3、2/3（曲线由后续拖控制柄生成）
            let (fx, fy) = (from.x, from.y);
            *to = end;
            *c1 = Point::new(fx + (end.x - fx) / 3.0, fy + (end.y - fy) / 3.0);
            *c2 = Point::new(fx + (end.x - fx) * 2.0 / 3.0, fy + (end.y - fy) * 2.0 / 3.0);
        }
        Kind::Line { to, .. } => *to = end,
        Kind::Number { tip, .. } => *tip = end,
        Kind::Freehand { pts } => {
            // 原地追加（EDT-15）：点列可能很长，避免每帧整列克隆
            if let Some(last) = pts.last() {
                let (dx, dy) = (end.x - last.x, end.y - last.y);
                if dx * dx + dy * dy >= FREEHAND_MIN_STEP * FREEHAND_MIN_STEP {
                    Arc::make_mut(pts).push(end);
                }
            }
        }
        Kind::Text { .. } => {}
    }
}

/// 松手：草稿进历史（太小视为误触丢弃）。
pub fn commit(ov: &mut Overlay) {
    let Some(draft) = ov.draft.take() else {
        return;
    };
    let degenerate = match &draft.kind {
        // 序号标记按下即成立（点一下也能放徽标）
        Kind::Number { .. } => false,
        Kind::Freehand { pts } => pts.len() < 2,
        Kind::Arrow { from, to, .. } | Kind::Line { from, to } => {
            (to.x - from.x).abs() < 2.0 && (to.y - from.y).abs() < 2.0
        }
        Kind::Ellipse { a, b, .. } => (a.x - b.x).abs() < 2.0 || (a.y - b.y).abs() < 2.0,
        // 框类对象按两个角点直接比，别用包围盒（包围盒含线宽外扩，会把小拖动判成有效）
        Kind::Rect { a, b, .. } | Kind::Highlight { a, b, .. } | Kind::Blur { a, b, .. } | Kind::Glow { a, b, .. } => {
            (a.x - b.x).abs() < 3.0 || (a.y - b.y).abs() < 3.0
        }
        Kind::Text { .. } => true,
    };
    if degenerate {
        ov.doc.commit(false);
        crate::wayland::debug("草稿太小，丢弃");
        return;
    }
    crate::wayland::debug(format!("提交对象 #{}", ov.doc.objects().len()));
    ov.doc.push(draft);
    ov.doc.commit(true);
}

/// 撤销 / 重做（返回是否有变化）。
pub fn undo(ov: &mut Overlay) -> bool {
    ov.doc.undo()
}

pub fn redo(ov: &mut Overlay) -> bool {
    ov.doc.redo()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::state::Sel;

    fn overlay() -> Overlay {
        let canvas = crate::canvas::Canvas::new(64, 64);
        let mut ov = Overlay::new(&canvas).unwrap();
        ov.sel = Some(Sel {
            x: 0,
            y: 0,
            w: 64,
            h: 64,
        });
        ov
    }

    #[test]
    fn drag_creates_rect_and_commits() {
        let mut ov = overlay();
        ov.tool = Tool::Rect;
        begin(&mut ov, (10, 10));
        update(&mut ov, (40, 40), false);
        commit(&mut ov);
        assert_eq!(ov.doc.objects().len(), 1);
        assert!(matches!(ov.doc.objects()[0].kind, Kind::Rect { .. }));
    }

    #[test]
    fn tiny_drag_is_discarded() {
        let mut ov = overlay();
        ov.tool = Tool::Rect;
        begin(&mut ov, (10, 10));
        update(&mut ov, (11, 11), false);
        commit(&mut ov);
        assert!(ov.doc.objects().is_empty());
    }

    #[test]
    fn number_click_places_badge() {
        let mut ov = overlay();
        ov.tool = Tool::Number;
        begin(&mut ov, (20, 20));
        commit(&mut ov);
        assert_eq!(ov.doc.objects().len(), 1);
    }

    #[test]
    fn points_are_clamped_into_selection() {
        let mut ov = overlay();
        ov.sel = Some(Sel {
            x: 10,
            y: 10,
            w: 20,
            h: 20,
        });
        ov.tool = Tool::Line;
        begin(&mut ov, (10, 10));
        update(&mut ov, (999, 999), false);
        let Kind::Line { to, .. } = ov.draft.as_ref().unwrap().kind else {
            panic!("应是直线")
        };
        assert_eq!((to.x, to.y), (29.0, 29.0));
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut ov = overlay();
        ov.tool = Tool::Rect;
        begin(&mut ov, (5, 5));
        update(&mut ov, (30, 30), false);
        commit(&mut ov);
        assert_eq!(ov.doc.objects().len(), 1);
        assert!(undo(&mut ov));
        assert!(ov.doc.objects().is_empty());
        assert!(redo(&mut ov));
        assert_eq!(ov.doc.objects().len(), 1);
    }

    #[test]
    fn text_tool_does_not_create_object_yet() {
        let mut ov = overlay();
        ov.tool = Tool::Text;
        begin(&mut ov, (10, 10));
        assert!(ov.draft.is_none());
    }
}
