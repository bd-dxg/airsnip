// SPDX-License-Identifier: GPL-3.0-only
//! 一帧的合成与上屏（CAP-2/CAP-3、EDT-1…EDT-15）。
//!
//! `display` = 暗化底图 → 选区恢复原亮度 → 标注对象（core 绘制层）→ 选区装饰。
//! 每个输出的 layer surface 只做一次「按位置拷贝 + B/R 交换」，
//! 因此拖拽时的成本与屏幕像素数成正比，与标注对象数量几乎无关。
//!
//! `wl_shm` 的 `Argb8888` 在小端机内存序是 B,G,R,A，而 tiny-skia 是 R,G,B,A，
//! 所以拷贝那一层必须换通道。

use tiny_skia::{Paint, PathBuilder, Rect, Stroke, Transform};

use super::state::{Overlay, Sel};
use crate::Error;

/// 主题蓝（与 Windows 版 `selection::draw` 的 ACCENT 一致）。
const ACCENT: (u8, u8, u8) = (26, 115, 232);
/// 手柄边长（物理像素，基准 7px @100%）。
const HANDLE: f32 = 7.0;

/// 重绘所有图层（选区 / 标注变化后调用）。
pub fn paint_all(state: &mut crate::wayland::Backend) {
    let n = state.overlay.as_ref().map(|o| o.layers.len()).unwrap_or(0);
    for i in 0..n {
        paint_layer(state, i);
    }
}

/// 重算 `display`，然后重绘所有图层。
pub fn refresh_all(state: &mut crate::wayland::Backend) {
    if let Some(ov) = state.overlay.as_mut() {
        rebuild(ov);
    }
    paint_all(state);
}

/// 重算 `display`（暗化 + 选区 + 标注 + 装饰）。
pub fn rebuild(ov: &mut Overlay) {
    let Some(sel) = ov.current_sel() else {
        ov.display = ov.dim.clone();
        return;
    };
    {
        let Overlay {
            bright,
            dim,
            display,
            doc,
            draft,
            text_edit,
            ..
        } = ov;
        display.clone_from(dim);
        // 聚光（Glow）语义与 Windows 版一致：**选区内整体保持暗化**，
        // 只把每块聚光区域恢复成原图亮度（`Kind::Glow` 在对象绘制里是空实现，
        // 效果由这条管线提供；导出时由 `draw::bake` 做同样的事）。
        let glow_on = doc
            .objects()
            .iter()
            .chain(draft.iter())
            .any(|o| matches!(o.kind, airsnip_core::editor::Kind::Glow { .. }));
        if glow_on {
            for obj in doc.objects().iter().chain(draft.iter()) {
                if let airsnip_core::editor::Kind::Glow { a, b, corner } = &obj.kind {
                    restore_glow(display, bright, *a, *b, *corner);
                }
            }
        } else {
            copy_block(display, bright, sel.x, sel.y, sel.w, sel.h, sel.x, sel.y);
        }
        let clip = (sel.x, sel.y, sel.x + sel.w, sel.y + sel.h);
        crate::draw::draw_objects(display, doc.objects(), draft.as_ref(), clip);
        // 选中对象的编辑框（虚线框 + 手柄），仅上屏不导出
        if let Some(idx) = ov.selected
            && let Some(obj) = doc.objects().get(idx)
        {
            super::objedit::paint::frame(display, obj);
            super::objedit::paint::curve_handles(display, obj);
            super::objedit::paint::radius_handle(display, obj);
        }
        // 正在输入的文本（光标提示）画在所有对象之上
        if let Some(edit) = text_edit {
            super::text_input::paint(display, edit, sel.w as f32);
        }
    }
    let (cw, ch) = (ov.display.width() as i32, ov.display.height() as i32);
    let tb = super::toolbar::layout(cw, ch, sel);
    draw_size_hint(&mut ov.display, sel, tb.y, ch);
    draw_decorations(&mut ov.display, sel);
    // 工具栏（选区下方优先，塞不下翻上方）
    let (tool, color_index, filled, pointer) = (ov.tool, ov.color_index, ov.filled, ov.pointer);
    super::toolbar::paint(&mut ov.display, &tb, tool, color_index, filled, pointer);
}

