// SPDX-License-Identifier: GPL-3.0-only
//! 像素缓冲与上屏（CAP-3、OUT-2）：RGBA↔BGRA 转换、暗化、DIB section、
//! BitBlt 局部上屏、选区裁剪输出。GDI 资源集中创建与释放。

use std::mem::size_of;

use tiny_skia::{Pixmap, Transform};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, HBITMAP, HDC, ReleaseDC, SRCCOPY, SelectObject,
};

use super::SelRect;
use super::geometry::{info_font_size, info_pad_x};
use super::ui::Ui;
use crate::editor::Point;
use crate::render::text::{self as dwrite, FontSpec};

/// 暗化系数（CAP-3）：暗图 = 原图 × `DIM_K` / 256（即叠加一层黑色 alpha=120）。
/// 初始化与「再次截图」共用，避免两边各写一份数值而走形。
pub(super) const DIM_K: u32 = 255 - 120;

impl Drop for super::Overlay {
    /// 释放上屏用 GDI 资源（DIB section 位图 + 内存 DC）。
    fn drop(&mut self) {
        unsafe {
            if !self.dib_bmp.0.is_null() {
                let _ = DeleteObject(self.dib_bmp.into());
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
    }
}

impl super::Overlay {
    /// 上屏指定区域：局部 RGBA → BGRA 写入 DIB section，再用 BitBlt 刷到窗口。
    /// 用 BitBlt 而非 SetDIBitsToDevice：后者对 top-down DIB 的源行语义易出错。
    pub(super) fn present_region(&mut self, r: SelRect) {
        let w = self.display.width() as i32;
        let h = self.display.height() as i32;
        let x0 = r.x.clamp(0, w);
        let y0 = r.y.clamp(0, h);
        let x1 = (r.x + r.w).clamp(0, w);
        let y1 = (r.y + r.h).clamp(0, h);
        let rw = x1 - x0;
        let rh = y1 - y0;
        if rw <= 0 || rh <= 0 {
            return;
        }

        // 局部 RGBA → BGRA，直接写入 DIB section（大面积多线程、小面积串行）
        let area = rw as usize * rh as usize;
        let workers = if area >= 1 << 18 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .clamp(1, 8)
        } else {
            1
        };
        convert_rgba_to_bgra(self.display.data(), self.dib_bits as usize, w, x0, y0, rw, rh, workers);

        unsafe {
            let hdc = GetDC(Some(self.hwnd));
            if hdc.0.is_null() {
                return;
            }
            let _ = BitBlt(hdc, x0, y0, rw, rh, Some(self.mem_dc), x0, y0, SRCCOPY);
            let _ = ReleaseDC(Some(self.hwnd), hdc);
        }
    }
}

/// 选区尺寸提示（物理像素）：与工具栏文字走同一套 DirectWrite 栅格化。
/// 位置由调用方算好传入（选区走 `text_rect`，吸附候选还要再夹回屏内，见 ADR 0037），
/// 字色纯白；字号随显示器 DPI 缩放（CAP-5）。
pub(super) fn draw_info_text(pixmap: &mut Pixmap, r: SelRect, sel: SelRect, ui: Ui) {
    let text = format!("{} × {}", sel.w, sel.h);
    // 字重加粗：16px 常规字重在暗底上偏细发灰，加粗后笔画更实且不刺眼。
    let mut spec = FontSpec::new(info_font_size(ui));
    spec.bold = true;
    let (_, th) = dwrite::measure(&text, &spec);
    let x = (r.x + info_pad_x(ui)) as f32;
    let y = r.y as f32 + (r.h as f32 - th) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x, y),
        &text,
        &spec,
        [255, 255, 255, 255],
        Transform::identity(),
    );
}

/// 将 src 的 r 区域拷贝到 dst（原地回贴）。
pub(super) fn blit_region(dst: &mut Pixmap, src: &Pixmap, r: SelRect) {
    let w = dst.width() as i32;
    let h = dst.height() as i32;
    let x0 = r.x.clamp(0, w);
    let y0 = r.y.clamp(0, h);
    let x1 = (r.x + r.w).clamp(0, w);
    let y1 = (r.y + r.h).clamp(0, h);
    let rw = (x1 - x0).max(0);
    let rh = (y1 - y0).max(0);
    for row in 0..rh {
        let off = ((y0 + row) as usize * dst.width() as usize + x0 as usize) * 4;
        dst.data_mut()[off..off + rw as usize * 4].copy_from_slice(&src.data()[off..off + rw as usize * 4]);
    }
}

