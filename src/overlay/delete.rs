// SPDX-License-Identifier: GPL-3.0-only
//! 选中对象的单独删除（issue #71 / ADR 0032）：`Del` / `Backspace` 只移除当前选中的那一个对象，
//! 一步撤销可恢复，不连带撤销它之后画的东西。按键派发在 `wndproc`。
//!
//! 从 `annotate` 拆出独立模块：那个文件已接近 300 行上限，而删除的脏区判定
//! （聚光灯会整选区改变亮度）值得单独用测试守住。

use crate::editor::{Document, Kind, Object};

use super::geometry::{SelRect, expand_rect, union_rect};

impl super::Overlay {
    /// 删除选中对象（issue #71 / ADR 0032）。
    /// 返回**是否真的删了**——为 `false` 时调用方把按键让给工具切换
    /// （`Del` / `Backspace` 可被用户绑成工具键，没选中对象时不该吃掉按键）。
    pub(super) fn on_delete_selected(&mut self) -> bool {
        // 拖动 / 绘制 / 单击待定中忽略：这些流程正按索引改写同一个对象，
        // 此时删掉会让拖拽指向已消失的对象（与 `nudge` 的中断判据一致）。
        if self.obj_drag.is_some() || self.ctrl_drag.is_some() || self.draft.is_some() || self.pending_click.is_some() {
            return false;
        }
        let Some(index) = self.selected else {
            return false;
        };
        let glow_before = glow_on(self.doc.objects());
        let Some(region) = delete_object(&mut self.doc, index) else {
            // 索引已失效（撤销/重做后索引会变，`on_undo` / `on_redo` 已清空选中）：只清选中
            self.selected = None;
            return false;
        };
        let glow_off = glow_before && !glow_on(self.doc.objects());
        self.selected = None;
        // 删掉的若是文本对象，第二行工具栏随之收起（EDT-4）
        self.refresh_toolbar();
        // 被删对象所在区域 + 工具栏（撤销按钮可用性变了）
        let mut dirty = delete_dirty_region(region, self.selection, self.dirty_pad(), glow_off);
        if let Some(bar) = self.bar.as_ref() {
            dirty = union_rect(Some(dirty), Some(expand_rect(bar.rect, 2))).unwrap_or(dirty);
        }
        self.repaint(dirty);
        true
    }
}

/// 删除指定索引的对象，归档为**一步**撤销，返回被删对象占用的像素区域（调用方据此局部重绘）。
/// 越界索引不删、不动历史，返回 `None`。
fn delete_object(doc: &mut Document, index: usize) -> Option<SelRect> {
    let obj = doc.objects().get(index)?;
    let c = crate::render::bounds_to_clip(&obj.bounds());
    let region = SelRect {
        x: c.0,
        y: c.1,
        w: c.2 - c.0,
        h: c.3 - c.1,
    };
    // 先 begin 再删：撤销回到「删除前」的整份对象列表
    doc.begin();
    doc.remove(index);
    doc.commit(true);
    Some(region)
}

/// 文档里是否存在聚光对象：它决定选区是否「整体暗化、只在聚光区域恢复亮度」。
fn glow_on(objects: &[Object]) -> bool {
    objects.iter().any(|o| matches!(o.kind, Kind::Glow { .. }))
}

/// 删除后的脏区。
///
/// 常规情况只重绘被删对象的包围盒；但**删掉最后一个聚光对象**时，选区的整体暗化消失、
/// 亮度变化覆盖**整个选区**——只重绘原包围盒会在原处留下一块暗斑。
/// 这与 `start_draw` 里「第一块聚光按下即重绘整个选区」是对称的。
fn delete_dirty_region(obj: SelRect, sel: Option<SelRect>, pad: i32, glow_turned_off: bool) -> SelRect {
    match sel {
        Some(s) if glow_turned_off => expand_rect(s, pad),
        _ => expand_rect(obj, pad),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Point, Style};

    fn obj(kind: Kind) -> Object {
        Object {
            kind,
            style: Style::new([255, 0, 0, 255], 4.0),
        }
    }

    fn rect_at(x: f32) -> Object {
        obj(Kind::Rect {
            a: Point::new(x, 10.0),
            b: Point::new(x + 20.0, 30.0),
            radius: 0.0,
            filled: false,
        })
    }

    fn glow_at(x: f32) -> Object {
        obj(Kind::Glow {
            a: Point::new(x, 10.0),
            b: Point::new(x + 20.0, 30.0),
            corner: 0.0,
        })
    }

    /// 删除单个对象（issue #71）：只移除目标那一个，其余保留；归档为**一步**撤销，
    /// `Ctrl+Z` 能原样恢复；越界索引不删也不污染历史。
    #[test]
    fn delete_one_object_keeps_others_and_is_one_undo_step() {
        let mut doc = Document::new();
        for i in 0..3 {
            doc.push(rect_at(i as f32 * 50.0));
        }
        // 删中间那个（用户画了多个标注、只想删其中一个的场景）
        let region = delete_object(&mut doc, 1).expect("索引 1 有效");
        assert_eq!(doc.objects().len(), 2, "只应少一个对象");
        assert!(region.w > 0 && region.h > 0, "脏区应覆盖被删对象的包围盒：{region:?}");
        // 剩下的是第 1、3 个（x = 0 与 x = 100），顺序不变
        // （bounds 含线宽外扩，故只比位置的相对关系，不比绝对值）
        let xs: Vec<f32> = doc.objects().iter().map(|o| o.bounds().x0).collect();
        assert_eq!(xs.len(), 2);
        assert!(
            (xs[1] - xs[0] - 100.0).abs() < 0.01,
            "中间那个应被删掉、首尾保留：{xs:?}"
        );
        // 一步撤销回到删除前
        assert!(doc.can_undo());
        assert!(doc.undo());
        assert_eq!(doc.objects().len(), 3, "一步撤销应恢复被删对象");
        // 越界索引：不删、不进历史
        assert!(delete_object(&mut doc, 9).is_none());
        assert!(!doc.undo(), "越界删除不应产生撤销步");
    }

    /// 聚光灯判定：只要还剩一块聚光，选区就仍是「整体暗化」状态。
    #[test]
    fn glow_on_sees_remaining_glow_objects() {
        assert!(!glow_on(&[]));
        assert!(!glow_on(&[rect_at(0.0)]));
        assert!(glow_on(&[rect_at(0.0), glow_at(50.0)]));
        assert!(glow_on(&[glow_at(0.0), glow_at(50.0)]), "还有一块聚光时仍是暗化态");
    }

    /// 删掉**最后一个**聚光对象：脏区必须覆盖整个选区（否则原处留暗斑）；
    /// 其余情况下只重绘被删对象的包围盒。
    #[test]
    fn last_glow_removal_repaints_whole_selection() {
        let sel = SelRect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        };
        let obj = SelRect {
            x: 200,
            y: 150,
            w: 60,
            h: 40,
        };
        let pad = 12;
        // 最后一个聚光被删 → 整个选区（含外扩）
        assert_eq!(
            delete_dirty_region(obj, Some(sel), pad, true),
            expand_rect(sel, pad),
            "选区整体亮度翻转，必须整选区重绘"
        );
        // 还有其它聚光 / 删的不是聚光 → 只重绘被删对象
        assert_eq!(delete_dirty_region(obj, Some(sel), pad, false), expand_rect(obj, pad));
        // 没有选区（理论上不会发生）时退回对象包围盒
        assert_eq!(delete_dirty_region(obj, None, pad, true), expand_rect(obj, pad));
    }
}
