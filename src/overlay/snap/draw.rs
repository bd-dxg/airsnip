// SPDX-License-Identifier: GPL-3.0-only
//! 候选框绘制（CAP-6 / ADR 0037）：只有一条强调色细边。
//! 手柄与工具栏留给「确认选中」之后——预览态画上手柄，用户会以为已经选好了。
//! 亮度三档也在本文件：桌面暗 / 窗口半亮 / 控件全亮（ADR 0039 §14）。

use tiny_skia::{Paint, PathBuilder, Rect, Stroke, Transform};

use super::super::geometry::{SelRect, intersect_rect};
use super::super::shape::ACCENT;
use super::super::surface::{blit_region, dim_region};
use super::super::ui::Ui;
use super::Snap;

/// 候选的亮度（ADR 0037 / ADR 0039 §14）：窗口半亮、控件全亮。
///
/// 「窗口半亮」而不是全亮：候选常常就是整屏（任务栏自动隐藏 + 窗口最大化、全屏窗口），
/// 全亮的话暗化一点不剩，用户看不出覆盖层已经生效，只会以为 F1 没反应。
/// 控件级下钻后候选比窗口小，**窗口其余部分（标题栏 / 状态栏）必须留在半亮档**——
/// 否则同一窗口一半亮一半暗，看起来像「顶部多了一条暗带」（实测反馈）。
/// 所以这里是三档：桌面保持暗图 < 窗口半亮 < 控件回贴原图。
///
/// `region` 是本次真正重绘的区域，内部按两个矩形的交集分段处理。
pub(in crate::overlay) fn shade_candidate(
    display: &mut tiny_skia::Pixmap,
    original: &tiny_skia::Pixmap,
    region: SelRect,
    snap: &Snap,
) {
    if let Some(win) = snap.window()
        && let Some(part) = intersect_rect(region, win)
    {
        blit_region(display, original, part);
        dim_region(display, part, super::CAND_K);
    }
    // 整窗一档（`cand == win`，含滚轮爬回整窗）到上一步为止就是最终结果（半亮）；
    // 只有控件级才把控件矩形回贴原图。
    if snap.is_finer_than_window()
        && let Some(cand) = snap.candidate
        && let Some(part) = intersect_rect(region, cand)
    {
        blit_region(display, original, part);
    }
}

/// 画候选窗口边框。
pub(in crate::overlay) fn draw_candidate(pixmap: &mut tiny_skia::Pixmap, rect: SelRect, ui: Ui) {
    let line = ui.f(2.0);
    let Some(r) = Rect::from_xywh(
        rect.x as f32 + 0.5,
        rect.y as f32 + 0.5,
        (rect.w as f32 - 1.0).max(1.0),
        (rect.h as f32 - 1.0).max(1.0),
    ) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(r);
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], 255);
    let stroke = Stroke {
        width: line,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}
