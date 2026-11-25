// SPDX-License-Identifier: GPL-3.0-only
//! 文本栅格化（EDT-4 / issue #221）：`IDWriteTextLayout` → 自定义 `IDWriteTextRenderer`
//! → `IDWriteBitmapRenderTarget`（内存 DIB，黑字白底）→ 反色取覆盖度并与目标色混合。
//!
//! 从 `render::text` 拆出（那里加完折行 API 后撞了单文件 300 行硬上限）：
//! 量测 / 折行 / 光标查询留在父模块，这里只负责把字形画进像素图。

use tiny_skia::{Pixmap, Transform};
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, GdiFlush, HBITMAP, HDC, PatBlt, WHITENESS};
use windows::core::{BOOL, IUnknown, Ref, Result, implement};

use crate::editor::Point;

/// 自定义文字器：只把字形转发给位图渲染目标（黑字白底），忽略下划线/删除线。
// 实现目标为 `Renderer_Impl`（windows-implement 0.60 约定），字段经 `this` 访问。
#[implement(IDWriteTextRenderer)]
struct Renderer {
    target: IDWriteBitmapRenderTarget,
    params: IDWriteRenderingParams,
}

impl IDWritePixelSnapping_Impl for Renderer_Impl {
    fn IsPixelSnappingDisabled(&self, _: *const core::ffi::c_void) -> Result<BOOL> {
        Ok(false.into())
    }

    fn GetCurrentTransform(&self, _: *const core::ffi::c_void, transform: *mut DWRITE_MATRIX) -> Result<()> {
        unsafe {
            *transform = DWRITE_MATRIX {
                m11: 1.0,
                m12: 0.0,
                m21: 0.0,
                m22: 1.0,
                dx: 0.0,
                dy: 0.0,
            };
        }
        Ok(())
    }

    fn GetPixelsPerDip(&self, _: *const core::ffi::c_void) -> Result<f32> {
        Ok(1.0)
    }
}

impl IDWriteTextRenderer_Impl for Renderer_Impl {
    fn DrawGlyphRun(
        &self,
        _ctx: *const core::ffi::c_void,
        x: f32,
        y: f32,
        mode: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        _desc: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _effect: Ref<IUnknown>,
    ) -> Result<()> {
        unsafe {
            self.this
                .target
                .DrawGlyphRun(x, y, mode, run, &self.this.params, COLORREF(0), None)
        }?;
        Ok(())
    }

