// SPDX-License-Identifier: GPL-3.0-only
//! 命中测试与选中状态（EDT-3/EDT-7）：选区八向手柄、已有对象、曲线控制柄、
//! 选中对象的包围框边缘。控制柄位置即曲线控制点 P1/P2（直线箭头时落在 1/3、2/3 处）。
//!
//! 「整块命中」与「按下目标」是两件事（ADR 0023）：`hit_object` 只用于对象级操作，
//! 左键按下走 `pick_target`，它把命中结果分流为「绘制」或「移动 / 缩放」。
//! 滚轮与属性调整在 `wheel`。

use tiny_skia::{Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::editor::{Kind, Object, Point};

use super::geometry::{SelRect, expand_rect};
use super::objedit;
use super::ui::Ui;

/// 曲线控制柄半径基准值（含描边，需 ≤ 对象包围盒的余量）。
const CTRL_RADIUS_BASE: f32 = 4.5;
const ACCENT: u8 = 26; // 主题蓝
const ACCENT_G: u8 = 115;
const ACCENT_B: u8 = 232;

/// 手柄命中半径基准值（物理像素，经 `Ui` 缩放）。
pub(super) const PICK_RADIUS_BASE: f32 = 6.0;

/// 按界面缩放后的手柄命中半径。
pub(super) fn pick_radius(ui: Ui) -> i32 {
    ui.px(PICK_RADIUS_BASE)
}

/// 选区手柄类型（八向 + 移动）；对象手柄见 `objedit::ObjHandle`。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
    Move,
}

/// 八向手柄/移动命中（CAP-2）。
pub(super) fn hit_handle(sel: SelRect, x: i32, y: i32, ui: Ui) -> Option<Handle> {
    let pick = pick_radius(ui);
    let (x0, y0, x1, y1) = (sel.x, sel.y, sel.x + sel.w, sel.y + sel.h);
    let cx = x0 + sel.w / 2;
    let cy = y0 + sel.h / 2;
    for (hx, hy, h) in [
        (x0, y0, Handle::NW),
        (cx, y0, Handle::N),
        (x1, y0, Handle::NE),
        (x0, cy, Handle::W),
        (x1, cy, Handle::E),
        (x0, y1, Handle::SW),
        (cx, y1, Handle::S),
        (x1, y1, Handle::SE),
    ] {
        if (x - hx).abs() <= pick && (y - hy).abs() <= pick {
            return Some(h);
        }
    }
    None
}

/// 左键按下的命中目标（ADR 0023）：决定这次按下是「绘制」还是「移动 / 缩放」，
/// 也是光标形状的唯一依据（两者必须一致，否则「看到的光标 ≠ 按下会发生的事」）。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum PickTarget {
    /// 什么都没命中：选区内框选、选区外重新框选。
    None,
    /// 选中对象的控制柄 → 缩放 / 圆角 / 拉端点。
    ObjHandle(usize, objedit::ObjHandle),
    /// 曲线控制柄 → 改弯曲。
    CurveCtrl(usize),
    /// 选区八向手柄 → 缩放选区。
    SelHandle(Handle),
    /// 空格 + 选区内 → 移动整个选区。
    MoveSelection,
    /// 对象本体（Ctrl 按下，或命中选中对象的包围框边缘）→ 移动该对象。
    ObjBody(usize),
    /// 命中对象内部但要走绘制通道：拖动 = 画新标注，无位移松手 = 选中该对象。
    Inside(usize),
    /// 选区内空白：直接绘制。
    InsideEmpty,
}