/// 把源图的一块搬到目标图的指定位置。
///
/// 源矩形与目标偏移**分开传**：裁剪导出时目标从 (0,0) 开始而源在选区坐标，
/// 早先版本让两者共用同一组坐标，导致裁剪图整块被跳过（导出全空白）。
fn copy_block(
    dst: &mut tiny_skia::Pixmap,
    src: &tiny_skia::Pixmap,
    sx: i32,
    sy: i32,
    w: i32,
    h: i32,
    dx: i32,
    dy: i32,
) {
    let (sw, sh) = (src.width() as i32, src.height() as i32);
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    // 与源、与目标都要钳一次
    let x0 = sx.max(0);
    let y0 = sy.max(0);
    let x1 = (sx + w).min(sw);
    let y1 = (sy + h).min(sh);
    let tx = dx + (x0 - sx);
    let ty = dy + (y0 - sy);
    let n = (x1 - x0).min(dw - tx);
    let rows = (y1 - y0).min(dh - ty);
    if n <= 0 || rows <= 0 {
        return;
    }
    let row_bytes = n as usize * 4;
    for r in 0..rows {
        let so = (((y0 + r) as usize) * src.width() as usize + x0 as usize) * 4;
        let do_ = (((ty + r) as usize) * dst.width() as usize + tx as usize) * 4;
        let bytes = &src.data()[so..so + row_bytes];
        dst.data_mut()[do_..do_ + row_bytes].copy_from_slice(bytes);
    }
}

/// 选区尺寸提示（CAP-2）：「宽 × 高」画在工具栏的**对面**一侧（工具栏在下 → 文字在上）。
///
/// 用 16px 粗体 + 1px 黑色阴影：纯白字在浅色底图上会看不清，加阴影后深浅都可用。
fn draw_size_hint(pixmap: &mut tiny_skia::Pixmap, sel: Sel, toolbar_y: i32, canvas_h: i32) {
    use airsnip_core::draw::text::{TextSink, TextSpec};
    let label = format!("{} × {}", sel.w, sel.h);
    let spec = TextSpec {
        family: std::sync::Arc::from(super::text_input::DEFAULT_FAMILY),
        size: 16.0,
        bold: true,
        italic: false,
    };
    crate::draw::text::with_engine(|sink| {
        let (_, th) = sink.measure(&label, &spec);
        // 工具栏在选区下方 → 文字放上方；反之放下方
        let toolbar_below = toolbar_y >= sel.y + sel.h;
        let mut y = if toolbar_below {
            sel.y as f32 - th - 6.0
        } else {
            (sel.y + sel.h) as f32 + 6.0
        };
        // 上方放不下就翻到下方，再不行贴边
        if y < 0.0 {
            y = (sel.y + sel.h) as f32 + 6.0;
        }
        y = y.clamp(0.0, (canvas_h as f32 - th).max(0.0));
        let x = (sel.x as f32 + 2.0).clamp(0.0, (pixmap.width() as f32 - 4.0).max(0.0));
        let pos = airsnip_core::editor::Point::new(x, y);
        // 阴影 + 白字
        sink.draw(
            pixmap,
            airsnip_core::editor::Point::new(x + 1.0, y + 1.0),
            &label,
            &spec,
            [0, 0, 0, 210],
            tiny_skia::Transform::identity(),
        );
        sink.draw(
            pixmap,
            pos,
            &label,
            &spec,
            [255, 255, 255, 255],
            tiny_skia::Transform::identity(),
        );
    });
}

