// SPDX-License-Identifier: GPL-3.0-only
//! 圆角半径的读写与收敛（EDT-1）：矩形类对象共用同一套上限规则。
//!
//! 圆角不再由工具栏开关控制，而是选中对象后拖内侧角的**圆角控制柄**直接调
//! （见 `overlay::objedit`）。上限取短边一半 —— 与 `render` 里圆角路径的收敛规则一致，
//! 拖过头也不会把图形拖崩。

use super::Kind;

impl Kind {
    /// 当前圆角半径；非矩形类对象返回 None。
    pub fn corner(&self) -> Option<f32> {
        match self {
            Kind::Rect { radius, .. } => Some(*radius),
            _ => None,
        }
    }

    /// 圆角半径上限：短边一半；非矩形类对象返回 None。
    pub fn corner_limit(&self) -> Option<f32> {
        let Kind::Rect { a, b, .. } = self else {
            return None;
        };
        Some(((a.x - b.x).abs().min((a.y - b.y).abs())) * 0.5)
    }

    /// 设置圆角半径，夹到 `[0, 短边一半]`；返回是否真的发生了变化。
    pub fn set_corner(&mut self, r: f32) -> bool {
        let Some(limit) = self.corner_limit() else {
            return false;
        };
        let Kind::Rect { radius, .. } = self else {
            return false;
        };
        let v = r.clamp(0.0, limit);
        if (v - *radius).abs() < 0.01 {
            return false;
        }
        *radius = v;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Point;

    fn rect(a: (f32, f32), b: (f32, f32), radius: f32) -> Kind {
        Kind::Rect {
            a: Point::new(a.0, a.1),
            b: Point::new(b.0, b.1),
            radius,
            filled: false,
        }
    }

    /// 上限取短边一半（此处短边是高度 100）。
    #[test]
    fn limit_is_half_of_shorter_side() {
        assert_eq!(rect((0.0, 0.0), (400.0, 100.0), 0.0).corner_limit(), Some(50.0));
        assert_eq!(rect((0.0, 0.0), (60.0, 300.0), 0.0).corner_limit(), Some(30.0));
    }

    /// 设置时夹在上限内，且不接受负值。
    #[test]
    fn set_clamps_to_limit_and_zero() {
        let mut k = rect((0.0, 0.0), (400.0, 100.0), 0.0);
        assert!(k.set_corner(500.0));
        assert_eq!(k.corner(), Some(50.0), "超出上限应夹到短边一半");
        assert!(k.set_corner(-20.0));
        assert_eq!(k.corner(), Some(0.0), "负值应夹到 0");
    }

    /// 值没变化时返回 false（拖动时避免空撤销步）。
    #[test]
    fn set_reports_no_change_when_identical() {
        let mut k = rect((0.0, 0.0), (400.0, 100.0), 20.0);
        assert!(!k.set_corner(20.0));
        assert!(k.set_corner(21.0));
    }

    /// 非矩形对象没有圆角概念。
    #[test]
    fn non_rect_has_no_corner() {
        let mut arrow = Kind::arrow(Point::new(0.0, 0.0), Point::new(10.0, 10.0));
        assert_eq!(arrow.corner(), None);
        assert_eq!(arrow.corner_limit(), None);
        assert!(!arrow.set_corner(5.0));
    }
}
