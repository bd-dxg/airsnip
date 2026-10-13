// SPDX-License-Identifier: GPL-3.0-only
//! 对象二次编辑（§4.3 / EDT-7）：选中对象的包围框与八向控制柄、拖动移动与缩放、
//! 删除与层级调整（置顶/置底）。
//!
//! 文件分工：本文件是手柄几何与命中，`drag` 是拖动/删除/层级的状态机与脏区重绘，
//! `frame` 是包围框与控制柄绘制。几何变换复用 `editor::transform`（纯几何，可单测）；
//! 命中优先级（对象手柄 > 曲线控制柄 > 选区手柄 > 对象）见 `interaction`。

mod drag;
mod frame;

pub(in crate::overlay) use frame::draw_frame;

use crate::editor::{Bounds, Kind, Object};
use crate::render;

use super::geometry::SelRect;
use super::pick::PICK_RADIUS;

/// 对象手柄类型（八向缩放 + `Move`；箭头用两端点手柄 `ArrowFrom`/`ArrowTo`）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum ObjHandle {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
    /// 箭头起点（EDT-2）：拖动拉长 / 改向。
    ArrowFrom,
    /// 箭头终点（尖端）。
    ArrowTo,
}

/// 缩放后的最小边长（物理像素），避免对象被拖成零尺寸。
const MIN_SIZE: f32 = 4.0;

/// 拖动中的上下文。
pub(super) struct ObjDrag {
    index: usize,
    pub(super) handle: ObjHandle,
    /// 按下时的对象：每帧从它重算几何，避免逐帧增量的误差累积。
    orig: Object,
    /// 按下时的包围盒（映射基准）。
    frame: Bounds,
    px: i32,
    py: i32,
    /// 是否发生过实际变化（无变化不进撤销栈）。
    moved: bool,
}

/// 对象包围盒的像素矩形（含线宽 / 文本测量余量）。
pub(super) fn frame_of(obj: &Object) -> SelRect {
    let c = render::bounds_to_clip(&obj.bounds());
    SelRect {
        x: c.0,
        y: c.1,
        w: c.2 - c.0,
        h: c.3 - c.1,
    }
}

/// 手柄在包围盒上的位置。
fn handle_point(f: SelRect, h: ObjHandle) -> (f32, f32) {
    let (x0, y0) = (f.x as f32, f.y as f32);
    let (x1, y1) = ((f.x + f.w) as f32, (f.y + f.h) as f32);
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    match h {
        ObjHandle::Move => (cx, cy),
        ObjHandle::N => (cx, y0),
        ObjHandle::S => (cx, y1),
        ObjHandle::W => (x0, cy),
        ObjHandle::E => (x1, cy),
        ObjHandle::NW => (x0, y0),
        ObjHandle::NE => (x1, y0),
        ObjHandle::SW => (x0, y1),
        ObjHandle::SE => (x1, y1),
        // 箭头端点手柄不基于包围框（见 `frame::draw_frame` 的早退分支），不参与此处布局
        ObjHandle::ArrowFrom | ObjHandle::ArrowTo => (cx, cy),
    }
}

/// 命中对象手柄：箭头只有两端点手柄（没有包围框），其余对象为八个控制柄。
/// 「内部整体移动」不在此判定：未填充矩形的内部是空白，按下应当能画新标注；
/// 拖动已有对象由 `interaction` 在对象命中（`hit_object`）后发起（EDT-7）。
pub(in crate::overlay) fn hit_handle(obj: &Object, x: i32, y: i32) -> Option<ObjHandle> {
    if let Kind::Arrow { from, to, .. } = &obj.kind {
        for (h, p) in [(ObjHandle::ArrowFrom, from), (ObjHandle::ArrowTo, to)] {
            if (p.x - x as f32).abs() <= PICK_RADIUS as f32 && (p.y - y as f32).abs() <= PICK_RADIUS as f32 {
                return Some(h);
            }
        }
        return None;
    }
    let f = frame_of(obj);
    for h in [
        ObjHandle::NW,
        ObjHandle::N,
        ObjHandle::NE,
        ObjHandle::W,
        ObjHandle::E,
        ObjHandle::SW,
        ObjHandle::S,
        ObjHandle::SE,
    ] {
        let (hx, hy) = handle_point(f, h);
        if (hx - x as f32).abs() <= PICK_RADIUS as f32 && (hy - y as f32).abs() <= PICK_RADIUS as f32 {
            return Some(h);
        }
    }
    None
}