/// 包围框边缘环带命中（ADR 0023 §2）：`frame_of` 外扩 / 内缩 `pick_radius` 的框线，
/// 宽或高 ≤ 2×`pick_radius` 时退化整块（否则极小对象抓不到边、无法拖动）。
/// 箭头与直线不画包围框（只有两端点手柄），故不参与。
pub(in crate::overlay) fn hit_frame_edge(obj: &Object, x: i32, y: i32, ui: Ui) -> bool {
    if matches!(obj.kind, Kind::Arrow { .. } | Kind::Line { .. }) {
        return false;
    }
    let f = objedit::frame_of(obj);
    let pick = pick_radius(ui);
    // 整块包围盒（含外扩）：小对象直接判定整块
    let (ox, oy) = (f.x - pick, f.y - pick);
    let (ow, oh) = ((f.w + 2 * pick).max(0), (f.h + 2 * pick).max(0));
    if !(x >= ox && x < ox + ow && y >= oy && y < oy + oh) {
        return false;
    }
    if f.w <= 2 * pick || f.h <= 2 * pick {
        return true;
    }
    // 环带 = 外框 − 内框（内框为包围盒内缩 pick）
    let inner = SelRect {
        x: f.x + pick,
        y: f.y + pick,
        w: f.w - 2 * pick,
        h: f.h - 2 * pick,
    };
    !inner.contains(x, y)
}

/// 误触判定：长宽均不足 2px 的对象丢弃（EDT-1/EDT-2）。
pub(super) fn is_degenerate(obj: &Object) -> bool {
    match &obj.kind {
        Kind::Rect { a, b, .. } => (a.x - b.x).abs() < 2.0 && (a.y - b.y).abs() < 2.0,
        Kind::Arrow { from, to, .. } => (from.x - to.x).abs() < 2.0 && (from.y - to.y).abs() < 2.0,
        // 区域类（高亮/模糊）：过小视为误触
        Kind::Highlight { a, b, .. } => (a.x - b.x).abs() < 2.0 && (a.y - b.y).abs() < 2.0,
        Kind::Blur { a, b, .. } => (a.x - b.x).abs() < 2.0 && (a.y - b.y).abs() < 2.0,
        Kind::Glow { a, b, .. } => (a.x - b.x).abs() < 2.0 && (a.y - b.y).abs() < 2.0,
        // 文本：空文本在提交时已拦截，进入文档的均为有效对象
        Kind::Text { .. } => false,
        // 序号标记（EDT-12）：只点一下不拖也是有效对象（只有徽标、无引线）
        Kind::Number { .. } => false,
        // EDT-13 直线：两端点过近视为误触（同箭头判据）
        Kind::Line { from, to } => (from.x - to.x).abs() < 2.0 && (from.y - to.y).abs() < 2.0,
        // EDT-14 椭圆：**任一轴**退化即视为误触。与矩形不同（矩形退化后仍可命中、可删），
        // 椭圆在任一轴 < 1px 时 `draw_ellipse` 不画、`hit_ellipse` 不命中，留着只能靠撤销清掉。
        Kind::Ellipse { a, b, .. } => (a.x - b.x).abs() < 2.0 || (a.y - b.y).abs() < 2.0,
        // EDT-15 手绘：点数 < 2 或总路径长度过短视为误触
        Kind::Freehand { pts } => {
            let len: f32 = pts
                .as_slice()
                .windows(2)
                .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
                .sum();
            pts.len() < 2 || len < 2.0
        }
    }
}

impl super::Overlay {
    /// 已选对象。
    pub(super) fn selected_object(&self) -> Option<&Object> {
        self.doc.objects().get(self.selected?)
    }

    /// 切换选中对象；控制柄与辅助线随之变化，故重绘整个选区（只在点选时触发）。
    pub(super) fn set_selected(&mut self, new: Option<usize>) {
        if self.selected == new {
            return;
        }
        let had = self.selected.is_some();
        self.selected = new;
        // 选中文本对象时第二行才出现（EDT-4）
        self.refresh_toolbar();
        if (had || new.is_some())
            && let Some(sel) = self.selection
        {
            self.repaint(expand_rect(sel, self.dirty_pad()));
        }
    }

    /// 命中已有对象：从上层往下找（列表顺序即 z 序，EDT-7）。
    /// 「整块命中」只服务对象级操作（双击编辑文本 / 恢复直线、滚轮调属性、单击选中）；
    /// 左键按下时「移动还是绘制」由 `pick_target` 决定（ADR 0023）。
    pub(super) fn hit_object(&self, x: i32, y: i32) -> Option<usize> {
        let q = Point::new(x as f32, y as f32);
        let tol = pick_radius(self.ui) as f32;
        self.doc.objects().iter().rposition(|o| o.hit(q, tol))
    }

