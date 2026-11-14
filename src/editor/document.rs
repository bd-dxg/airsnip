// SPDX-License-Identifier: GPL-3.0-only
//! 编辑文档（EDT-7）：对象列表 + 快照式撤销/重做。
//!
//! 快照式（每次编辑保存整份对象列表）而非逆操作命令：
//! 对象数量少（每个约 40 字节，文本对象共享字符串），快照成本可忽略，
//! 且不会出现逆操作实现的隐蔽错误。
//!
//! 例外是手绘笔画（`Kind::Freehand`）：点列长度随用户手势增长，所以点列用 `Arc` 共享——
//! 快照克隆只加引用计数，只有被编辑的那一笔才拷贝（见 ADR 0030 §3）。

use super::Object;

/// 撤销栈深度上限，防止长时间标注累积内存。
pub const MAX_HISTORY: usize = 100;

/// 编辑文档：对象列表 + 撤销/重做历史。
#[derive(Default)]
pub struct Document {
    objects: Vec<Object>,
    undo: Vec<Vec<Object>>,
    redo: Vec<Vec<Object>>,
    /// `begin` 记录的快照，`commit` 时归档。
    pending: Option<Vec<Object>>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    /// 可变对象列表（控制柄拖动用；不自动进历史，调用方负责 `begin`/`commit`）。
    pub fn objects_mut(&mut self) -> &mut [Object] {
        &mut self.objects
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// 开始一次可撤销编辑（拖拽/新建/编辑文本前调用）。
    pub fn begin(&mut self) {
        self.pending = Some(self.objects.clone());
    }

    /// 结束编辑：`changed` 为真时把 `begin` 时的快照压入撤销栈。
    /// 拖拽中每帧调用 `begin` 会污染历史，调用方应在按下时 `begin`、松开时 `commit`。
    pub fn commit(&mut self, changed: bool) {
        let Some(before) = self.pending.take() else {
            return;
        };
        if !changed {
            return;
        }
        self.push_history(before);
    }

    /// 直接提交一次变更（无 `begin` 的简单操作，如添加对象后立即归档）。
    fn push_history(&mut self, before: Vec<Object>) {
        self.undo.push(before);
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// 追加对象（不自动进历史，需调用方自行 `begin`/`commit`）。
    pub fn push(&mut self, obj: Object) {
        self.objects.push(obj);
    }

    /// 整体平移所有对象（选区平移时标注跟随，ADR 0036）。
    /// **不进撤销栈**：选区位置本身不可撤销，若只把标注位置入历史，
    /// 撤销一次就会出现「选区已挪回、标注却没跟着回来」的错位。
    pub fn translate(&mut self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        for obj in &mut self.objects {
            obj.kind.translate(dx as f32, dy as f32);
        }
    }

    /// 移除指定索引的对象（不自动进历史）。
    pub fn remove(&mut self, index: usize) -> Object {
        self.objects.remove(index)
    }

    /// 撤销；返回是否有变化。
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.objects, prev));
        true
    }

    /// 重做；返回是否有变化。
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.objects, next));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Kind, Object, Point, Style};

    fn rect_at(x: f32) -> Object {
        Object {
            kind: Kind::Rect {
                a: Point::new(x, 0.0),
                b: Point::new(x + 10.0, 10.0),
                radius: 0.0,
                filled: false,
            },
            style: Style::new([255, 0, 0, 255], 4.0),
        }
    }

    /// ADR 0036：选区平移时标注整体跟随，而且**不进撤销栈**：
    /// 选区位置本身不可撤销，只把标注位置入历史会造成「框挪回来了、标注没跟着」的错位。
    #[test]
    fn translate_moves_every_object_without_history() {
        let mut doc = Document::new();
        doc.push(rect_at(0.0));
        doc.push(rect_at(100.0));
        doc.translate(5, -3);
        // 读 `kind` 里的原始坐标（`bounds()` 会把线宽算进去）
        let xs: Vec<f32> = doc
            .objects()
            .iter()
            .map(|o| match o.kind {
                Kind::Rect { a, .. } => a.x,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(xs, vec![5.0, 105.0], "两个对象都应整体跟随平移");
        assert!(!doc.can_undo(), "平移不进撤销栈");

        // 零位移不应改动任何东西（避免每帧空转时把对象反复重写）
        doc.translate(0, 0);
        assert!(matches!(doc.objects()[0].kind, Kind::Rect { a, .. } if a.x == 5.0));
    }
}