/// Shift 移动约束（EDT-7）：锁定到位移较大的轴（水平或垂直）。
pub(super) fn lock_axis(dx: i32, dy: i32) -> (i32, i32) {
    if dx.abs() >= dy.abs() { (dx, 0) } else { (0, dy) }
}

/// 等比缩放（Shift + 角手柄）：按主导轴比例同步另一轴，保持原宽高比。
/// 对边手柄无意义，直接返回普通缩放结果。
pub(super) fn resized_aspect(frame: Bounds, handle: ObjHandle, x: i32, y: i32) -> Bounds {
    if !matches!(handle, ObjHandle::NW | ObjHandle::NE | ObjHandle::SW | ObjHandle::SE) {
        return resized(frame, handle, x, y);
    }
    let new = resized(frame, handle, x, y);
    let (w0, h0) = (frame.x1 - frame.x0, frame.y1 - frame.y0);
    if w0 < 1e-3 || h0 < 1e-3 {
        return new;
    }
    // 取主导轴比例，另一轴按原宽高比跟算（不允许翻转：拖过头时夹住）
    let sx = ((new.x1 - new.x0) / w0).abs();
    let sy = ((new.y1 - new.y0) / h0).abs();
    let s = sx.max(sy);
    let (w, h) = (w0 * s, h0 * s);
    let west = matches!(handle, ObjHandle::NW | ObjHandle::SW);
    let north = matches!(handle, ObjHandle::NW | ObjHandle::NE);
    let (x0, x1) = if west {
        (new.x1 - w, new.x1)
    } else {
        (new.x0, new.x0 + w)
    };
    let (y0, y1) = if north {
        (new.y1 - h, new.y1)
    } else {
        (new.y0, new.y0 + h)
    };
    Bounds { x0, y0, x1, y1 }
}