    /// 左键按下时的命中目标（ADR 0023）。
    pub(super) fn pick_target(&self, x: i32, y: i32, ctrl: bool) -> PickTarget {
        let ui = self.ui;
        // 1) 选中对象的控制柄（EDT-7）：可能压在选区装饰或对象描边上，最优先
        if let Some((index, handle)) = self
            .selected
            .and_then(|i| self.doc.objects().get(i).map(|o| (i, o)))
            .and_then(|(i, o)| objedit::hit_handle(o, x, y, ui).map(|h| (i, h)))
        {
            return PickTarget::ObjHandle(index, handle);
        }
        // 2) 曲线控制柄（EDT-3）：控制点常落在自己线身上，需先于「选中对象」判定
        if let Some(index) = self.hit_control(x, y) {
            return PickTarget::CurveCtrl(index);
        }
        let Some(sel) = self.selection else {
            return PickTarget::None;
        };
        // 3) 选区八向手柄（CAP-2）；贴图编辑模式（ADR 0028）选区锁定，不提供手柄
        if !self.locked_selection
            && let Some(h) = hit_handle(sel, x, y, ui)
        {
            return PickTarget::SelHandle(h);
        }
        if !sel.contains(x, y) {
            return PickTarget::None;
        }
        // 4) 空格 + 选区内：移动整个选区（CAP-2），优先于对象；锁定态禁用
        if !self.locked_selection && super::wndproc::space_down() {
            return PickTarget::MoveSelection;
        }
        // 5) Ctrl + 对象本体：移动该对象（ADR 0023 §2）
        if let Some(index) = self.hit_object(x, y) {
            if ctrl {
                return PickTarget::ObjBody(index);
            }
            // 6) 选中对象的包围框边缘：移动（框外靠描边选中的箭头也走这条）
            if let Some(i) = self.selected
                && self.doc.objects().get(i).is_some_and(|o| hit_frame_edge(o, x, y, ui))
            {
                return PickTarget::ObjBody(i);
            }
            // 7) 其余命中对象内部：绘制优先（ADR 0023 §1）
            return PickTarget::Inside(index);
        }
        // 8) 选区内空白：用当前工具绘制
        PickTarget::InsideEmpty
    }

    /// 选中对象占据的像素区域。
    pub(super) fn selected_clip(&self) -> Option<SelRect> {
        let c = crate::render::bounds_to_clip(&self.selected_object()?.bounds());
        Some(SelRect {
            x: c.0,
            y: c.1,
            w: c.2 - c.0,
            h: c.3 - c.1,
        })
    }
}

/// 选中箭头的控制柄（EDT-3）：控制点处画圆点，与端点之间用虚线辅助线连接。
pub(super) fn draw_curve_handles(pixmap: &mut Pixmap, obj: &Object, ui: Ui) {
    let (Some(cps), Kind::Arrow { from, to, .. }) = (obj.kind.control_points(), &obj.kind) else {
        return;
    };
    let [c1, c2] = cps;
    dashed_line(pixmap, *from, c1, ui);
    dashed_line(pixmap, c2, *to, ui);
    for p in [c1, c2] {
        let mut pb = PathBuilder::new();
        pb.push_circle(p.x, p.y, ui.f(CTRL_RADIUS_BASE));
        let Some(path) = pb.finish() else {
            continue;
        };
        let mut fill = Paint::default();
        fill.set_color_rgba8(255, 255, 255, 255);
        pixmap.fill_path(&path, &fill, tiny_skia::FillRule::Winding, Transform::identity(), None);
        let mut edge = Paint::default();
        edge.set_color_rgba8(ACCENT, ACCENT_G, ACCENT_B, 255);
        let stroke = Stroke {
            width: ui.f(1.5),
            ..Default::default()
        };
        pixmap.stroke_path(&path, &edge, &stroke, Transform::identity(), None);
    }
}