/// 区域从 src 拷贝到 dst（用于从暗图恢复旧选区区域）。
pub(super) fn restore_region(dst: &mut Pixmap, src: &Pixmap, r: SelRect) {
    blit_region(dst, src, r);
}

/// 区域就地压暗（ADR 0037）：逐行复用 `dim_chunk`，只碰 r 覆盖的像素。
/// 窗口吸附的候选区先回贴原图再压暗一档，得到「比未选区域亮、比确认后的选区暗」的半亮预览。
pub(super) fn dim_region(dst: &mut Pixmap, r: SelRect, k: u32) {
    let w = dst.width() as i32;
    let h = dst.height() as i32;
    let x0 = r.x.clamp(0, w);
    let y0 = r.y.clamp(0, h);
    let x1 = (r.x + r.w).clamp(0, w);
    let y1 = (r.y + r.h).clamp(0, h);
    let row_bytes = (x1 - x0).max(0) as usize * 4;
    if row_bytes == 0 {
        return;
    }
    for y in y0..y1 {
        let off = (y as usize * w as usize + x0 as usize) * 4;
        dim_chunk(&mut dst.data_mut()[off..off + row_bytes], k);
    }
}

/// 把任意尺寸的 `src`（straight RGBA）盖印到 `dst` 的 (x, y) 处（贴图编辑模式落底图，ADR 0028）。
/// 越界部分自动裁切；两侧均为 straight，直接逐行拷贝。
pub(super) fn stamp(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32) {
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    let (sw, sh) = (src.width() as i32, src.height() as i32);
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + sw).min(dw);
    let y1 = (y + sh).min(dh);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let row_bytes = (x1 - x0) as usize * 4;
    let sx = (x0 - x) as usize;
    for row in y0..y1 {
        let sy = (row - y) as usize;
        let dst_off = (row as usize * dw as usize + x0 as usize) * 4;
        let src_off = (sy * sw as usize + sx) * 4;
        dst.data_mut()[dst_off..dst_off + row_bytes].copy_from_slice(&src.data()[src_off..src_off + row_bytes]);
    }
}

/// 暗化像素图：逐通道乘以 k/256，多线程分块并行。
pub(super) fn dim_pixmap(pixmap: &mut Pixmap, k: u32) {
    let data = pixmap.data_mut();
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    if workers == 1 || data.len() < 1 << 20 {
        dim_chunk(data, k);
        return;
    }
    // 每块按 4 字节对齐切分（保证不跨像素）
    let chunk = ((data.len() / workers) + 3) & !3;
    std::thread::scope(|s| {
        for part in data.chunks_mut(chunk) {
            s.spawn(move || dim_chunk(part, k));
        }
    });
}

/// 对一段 RGBA 像素做暗化。
fn dim_chunk(data: &mut [u8], k: u32) {
    for px in data.chunks_exact_mut(4) {
        px[0] = ((px[0] as u32 * k) >> 8) as u8;
        px[1] = ((px[1] as u32 * k) >> 8) as u8;
        px[2] = ((px[2] as u32 * k) >> 8) as u8;
    }
}

/// 创建上屏用 DIB section（BGRA、top-down），返回内存 DC、位图句柄与像素指针。
pub(crate) fn create_dib_surface(w: u32, h: u32) -> Result<(HDC, HBITMAP, *mut u8), String> {
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.0.is_null() {
            return Err("CreateCompatibleDC 失败".into());
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            bmiColors: [Default::default()],
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbmp = match CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(e) => {
                let _ = DeleteDC(hdc);
                return Err(format!("CreateDIBSection 失败：{e}"));
            }
        };
        let _ = SelectObject(hdc, hbmp.into());
        Ok((hdc, hbmp, bits as *mut u8))
    }
}

/// RGBA → BGRA 写入目标缓冲区：转换 (x0,y0,rw,rh) 区域，按行多线程分块。
/// 目标地址用 usize 传递（裸指针不满足 Send）。
pub(crate) fn convert_rgba_to_bgra(
    src: &[u8],
    dst_addr: usize,
    w: i32,
    x0: i32,
    y0: i32,
    rw: i32,
    rh: i32,
    workers: usize,
) {
    let stride = w as usize * 4;
    let total_rows = rh as usize;
    let convert_row = move |row: usize| {
        let off = (y0 as usize + row) * stride + x0 as usize * 4;
        unsafe {
            let sp = src.as_ptr().add(off) as *const u32;
            let dp = (dst_addr as *mut u8).add(off) as *mut u32;
            for i in 0..rw as usize {
                let v = *sp.add(i);
                *dp.add(i) = (v & 0xFF00_FF00) | ((v & 0x00FF_0000) >> 16) | ((v & 0x0000_00FF) << 16);
            }
        }
    };

    if workers <= 1 {
        for row in 0..total_rows {
            convert_row(row);
        }
        return;
    }
    let row_chunk = total_rows.div_ceil(workers);
    std::thread::scope(|s| {
        for start in (0..total_rows).step_by(row_chunk.max(1)) {
            let rows = (total_rows - start).min(row_chunk);
            s.spawn(move || {
                for row in start..start + rows {
                    convert_row(row);
                }
            });
        }
    });
}