/// 聚光区域恢复原亮度（`corner > 0` 时四角按覆盖率插值，避免硬边）。
fn restore_glow(
    display: &mut tiny_skia::Pixmap,
    bright: &tiny_skia::Pixmap,
    a: airsnip_core::editor::Point,
    b: airsnip_core::editor::Point,
    corner: f32,
) {
    let (fx0, fy0) = (a.x.min(b.x), a.y.min(b.y));
    let (fx1, fy1) = (a.x.max(b.x), a.y.max(b.y));
    let (x0, y0) = (fx0.floor() as i32, fy0.floor() as i32);
    let (x1, y1) = (fx1.ceil() as i32, fy1.ceil() as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let full = (fx0, fy0, fx1, fy1);
    let r = airsnip_core::draw::mask::clamp_radius(full, corner);
    if r < 0.5 {
        copy_block(display, bright, x0, y0, x1 - x0, y1 - y0, x0, y0);
        return;
    }
    let region = (x0, y0, x1, y1);
    let buf = airsnip_core::draw::mask::copy_region(bright, region);
    airsnip_core::draw::mask::blend_compact(display, &buf, region, full, r);
}

/// 选区装饰：2px 主题蓝 + 1px 白色内线 + 八向手柄。
fn draw_decorations(p: &mut tiny_skia::Pixmap, sel: Sel) {
    let x0 = sel.x as f32 + 0.5;
    let y0 = sel.y as f32 + 0.5;
    let w = (sel.w as f32 - 1.0).max(1.0);
    let h = (sel.h as f32 - 1.0).max(1.0);
    let Some(rect) = Rect::from_xywh(x0, y0, w, h) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(rect);
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(ACCENT.0, ACCENT.1, ACCENT.2, 255);
    let stroke = Stroke {
        width: 2.0,
        ..Default::default()
    };
    p.stroke_path(&path, &paint, &stroke, Transform::identity(), None);

    // 内侧白线（观感与 Windows 版一致）
    if let Some(inner) = Rect::from_xywh(x0 + 2.0, y0 + 2.0, (w - 4.0).max(1.0), (h - 4.0).max(1.0)) {
        let mut pb = PathBuilder::new();
        pb.push_rect(inner);
        if let Some(path) = pb.finish() {
            let mut white = Paint::default();
            white.set_color_rgba8(255, 255, 255, 180);
            let thin = Stroke {
                width: 1.0,
                ..Default::default()
            };
            p.stroke_path(&path, &white, &thin, Transform::identity(), None);
        }
    }

    // 八向手柄：白底 + 蓝色描边
    let cx = x0 + w / 2.0;
    let cy = y0 + h / 2.0;
    let points = [
        (x0, y0),
        (cx, y0),
        (x0 + w, y0),
        (x0, cy),
        (x0 + w, cy),
        (x0, y0 + h),
        (cx, y0 + h),
        (x0 + w, y0 + h),
    ];
    for (px, py) in points {
        let Some(r) = Rect::from_xywh(px - HANDLE / 2.0, py - HANDLE / 2.0, HANDLE, HANDLE) else {
            continue;
        };
        let mut pb = PathBuilder::new();
        pb.push_rect(r);
        if let Some(path) = pb.finish() {
            let mut outline = Paint::default();
            outline.set_color_rgba8(ACCENT.0, ACCENT.1, ACCENT.2, 255);
            p.stroke_path(
                &path,
                &outline,
                &Stroke {
                    width: 1.0,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
            let mut fill = Paint::default();
            fill.set_color_rgba8(255, 255, 255, 255);
            p.fill_path(&path, &fill, tiny_skia::FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// 重绘单个图层（从 `display` 拷贝到该输出的缓冲）。
pub fn paint_layer(state: &mut crate::wayland::Backend, layer_idx: usize) {
    let Some(overlay) = state.overlay.as_mut() else {
        return;
    };
    let Some(layer) = overlay.layers.get_mut(layer_idx) else {
        return;
    };
    let geom = layer.geom;
    let Some(bi) = layer.take_buffer_index() else {
        return;
    };
    {
        let display = &overlay.display;
        let buf = &mut layer.bufs[bi];
        blit_to_shm(buf, display, geom.phys.0, geom.phys.1);
    }
    layer.present(&layer.bufs[bi]);
}

/// 把 `display` 的一块拷进 shm 缓冲（RGBA → BGRA）。
fn blit_to_shm(buf: &mut super::surface::Buf, src: &tiny_skia::Pixmap, ox: i32, oy: i32) {
    let (bw, bh) = (buf.w, buf.h);
    let (sw, sh) = (src.width() as i32, src.height() as i32);
    let pixels = buf.pixels_mut();
    for y in 0..bh {
        let sy = oy + y;
        if sy < 0 || sy >= sh {
            continue;
        }
        for x in 0..bw {
            let sx = ox + x;
            if sx < 0 || sx >= sw {
                continue;
            }
            let si = ((sy as usize) * src.width() as usize + sx as usize) * 4;
            let di = (y as usize * bw as usize + x as usize) * 4;
            let s = &src.data()[si..si + 4];
            pixels[di] = s[2];
            pixels[di + 1] = s[1];
            pixels[di + 2] = s[0];
            pixels[di + 3] = 255;
        }
    }
}

/// 由选区和标注生成导出图（裁剪 + 烘焙）。
pub fn export(ov: &Overlay, sel: Sel) -> Result<crate::canvas::Canvas, Error> {
    let mut cropped = tiny_skia::Pixmap::new(sel.w as u32, sel.h as u32)
        .ok_or_else(|| -> Error { "导出画布分配失败".into() })?;
    copy_block(&mut cropped, &ov.bright, sel.x, sel.y, sel.w, sel.h, 0, 0);
    crate::draw::bake(&mut cropped, ov.doc.objects(), (sel.x, sel.y));
    Ok(crate::draw::pixmap_to_canvas(&cropped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::state::{Overlay, Sel};

    /// 用已知图案构造画布：像素值 = (x, y) 的低字节，便于验证裁剪位置。
    fn patterned(w: u32, h: u32) -> crate::canvas::Canvas {
        let mut c = crate::canvas::Canvas::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                c.px[i] = (x % 256) as u8;
                c.px[i + 1] = (y % 256) as u8;
                c.px[i + 2] = 200;
                c.px[i + 3] = 255;
            }
        }
        c
    }

    #[test]
    fn export_crops_the_selected_region() {
        let canvas = patterned(400, 300);
        let mut ov = Overlay::new(&canvas).unwrap();
        let sel = Sel {
            x: 100,
            y: 80,
            w: 120,
            h: 60,
        };
        ov.sel = Some(sel);
        let out = export(&ov, sel).unwrap();
        assert_eq!((out.w, out.h), (120, 60), "裁剪尺寸应等于选区");
        // 左上角应等于原图 (100,80) 的像素，而不是全 0（早先的 bug 就是整块空白）
        assert_eq!(&out.px[0..3], &[100, 80, 200]);
        let i = ((59 * 120 + 119) * 4) as usize;
        assert_eq!(&out.px[i..i + 3], &[219, 139, 200], "右下角对应原图 (219,139)");
    }

    #[test]
    fn export_bakes_annotations_on_top() {
        let canvas = patterned(200, 200);
        let mut ov = Overlay::new(&canvas).unwrap();
        let sel = Sel {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        };
        ov.sel = Some(sel);
        ov.tool = airsnip_core::editor::Tool::Rect;
        ov.filled = true;
        ov.style = airsnip_core::editor::Style::new([255, 0, 0, 255], 4.0);
        crate::overlay::edit::begin(&mut ov, (20, 20));
        crate::overlay::edit::update(&mut ov, (180, 180), false);
        crate::overlay::edit::commit(&mut ov);
        let out = export(&ov, sel).unwrap();
        let i = ((100 * 200 + 100) * 4) as usize;
        assert_eq!(&out.px[i..i + 3], &[255, 0, 0], "填充矩形应盖住底图");
    }

    /// 十种工具都在画：确认 highlight / blur / glow / text 真的落到像素上。
    #[test]
    fn every_tool_kind_renders() {
        use airsnip_core::draw::text::TextSink;
        use airsnip_core::editor::{Kind, Object, Point, Style};
        let mut canvas = crate::canvas::Canvas::new(600, 400);
        // 条纹底图（模糊/高亮/聚光都需要「有内容」才看得出来）
        for y in 0..400u32 {
            for x in 0..600u32 {
                let i = ((y * 600 + x) * 4) as usize;
                let v = if ((x / 12) + (y / 12)) % 2 == 0 { 200 } else { 60 };
                canvas.px[i] = v as u8;
                canvas.px[i + 1] = (v as u8).wrapping_add(30);
                canvas.px[i + 2] = (v as u8).wrapping_sub(20);
                canvas.px[i + 3] = 255;
            }
        }
        let mut ov = Overlay::new(&canvas).unwrap();
        let sel = Sel {
            x: 0,
            y: 0,
            w: 600,
            h: 400,
        };
        ov.sel = Some(sel);
        let obj = |kind: Kind, color: [u8; 4], width: f32| Object {
            kind,
            style: Style::new(color, width),
        };
        let spec = airsnip_core::draw::text::TextSpec::new(28.0);
        let size = crate::draw::text::with_engine(|sink| sink.measure("Text 文本", &spec));
        ov.doc.push(obj(
            Kind::Rect {
                a: Point::new(20.0, 20.0),
                b: Point::new(140.0, 90.0),
                radius: 6.0,
                filled: false,
            },
            [229, 57, 53, 255],
            4.0,
        ));
        ov.doc.push(obj(
            Kind::arrow(Point::new(160.0, 30.0), Point::new(300.0, 90.0)),
            [229, 57, 53, 255],
            4.0,
        ));
        // 高亮：橙色正片叠底
        ov.doc.push(obj(
            Kind::Highlight {
                a: Point::new(20.0, 110.0),
                b: Point::new(200.0, 170.0),
                corner: 0.0,
            },
            [251, 140, 0, 255],
            4.0,
        ));
        // 模糊
        ov.doc.push(obj(
            Kind::Blur {
                a: Point::new(220.0, 110.0),
                b: Point::new(340.0, 170.0),
                radius: 16.0,
                corner: 0.0,
            },
            [0, 0, 0, 255],
            4.0,
        ));
        // 聚光：这块钱应该恢复原亮度，其余选区暗化
        ov.doc.push(obj(
            Kind::Glow {
                a: Point::new(380.0, 110.0),
                b: Point::new(560.0, 170.0),
                corner: 0.0,
            },
            [0, 0, 0, 255],
            4.0,
        ));
        ov.doc.push(obj(
            Kind::Number {
                center: Point::new(60.0, 260.0),
                tip: Point::new(120.0, 300.0),
                n: 1,
                font_size: 20.0,
                radius: 16.0,
            },
            [229, 57, 53, 255],
            4.0,
        ));
        ov.doc.push(obj(
            Kind::Line {
                from: Point::new(150.0, 250.0),
                to: Point::new(260.0, 300.0),
            },
            [67, 160, 71, 255],
            4.0,
        ));
        ov.doc.push(obj(
            Kind::Ellipse {
                a: Point::new(300.0, 240.0),
                b: Point::new(400.0, 310.0),
                filled: true,
            },
            [30, 136, 229, 255],
            4.0,
        ));
        ov.doc.push(obj(
            Kind::Freehand {
                pts: std::sync::Arc::new(vec![
                    Point::new(430.0, 250.0),
                    Point::new(470.0, 300.0),
                    Point::new(520.0, 260.0),
                ]),
            },
            [255, 255, 255, 255],
            4.0,
        ));
        ov.doc.push(obj(
            Kind::Text {
                pos: Point::new(30.0, 330.0),
                text: std::sync::Arc::from("Text 文本"),
                font_size: 28.0,
                font_family: std::sync::Arc::from(super::super::text_input::DEFAULT_FAMILY),
                bold: false,
                italic: false,
                size,
            },
            [255, 255, 255, 255],
            4.0,
        ));
        rebuild(&mut ov);

        let px = |x: u32, y: u32| -> [u8; 3] {
            let i = ((y * 600 + x) * 4) as usize;
            [ov.display.data()[i], ov.display.data()[i + 1], ov.display.data()[i + 2]]
        };
        // 高亮区域应偏橙（红通道明显高于蓝）
        let hl = px(100, 140);
        assert!(hl[0] > hl[2], "高亮应把像素染向橙色：{hl:?}");
        // 聚光区域应恢复成**原图**亮度；选区其它地方应是暗化后的值
        let orig = |x: u32, y: u32| -> [u8; 3] {
            let i = ((y * 600 + x) * 4) as usize;
            [canvas.px[i], canvas.px[i + 1], canvas.px[i + 2]]
        };
        assert_eq!(px(480, 140), orig(480, 140), "聚光区应恢复原亮度");
        let dimpx = px(210, 190);
        let o = orig(210, 190);
        let expect = |v: u8| ((v as u32 * crate::overlay::state::DIM_K) >> 8) as u8;
        assert_eq!(dimpx[0], expect(o[0]), "选区其余部分应为暗化值");
        // 模糊区域应比条纹边界平滑（取相邻两像素，差值小于底图的强弱差）
        let a = px(240, 140);
        let b = px(252, 140);
        let d = (a[0] as i32 - b[0] as i32).abs();
        assert!(d < 60, "模糊后相邻像素差应减小：{a:?} {b:?} 差 {d}");
        let sum = |p: [u8; 3]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        // 文本区域应有亮像素
        let mut bright = 0;
        for y in 330..360 {
            for x in 30..160 {
                if sum(px(x, y)) > 600 {
                    bright += 1;
                }
            }
        }
        assert!(bright > 40, "文本应画出可见字形，实际亮像素={bright}");

        if std::env::var_os("AIRSNIP_DEBUG").is_some() {
            let c = crate::draw::pixmap_to_canvas(&ov.display);
            crate::codec::write_png(&c, std::path::Path::new("/tmp/airsnip-tools.png")).unwrap();
        }
    }

    #[test]
    fn shift_squares_the_rect() {
        use airsnip_core::editor::Tool;
        let canvas = crate::canvas::Canvas::new(300, 300);
        let mut ov = Overlay::new(&canvas).unwrap();
        ov.sel = Some(Sel {
            x: 0,
            y: 0,
            w: 300,
            h: 300,
        });
        ov.tool = Tool::Rect;
        crate::overlay::edit::begin(&mut ov, (20, 20));
        crate::overlay::edit::update(&mut ov, (200, 80), true);
        let kind = &ov.draft.as_ref().unwrap().kind;
        let airsnip_core::editor::Kind::Rect { a, b, .. } = kind else {
            panic!("应是矩形")
        };
        assert!(
            ((a.x - b.x).abs() - (a.y - b.y).abs()).abs() < 1.5,
            "Shift 应把矩形变成正方形：{:?} {:?}",
            a,
            b
        );
    }

    #[test]
    fn shift_snaps_the_arrow() {
        use airsnip_core::editor::Tool;
        let canvas = crate::canvas::Canvas::new(300, 300);
        let mut ov = Overlay::new(&canvas).unwrap();
        ov.sel = Some(Sel {
            x: 0,
            y: 0,
            w: 300,
            h: 300,
        });
        ov.tool = Tool::Arrow;
        crate::overlay::edit::begin(&mut ov, (100, 100));
        crate::overlay::edit::update(&mut ov, (250, 130), true);
        let airsnip_core::editor::Kind::Arrow { from, to, .. } = &ov.draft.as_ref().unwrap().kind else {
            panic!("应是箭头")
        };
        // 30/150 的斜率小于 22.5°，应吸附成水平
        assert!((to.y - from.y).abs() < 2.0, "应吸附为水平：{:?} → {:?}", from, to);
    }

    #[test]
    fn display_shows_bright_selection_over_dim() {
        let canvas = patterned(300, 200);
        let mut ov = Overlay::new(&canvas).unwrap();
        let sel = Sel {
            x: 50,
            y: 40,
            w: 100,
            h: 80,
        };
        ov.sel = Some(sel);
        rebuild(&mut ov);
        let i = ((100 * 300 + 100) * 4) as usize; // 选区内
        let o = ((10 * 300 + 10) * 4) as usize; // 选区外（暗化）
        let inside = ov.display.data()[i];
        let outside = ov.display.data()[o];
        assert!(inside > outside, "选区内应更亮：{inside} vs {outside}");
    }
}