    fn DrawUnderline(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_UNDERLINE,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawStrikethrough(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_STRIKETHROUGH,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawInlineObject(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: Ref<IDWriteInlineObject>,
        _: BOOL,
        _: BOOL,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
}

/// 栅格化文本并混合进 `pixmap`（`tr` 用于导出烘焙的坐标平移）。
/// `pos` 为文字左上角；`wrap` 为可用宽度（`None` = 不折行，issue #221）。
pub(crate) fn draw_wrapped(
    pixmap: &mut Pixmap,
    pos: Point,
    text: &str,
    spec: &super::FontSpec,
    color: [u8; 4],
    tr: Transform,
    wrap: Option<f32>,
) {
    let Some(f) = super::factory() else { return };
    let (w, h) = super::measure_wrapped(text, spec, wrap);
    let pad = crate::editor::text::pad(spec.size).ceil() as u32;
    let cw = (w.ceil() as u32 + pad * 2).max(4);
    let ch = (h.ceil() as u32 + pad * 2).max(4);

    // 内存 DIB（top-down BGRA），DirectWrite 栅格化画布
    let Some((dc, bmp)) = crate::render::text_bitmap::create_dib(cw, ch) else {
        return;
    };
    let cleanup = |dc: HDC, bmp: HBITMAP| unsafe {
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
    };
    let target = match unsafe {
        f.GetGdiInterop()
            .and_then(|i| i.CreateBitmapRenderTarget(Some(dc), cw, ch))
    } {
        Ok(t) => t,
        Err(_) => {
            cleanup(dc, bmp);
            return;
        }
    };
    if unsafe { target.SetPixelsPerDip(1.0) }.is_err() {
        cleanup(dc, bmp);
        return;
    }
    // 渲染目标的内存 DC：白化 + 读回共用这一个句柄
    let rt_dc = unsafe { target.GetMemoryDC() };
    unsafe {
        let _ = PatBlt(rt_dc, 0, 0, cw as i32, ch as i32, WHITENESS);
    }
    let (Some(format), Ok(params)) = (super::create_format(f, spec), unsafe { f.CreateRenderingParams() }) else {
        cleanup(dc, bmp);
        return;
    };
    let width = wrap.map_or(super::MAX_LAYOUT, |v| v.max(1.0));
    let Ok(layout) = (unsafe { f.CreateTextLayout(&super::utf16(text), &format, width, super::MAX_LAYOUT) }) else {
        cleanup(dc, bmp);
        return;
    };
    let renderer = Renderer { target, params };
    let renderer: super::IDWriteTextRenderer = renderer.into();
    // origin 取 (pad, pad)：字形在 DIB 中相对四周内边距居中，blend 时映射回 pos 即为文字左上角
    if unsafe { layout.Draw(None, &renderer, pad as f32, pad as f32) }.is_err() {
        cleanup(dc, bmp);
        return;
    }
    let _ = unsafe { GdiFlush() };
    // 字形绘制在 render target 内部位图上，用 GetDIBits 读回像素
    let Some(px) = crate::render::text_bitmap::read_glyphs(rt_dc, cw, ch) else {
        cleanup(dc, bmp);
        return;
    };
    crate::render::text_bitmap::blend_glyphs(pixmap, tr, pos, pad, cw, ch, &px, color);
    cleanup(dc, bmp);
}

/// 不折行的 `draw_wrapped`（徽标、工具栏提示、尺寸提示等界面文字）。
pub(crate) fn draw(pixmap: &mut Pixmap, pos: Point, text: &str, spec: &super::FontSpec, color: [u8; 4], tr: Transform) {
    draw_wrapped(pixmap, pos, text, spec, color, tr, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Point;
    use crate::render::text::FontSpec;

    fn spec(size: f32) -> FontSpec {
        FontSpec::new(size)
    }

    #[test]
    fn bold_and_italic_render_differently() {
        // 中文粗体在部分字体下字宽略窄，故以「实际栅格化像素」为判据：
        // 粗体笔画更粗（着墨像素更多），斜体字形发生倾斜（像素图不同）
        let ink = |spec: &FontSpec| {
            let mut p = Pixmap::new(160, 60).unwrap();
            p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
            draw(
                &mut p,
                Point::new(5.0, 5.0),
                "bold Wm 粗",
                spec,
                [255, 255, 255, 255],
                Transform::identity(),
            );
            p.data().chunks_exact(4).filter(|px| px[0] > 100).count()
        };
        let regular = FontSpec::new(24.0);
        let bold = FontSpec {
            bold: true,
            ..regular.clone()
        };
        let italic = FontSpec {
            italic: true,
            ..regular.clone()
        };
        let (r, b, i) = (ink(&regular), ink(&bold), ink(&italic));
        assert!(b > r, "粗体着墨应更多：regular={r} bold={b}");
        assert!(i != r, "斜体字形应与常规不同：regular={r} italic={i}");
    }

    #[test]
    fn draw_produces_white_pixels_on_black() {
        let mut p = Pixmap::new(120, 80).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        draw(
            &mut p,
            Point::new(10.0, 10.0),
            "测试",
            &spec(24.0),
            [255, 255, 255, 255],
            Transform::identity(),
        );
        let any = p.data().chunks_exact(4).any(|px| px[0] > 0);
        assert!(any, "黑底上应绘出白色字形");
    }

    #[test]
    fn draw_translates_with_transform() {
        let mut p = Pixmap::new(200, 100).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        let tr = Transform::from_translate(50.0, 20.0);
        draw(
            &mut p,
            Point::new(10.0, 10.0),
            "测试",
            &spec(24.0),
            [255, 255, 255, 255],
            tr,
        );
        // 平移后字形应出现在右侧区域（x ≥ 50），原位置（x < 50）应无像素
        let count = |keep: bool| {
            p.data()
                .chunks_exact(4)
                .enumerate()
                .filter(|(i, px)| {
                    let x = (i % 200) as i32;
                    (x >= 50) == keep && px[0] > 0
                })
                .count()
        };
        assert_eq!(count(false), 0, "平移后左侧不应有字形");
        assert!(count(true) > 0, "平移后右侧应有字形");
    }

    /// issue #221：带折行宽度绘制时，字形分布到多行（而不是溢出宽度继续向右）。
    #[test]
    fn draw_wrapped_puts_ink_on_multiple_lines() {
        let mut p = Pixmap::new(120, 160).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        draw_wrapped(
            &mut p,
            Point::new(5.0, 5.0),
            "一二三四五六七八",
            &spec(20.0),
            [255, 255, 255, 255],
            Transform::identity(),
            Some(62.0),
        );
        let ink_in =
            |y0: u32, y1: u32| (y0..y1).any(|y| (0..120).any(|x| p.pixel(x, y).is_some_and(|c| c.red() > 100)));
        assert!(ink_in(30, 55), "第二行应有字形（折行生效）");
        assert!(ink_in(60, 85), "第三行应有字形（折行生效）");
    }
}
