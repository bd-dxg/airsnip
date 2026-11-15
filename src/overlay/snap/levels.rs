// SPDX-License-Identifier: GPL-3.0-only
//! 控件层级栈（CAP-6 二级 / ADR 0039 §5、§9）：由深到浅的一组矩形 + 「当前展示第几层」。
//! `level == rects.len()` 表示整窗一档——最粗那一档永远由调用方（窗口矩形）提供，
//! 不依赖 UIA 树能不能到顶（实测常常到不了）。

use super::super::geometry::SelRect;

/// 控件层级（ADR 0039）：`rects` 由深到浅，`level` 是当前展示下标（= `rects.len()` 表示整窗）。
#[derive(Default)]
pub(super) struct Levels {
    rects: Vec<SelRect>,
    level: usize,
}

impl Levels {
    /// 清空（换窗口 / 回到整窗一档）。
    pub(super) fn clear(&mut self) {
        self.rects.clear();
        self.level = 0;
    }

    /// 是否没有更细的层级（该程序不暴露子元素）——决定重查步长（见 `Snap::needs_query`）。
    pub(super) fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// 回包到了：换上一组层级并回到最深一档，返回最深一级。空数组 = 没有信息，保持原状并返回 `None`。
    pub(super) fn set(&mut self, rects: Vec<SelRect>) -> Option<SelRect> {
        if rects.is_empty() {
            return None;
        }
        self.rects = rects;
        self.level = 0;
        self.rects.first().copied()
    }

    /// 滚轮爬层级：`up` = 往父级（更粗），否则往更深。返回新目标（已是最后一层返回 `None`）。
    /// 最粗一档由 `window` 提供，不依赖 UIA 树。
    pub(super) fn climb(&mut self, up: bool, window: Option<SelRect>) -> Option<SelRect> {
        let last = self.rects.len();
        if last == 0 {
            return None;
        }
        let next = if up {
            self.level + 1
        } else {
            self.level.saturating_sub(1)
        }
        .min(last);
        if next == self.level {
            return None;
        }
        self.level = next;
        if next < last { Some(self.rects[next]) } else { window }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 爬层级（ADR 0039 §1）：从最深一档往上是父级、最后一档恒为整窗，到顶 / 到底就不消费滚轮。
    #[test]
    fn climb_walks_from_deepest_to_window() {
        let deep = SelRect {
            x: 8,
            y: 8,
            w: 40,
            h: 20,
        };
        let mid = SelRect {
            x: 0,
            y: 0,
            w: 200,
            h: 150,
        };
        let win = SelRect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        };

        let mut l = Levels::default();
        assert!(l.climb(true, Some(win)).is_none(), "没有层级时不消费滚轮");

        assert_eq!(l.set(vec![deep, mid]), Some(deep), "回包后落在最深一档");
        assert_eq!(l.climb(true, Some(win)), Some(mid), "上滚到父级");
        assert_eq!(l.climb(true, Some(win)), Some(win), "最粗一档永远是整窗");
        assert!(l.climb(true, Some(win)).is_none(), "到顶不再消费");

        assert_eq!(l.climb(false, Some(win)), Some(mid), "下滚回更深");
        assert_eq!(l.climb(false, Some(win)), Some(deep), "回到最深一档");
        assert!(l.climb(false, Some(win)).is_none(), "到底不再消费");

        assert_eq!(l.set(Vec::new()), None, "空回包不覆盖已有层级");
        assert!(!l.is_empty(), "旧层级应保留");
    }
}