/// 虚线辅助线（5px 实 / 4px 虚）：tiny-skia 无 dash，手动拆成小段。
pub(super) fn dashed_line(pixmap: &mut Pixmap, a: Point, b: Point, ui: Ui) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    // 实线段与周期按界面缩放，避免 4K 下虚线看起来是实线
    let (dash, period) = (ui.f(5.0), ui.f(9.0));
    let mut pb = PathBuilder::new();
    let mut t = 0.0;
    while t < len {
        let end = (t + dash).min(len);
        pb.move_to(a.x + ux * t, a.y + uy * t);
        pb.line_to(a.x + ux * end, a.y + uy * end);
        t += period;
    }
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 200);
    let stroke = Stroke {
        width: ui.f(1.0),
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Kind, Object, Point, Style};

    /// 基准缩放（100% DPI）：`pick_radius` = 6px。
    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    fn rect_like(kind: Kind) -> Object {
        Object {
            kind,
            style: Style::new([255, 0, 0, 255], 2.0),
        }
    }

    /// 大矩形：框边环带命中、内部不命中（内部要留给绘制，ADR 0023）。
    #[test]
    fn frame_edge_hits_border_band_only() {
        let (a, b) = (Point::new(50.0, 50.0), Point::new(250.0, 150.0));
        let obj = rect_like(Kind::Rect {
            a,
            b,
            radius: 0.0,
            filled: false,
        });
        let ui = base();
        let f = objedit::frame_of(&obj);
        // 左边框上（含外扩 6px 内）→ 命中
        assert!(hit_frame_edge(&obj, f.x, f.y + f.h / 2, ui), "边框上应命中");
        assert!(hit_frame_edge(&obj, f.x - 5, f.y + f.h / 2, ui), "边框外 5px 应命中");
        assert!(hit_frame_edge(&obj, f.x, f.y, ui), "左上角应命中");
        // 内部（距边 10px 以上）→ 不命中
        assert!(!hit_frame_edge(&obj, f.x + f.w / 2, f.y + f.h / 2, ui), "内部不应命中");
        // 外侧更远 → 不命中
        assert!(!hit_frame_edge(&obj, f.x - 20, f.y + f.h / 2, ui), "远处不应命中");
    }

    /// 极小对象（宽或高 ≤ 2×pick）退化为整块：否则抓不到边、无法拖动。
    #[test]
    fn tiny_object_hits_whole_block() {
        let obj = rect_like(Kind::Glow {
            a: Point::new(100.0, 100.0),
            b: Point::new(108.0, 108.0),
            corner: 0.0,
        });
        let ui = base();
        let f = objedit::frame_of(&obj);
        assert!(hit_frame_edge(&obj, f.x + 2, f.y + 2, ui), "小对象内部也应算框边");
    }

    /// 区域类对象（高亮 / 模糊 / 聚光）都有自己的包围框，框边可拖动。
    #[test]
    fn area_kinds_have_draggable_frame() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(200.0, 100.0));
        for kind in [
            Kind::Highlight { a, b, corner: 0.0 },
            Kind::Blur {
                a,
                b,
                radius: 16.0,
                corner: 0.0,
            },
            Kind::Glow { a, b, corner: 0.0 },
        ] {
            let obj = rect_like(kind);
            let f = objedit::frame_of(&obj);
            assert!(
                hit_frame_edge(&obj, f.x, f.y + f.h / 2, base()),
                "{obj:?} 的框边应可拖动"
            );
        }
    }

    /// 序号标记（EDT-12）：只点一下不拖（只有徽标）也是有效对象，不算误触。
    #[test]
    fn number_is_never_degenerate() {
        let obj = rect_like(Kind::Number {
            center: Point::new(10.0, 10.0),
            tip: Point::new(10.0, 10.0),
            n: 1,
            font_size: 20.0,
            radius: 14.0,
        });
        assert!(!is_degenerate(&obj), "序号徽标不应被判为误触丢弃");
    }

    /// 箭头只画两端点手柄、不画包围框，不参与框边命中（否则整条框都能拖）。
    #[test]
    fn arrow_has_no_frame_edge() {
        let obj = rect_like(Kind::arrow(Point::new(0.0, 0.0), Point::new(100.0, 100.0)));
        assert!(!hit_frame_edge(&obj, 0, 0, base()), "箭头不应有框边命中");
    }

    /// EDT-13 直线与箭头一致：只有两端点手柄，不画包围框，不参与框边命中。
    #[test]
    fn line_has_no_frame_edge() {
        let obj = rect_like(Kind::Line {
            from: Point::new(0.0, 0.0),
            to: Point::new(100.0, 100.0),
        });
        assert!(!hit_frame_edge(&obj, 0, 0, base()), "直线不应有框边命中");
        let f = objedit::frame_of(&obj);
        assert!(
            !hit_frame_edge(&obj, f.x + f.w / 2, f.y, base()),
            "直线上边中点不应可拖框"
        );
    }

    /// EDT-13/14/15：新工具的误触判定（过小直线 / 过小椭圆 / 点太少或路径过短的手绘）。
    #[test]
    fn new_tools_degenerate_detection() {
        let tiny_line = rect_like(Kind::Line {
            from: Point::new(10.0, 10.0),
            to: Point::new(11.0, 11.0),
        });
        assert!(is_degenerate(&tiny_line), "过短直线应视为误触");
        let line = rect_like(Kind::Line {
            from: Point::new(10.0, 10.0),
            to: Point::new(90.0, 10.0),
        });
        assert!(!is_degenerate(&line), "正常直线不应丢弃");

        let tiny_ellipse = rect_like(Kind::Ellipse {
            a: Point::new(10.0, 10.0),
            b: Point::new(11.0, 11.0),
            filled: false,
        });
        assert!(is_degenerate(&tiny_ellipse), "过小椭圆应视为误触");
        // 单轴退化：横向拖得再长也画不出东西（`draw_ellipse` / `hit_ellipse` 都退），
        // 必须按误触丢弃，否则会留下既看不见、也选不中的幽灵对象。
        for flat in [
            (Point::new(10.0, 10.0), Point::new(300.0, 10.0)), // 高度 0
            (Point::new(10.0, 10.0), Point::new(10.0, 300.0)), // 宽度 0
            (Point::new(10.0, 10.0), Point::new(300.0, 10.5)), // 高度不足 1px（渲染退化）
        ] {
            let obj = rect_like(Kind::Ellipse {
                a: flat.0,
                b: flat.1,
                filled: false,
            });
            assert!(is_degenerate(&obj), "单轴退化椭圆应视为误触：{:?}", flat);
        }
        let ellipse = rect_like(Kind::Ellipse {
            a: Point::new(10.0, 10.0),
            b: Point::new(90.0, 60.0),
            filled: false,
        });
        assert!(!is_degenerate(&ellipse), "正常椭圆不应丢弃");

        let single = rect_like(Kind::Freehand {
            pts: std::sync::Arc::new(vec![Point::new(10.0, 10.0)]),
        });
        assert!(is_degenerate(&single), "单点手绘应视为误触");
        let short = rect_like(Kind::Freehand {
            pts: std::sync::Arc::new(vec![Point::new(10.0, 10.0), Point::new(10.5, 10.5)]),
        });
        assert!(is_degenerate(&short), "路径过短的手绘应视为误触");
        let hand = rect_like(Kind::Freehand {
            pts: std::sync::Arc::new(vec![
                Point::new(10.0, 10.0),
                Point::new(30.0, 20.0),
                Point::new(60.0, 50.0),
            ]),
        });
        assert!(!is_degenerate(&hand), "正常手绘不应丢弃");
    }

    /// 命中半径随界面缩放放大（CAP-5）：100% 下 8px 外不命中，200% 下命中。
    #[test]
    fn frame_edge_scales_with_dpi() {
        let obj = rect_like(Kind::Rect {
            a: Point::new(50.0, 50.0),
            b: Point::new(250.0, 150.0),
            radius: 0.0,
            filled: false,
        });
        let f = objedit::frame_of(&obj);
        let (x, y) = (f.x - 8, f.y + f.h / 2);
        assert!(!hit_frame_edge(&obj, x, y, base()), "100% 下 8px 外不应命中");
        assert!(hit_frame_edge(&obj, x, y, Ui::from_dpi(192)), "200% 下 8px 内应命中");
    }
}