/// 目标包围盒：把手柄拖到 (x, y) 后的新包围盒（含最小尺寸保护）。
fn resized(frame: Bounds, handle: ObjHandle, x: i32, y: i32) -> Bounds {
    let (mut x0, mut y0, mut x1, mut y1) = (frame.x0, frame.y0, frame.x1, frame.y1);
    let (fx, fy) = (x as f32, y as f32);
    let (west, north) = (
        matches!(handle, ObjHandle::W | ObjHandle::NW | ObjHandle::SW),
        matches!(handle, ObjHandle::N | ObjHandle::NW | ObjHandle::NE),
    );
    let (east, south) = (
        matches!(handle, ObjHandle::E | ObjHandle::NE | ObjHandle::SE),
        matches!(handle, ObjHandle::S | ObjHandle::SW | ObjHandle::SE),
    );
    if west {
        x0 = fx;
    } else if east {
        x1 = fx;
    }
    if north {
        y0 = fy;
    } else if south {
        y1 = fy;
    }
    // 最小尺寸：优先保住被拖动的那条边（拖过头时不让另一条边跟着跑）
    if x1 - x0 < MIN_SIZE {
        if west {
            x0 = x1 - MIN_SIZE;
        } else {
            x1 = x0 + MIN_SIZE;
        }
    }
    if y1 - y0 < MIN_SIZE {
        if north {
            y0 = y1 - MIN_SIZE;
        } else {
            y1 = y0 + MIN_SIZE;
        }
    }
    Bounds { x0, y0, x1, y1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Kind, Point, Style};

    fn rect_obj(a: (f32, f32), b: (f32, f32)) -> Object {
        Object {
            kind: Kind::Rect {
                a: Point::new(a.0, a.1),
                b: Point::new(b.0, b.1),
                radius: 0.0,
                filled: false,
            },
            style: Style::new([255, 0, 0, 255], 2.0),
        }
    }

    #[test]
    fn handle_hit_matches_corners_and_edges() {
        let obj = rect_obj((50.0, 50.0), (150.0, 100.0));
        let f = frame_of(&obj);
        // 左上角手柄
        assert_eq!(hit_handle(&obj, f.x, f.y), Some(ObjHandle::NW));
        // 右边中点（E）
        assert_eq!(hit_handle(&obj, f.x + f.w, f.y + f.h / 2), Some(ObjHandle::E));
        // 内部不是手柄（未填充矩形的内部要留给新标注）
        assert_eq!(hit_handle(&obj, f.x + f.w / 2, f.y + f.h / 2), None);
        // 远处 → 不命中
        assert_eq!(hit_handle(&obj, f.x - 60, f.y - 60), None);
    }

    #[test]
    fn shift_locks_move_to_dominant_axis() {
        assert_eq!(lock_axis(30, 4), (30, 0), "水平位移大→锁水平");
        assert_eq!(lock_axis(4, -30), (0, -30), "垂直位移大→锁垂直");
        assert_eq!(lock_axis(0, 0), (0, 0));
    }

    #[test]
    fn shift_resize_keeps_aspect_ratio_on_corners() {
        let f = Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: 100.0,
            y1: 50.0,
        }; // 2:1
        // 拖 SE 到 (200,60)：宽高比仍为 2:1，不动角 (0,0) 不变
        let n = resized_aspect(f, ObjHandle::SE, 200, 60);
        let (w, h) = (n.x1 - n.x0, n.y1 - n.y0);
        assert!((w / h - 2.0).abs() < 0.01, "实际 {w}×{h}");
        assert_eq!((n.x0, n.y0), (0.0, 0.0));
        // 拖 NW：不动角为 SE
        let n = resized_aspect(f, ObjHandle::NW, -100, -25);
        let (w, h) = (n.x1 - n.x0, n.y1 - n.y0);
        assert!((w / h - 2.0).abs() < 0.01, "实际 {w}×{h}");
        assert_eq!((n.x1, n.y1), (100.0, 50.0));
    }

    #[test]
    fn shift_resize_ignores_edge_handles() {
        let f = Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: 100.0,
            y1: 50.0,
        };
        assert_eq!(
            resized_aspect(f, ObjHandle::E, 200, 0),
            resized(f, ObjHandle::E, 200, 0),
            "边手柄不做等比"
        );
    }

    #[test]
    fn arrow_handles_are_endpoints_only() {
        let arrow = Object {
            kind: Kind::arrow(Point::new(40.0, 200.0), Point::new(200.0, 60.0)),
            style: Style::new([255, 0, 0, 255], 4.0),
        };
        // 头尾端点命中
        assert_eq!(hit_handle(&arrow, 40, 200), Some(ObjHandle::ArrowFrom));
        assert_eq!(hit_handle(&arrow, 200, 60), Some(ObjHandle::ArrowTo));
        // 包围框角上不应有手柄（箭头不画包围框）
        let f = frame_of(&arrow);
        assert_eq!(hit_handle(&arrow, f.x, f.y), None, "箭头不应有包围框手柄");
    }

    #[test]
    fn resize_east_edge_keeps_opposite_side() {
        let f = Bounds {
            x0: 10.0,
            y0: 20.0,
            x1: 110.0,
            y1: 70.0,
        };
        let n = resized(f, ObjHandle::E, 200, 0);
        assert_eq!((n.x0, n.y0), (10.0, 20.0), "西边/上下不应动");
        assert_eq!((n.x1, n.y1), (200.0, 70.0));
    }

    #[test]
    fn resize_has_min_size_when_dragged_past() {
        let f = Bounds {
            x0: 10.0,
            y0: 20.0,
            x1: 110.0,
            y1: 70.0,
        };
        // 把东边拖到西边左侧：应保住西边，宽度不小于 MIN_SIZE
        let n = resized(f, ObjHandle::E, -50, 0);
        assert_eq!(n.x0, 10.0);
        assert!(n.x1 - n.x0 >= MIN_SIZE, "实际 {n:?}");
        // 把西边拖到东边右侧：应保住东边
        let n = resized(f, ObjHandle::W, 300, 0);
        assert_eq!(n.x1, 110.0);
        assert!(n.x1 - n.x0 >= MIN_SIZE, "实际 {n:?}");
    }

    #[test]
    fn corner_resize_moves_both_axes() {
        let f = Bounds {
            x0: 0.0,
            y0: 0.0,
            x1: 100.0,
            y1: 50.0,
        };
        let n = resized(f, ObjHandle::NW, 20, 10);
        assert_eq!((n.x0, n.y0), (20.0, 10.0));
        assert_eq!((n.x1, n.y1), (100.0, 50.0));
    }
}
