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
            ..
        } = ov;
        display.clone_from(dim);
        copy_block(display, bright, sel.x, sel.y, sel.w, sel.h, sel.x, sel.y);
        let clip = (sel.x, sel.y, sel.x + sel.w, sel.y + sel.h);
        crate::draw::draw_objects(display, doc.objects(), draft.as_ref(), clip);
    }
    draw_decorations(&mut ov.display, sel);
    // 工具栏（选区下方优先，塞不下翻上方）
    let (cw, ch) = (ov.display.width() as i32, ov.display.height() as i32);
    let tb = super::toolbar::layout(cw, ch, sel);
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
        crate::overlay::edit::update(&mut ov, (180, 180));
        crate::overlay::edit::commit(&mut ov);
        let out = export(&ov, sel).unwrap();
        let i = ((100 * 200 + 100) * 4) as usize;
        assert_eq!(&out.px[i..i + 3], &[255, 0, 0], "填充矩形应盖住底图");
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