/// 裁剪出选区像素；尺寸非法时返回 None。
pub(super) fn crop_pixmap(src: &Pixmap, r: SelRect) -> Option<Pixmap> {
    let w = (r.w.max(1)) as u32;
    let h = (r.h.max(1)) as u32;
    let mut out = Pixmap::new(w, h)?;
    for (dy, row) in (0..h as i32).enumerate() {
        let sy = r.y + row;
        if sy < 0 || sy >= src.height() as i32 {
            continue;
        }
        let off_src = (sy as usize * src.width() as usize + r.x.max(0) as usize) * 4;
        let off_dst = dy * w as usize * 4;
        out.data_mut()[off_dst..off_dst + w as usize * 4]
            .copy_from_slice(&src.data()[off_src..off_src + w as usize * 4]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 盖印（ADR 0028）：越界与负偏移均按交集裁切，不越界访问。
    #[test]
    fn stamp_copies_with_clipping() {
        let red = [255u8, 0, 0, 255];
        let mut dst = Pixmap::new(8, 8).unwrap();
        dst.fill(tiny_skia::Color::BLACK);
        let mut src = Pixmap::new(4, 4).unwrap();
        src.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        stamp(&mut dst, &src, 6, 6); // 右下角越界，只盖 2×2
        assert_eq!(&dst.data()[(6 * 8 + 6) * 4..(6 * 8 + 6) * 4 + 4], &red);
        assert_eq!(&dst.data()[0..4], &[0, 0, 0, 255]); // 未覆盖处保持黑色

        let mut dst2 = Pixmap::new(4, 4).unwrap();
        dst2.fill(tiny_skia::Color::BLACK);
        stamp(&mut dst2, &src, -2, -2); // 负偏移：src(2,2) 落到 dst(0,0)
        assert_eq!(&dst2.data()[0..4], &red);
    }

    /// 区域压暗（ADR 0037）：只改 r 内的 RGB，区域外与 alpha 一字不动。
    #[test]
    fn dim_region_only_touches_rect() {
        let mut p = Pixmap::new(8, 8).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
        let r = SelRect { x: 2, y: 2, w: 3, h: 3 };
        dim_region(&mut p, r, 128);
        let px = |x: usize, y: usize| &p.data()[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4];
        // 区域内：各通道乘 128/256，alpha 不变
        assert_eq!(px(2, 2), &[100, 50, 25, 255]);
        assert_eq!(px(4, 4), &[100, 50, 25, 255]);
        // 区域外（紧邻的 x=5 / y=5）：完全没动
        assert_eq!(px(5, 2), &[200, 100, 50, 255]);
        assert_eq!(px(2, 5), &[200, 100, 50, 255]);
        assert_eq!(px(0, 0), &[200, 100, 50, 255]);
    }

    /// 区域压暗（ADR 0037）：越界区域被裁切，不越界访问也不 panic。
    #[test]
    fn dim_region_clips_out_of_bounds() {
        let mut p = Pixmap::new(4, 4).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(10, 20, 30, 255));
        dim_region(
            &mut p,
            SelRect {
                x: -4,
                y: -4,
                w: 6,
                h: 6,
            },
            128,
        );
        // 只到 (2,2)：左上 2×2 被压暗，其余保持原值
        assert_eq!(&p.data()[0..4], &[5, 10, 15, 255]);
        assert_eq!(&p.data()[(1 * 4 + 1) * 4..(1 * 4 + 1) * 4 + 4], &[5, 10, 15, 255]);
        assert_eq!(&p.data()[(2 * 4 + 2) * 4..(2 * 4 + 2) * 4 + 4], &[10, 20, 30, 255]);
        // 完全在屏外的区域：直接返回
        dim_region(
            &mut p,
            SelRect {
                x: 10,
                y: 10,
                w: 4,
                h: 4,
            },
            0,
        );
        assert_eq!(&p.data()[0..4], &[5, 10, 15, 255]);
    }
}
