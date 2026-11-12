// SPDX-License-Identifier: GPL-3.0-only
//! 位图编码：阶段 1 只落 PNG，用于验证抓屏链路。
//!
//! 阶段 2 起复用 Windows 版 `output/` 的编码器（PNG / JPEG / WebP 无损）——
//! `png` / `jpeg-encoder` / `image-webp` 三个 crate 都是纯 Rust、与平台无关。

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use crate::Error;
use crate::canvas::Canvas;

/// 把画布编码成 PNG 字节（剪贴板与保存共用）。
pub fn png_bytes(canvas: &Canvas) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, canvas.w, canvas.h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        // 出图优先看速度（选区可能很大）；体积敏感的场景见 S5 的另存为参数。
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header()?;
        writer.write_image_data(&canvas.px)?;
    }
    Ok(out)
}

/// 把画布写成 PNG 文件。
pub fn write_png(canvas: &Canvas, path: &Path) -> Result<(), Error> {
    let file = File::create(path).map_err(|e| format!("无法写入 {}：{e}", path.display()))?;
    let mut w = BufWriter::new(file);
    std::io::Write::write_all(&mut w, &png_bytes(canvas)?)?;
    Ok(())
}
