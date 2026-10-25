// SPDX-License-Identifier: GPL-3.0-only
//! 微信赞赏码（关于页）：`assets/reward-code.png` 编译期嵌入，首次绘制时解码并缓存。
//! 图片随二进制分发，不依赖运行目录；解码失败则关于页只显示文案，不影响面板。
//! 用 png 解码（`png` crate 已用于保存截图），不额外引入 webp 解码器。

use std::sync::OnceLock;

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::editor::Point;
use crate::render::text as dwrite;

use super::Panel;
use super::controls::{PAD, Row};

/// 嵌入的赞赏码（png，300×329，约 2x 面板显示边长）。
const REWARD_CODE: &[u8] = include_bytes!("../../../assets/reward-code.png");

static CACHE: OnceLock<Option<Pixmap>> = OnceLock::new();

/// 支持项目块：左侧三行说明（末行强调色），右侧微信赞赏码（尺寸随 DPI 缩放）。
/// 标签由本函数自行绘制（高行顶对齐），不走上层通用标签位置。
pub(super) fn draw_block(pixmap: &mut Pixmap, panel: &Panel, row: &Row) {
    let p = panel.palette;
    let s = panel.scale;
    dwrite::draw(
        pixmap,
        Point::new(row.rect.x as f32 + PAD as f32 * s, row.rect.y as f32 + 12.0 * s),
        row.label,
        &dwrite::FontSpec::new(16.0 * s),
        p.text,
        Transform::identity(),
    );
    let size = 16.0 * s;
    let spec = dwrite::FontSpec::new(size);
    let lx = row.rect.x as f32 + PAD as f32 * s;
    let mut ly = row.rect.y as f32 + 46.0 * s;
    for (i, line) in [
        "功能都是业余时间开发和维护的：",
        "跟进系统改动、修 bug、加新功能。",
        "微信扫码支持一下，",
        "每份心意都是它继续更新的动力。",
    ]
    .iter()
    .enumerate()
    {
        // 后两行（扫码引导）用强调色，避免四行糊成一片
        let color = if i >= 2 { p.accent } else { p.text_dim };
        dwrite::draw(pixmap, Point::new(lx, ly), line, &spec, color, Transform::identity());
        ly += size * 1.7;
    }
    let Some(qr) = pixmap_cached() else {
        return;
    };
    let side = (150.0 * s) as u32;
    let x = (row.rect.x + row.rect.w) as f32 - side as f32 - PAD as f32 * s;
    let y = row.rect.y as f32 + (row.rect.h as f32 - side as f32) / 2.0;
    draw_scaled(pixmap, qr, x as i32, y as i32, side);
}

/// 赞赏码位图（RGBA → tiny-skia 预乘），首次调用解码，之后复用。
pub(super) fn pixmap_cached() -> Option<&'static Pixmap> {
    CACHE.get_or_init(decode).as_ref()
}

/// 双线性缩放到目标边长后贴到面板。
fn draw_scaled(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32, side: u32) {
    let Some(mut scaled) = Pixmap::new(side, side) else {
        return;
    };
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..Default::default()
    };
    let tr = Transform::from_scale(side as f32 / src.width() as f32, side as f32 / src.height() as f32);
    scaled.draw_pixmap(0, 0, src.as_ref(), &paint, tr, None);
    dst.draw_pixmap(
        x,
        y,
        scaled.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

/// 解码 png 为预乘 Pixmap；格式不支持或解码失败返回 None。
fn decode() -> Option<Pixmap> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(REWARD_CODE)).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    let (w, h) = (info.width, info.height);
    let mut rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        // RGB：补不透明 alpha
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        _ => return None,
    };
    premultiply(&mut rgba);
    Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(w, h)?)
}

/// RGBA → 预乘 alpha（tiny-skia 内部约定；全不透明的图此步无变化）。
fn premultiply(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((*c as u32 * a) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 嵌入的赞赏码能解码且尺寸合理（防止资源文件被换坏后静默丢失图片）。
    #[test]
    fn embedded_reward_code_decodes() {
        let qr = pixmap_cached().expect("嵌入的 assets/reward-code.png 应能解码");
        assert!(
            qr.width() >= 100 && qr.height() >= 100,
            "赞赏码尺寸异常：{}x{}",
            qr.width(),
            qr.height()
        );
    }

    /// 预乘只影响半透明像素，不透明像素保持不变。
    #[test]
    fn premultiply_keeps_opaque_pixels() {
        let mut rgba = vec![200, 100, 50, 255, 200, 100, 50, 128];
        premultiply(&mut rgba);
        assert_eq!(&rgba[..4], &[200, 100, 50, 255], "不透明像素不应改变");
        assert_eq!(&rgba[4..], &[100, 50, 25, 128], "半透明像素应预乘 alpha");
    }
}
