// SPDX-License-Identifier: GPL-3.0-only
//! RGBA `Pixmap` → WinRT `SoftwareBitmap`（OCR-5）：BGRA8 + `BitmapAlphaMode::Ignore`。
//!
//! 用 `Ignore` 而不是 `Premultiplied`：截图是不透明图，alpha 只会干扰识别率。

use tiny_skia::Pixmap;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Storage::Streams::Buffer;
use windows::Win32::System::WinRT::IBufferByteAccess;
use windows::core::Interface;

use super::super::Error;

/// 送给引擎的最大放大倍数。
///
/// 实测（2003×160 的真实截图，16px 中文字）：1x 拆 4 处偏旁（`亻旦`、`氵益出`、`氵则量`），
/// 2x 全对，**3x 反而变差**（拆回 2 处，标点还退化成逗号）。所以固定封顶 2 倍。
const MAX_SCALE: u32 = 2;
/// 放大后的像素预算（约 64MB 缓冲）：大选区不放大，避免一次吃掉几百 MB。
const MAX_SCALED_PIXELS: u64 = 16_000_000;

/// 放大倍数：默认 2 倍，受引擎尺寸上限与内存预算双重限制。
///
/// 早期只对「长边 ≤1200」的图放大，结果**宽图反而吃亏**：宽截图上的小字号照样被引擎
/// 按偏旁拆开。偏旁拆分的根因是笔画在引擎内部降采样时被打断，与图宽窄无关。
fn scale_factor(w: u32, h: u32, max_dim: u32) -> u32 {
    let long = w.max(h) as u64;
    let scaled_pixels = w as u64 * h as u64 * (MAX_SCALE as u64 * MAX_SCALE as u64);
    if long * MAX_SCALE as u64 <= max_dim as u64 && scaled_pixels <= MAX_SCALED_PIXELS {
        MAX_SCALE
    } else {
        1
    }
}

/// 转成 OCR 引擎接受的 BGRA8 位图（必要时 2 倍最近邻放大）。
///
/// 超过引擎上限（`max_dim`）时不缩放，直接报错让用户缩小选区——重采样会糊掉笔画，
/// 识别率反而更差，且缩放本身要额外一份全图内存。
pub(super) fn to_software_bitmap(pixmap: &Pixmap, max_dim: u32) -> Result<SoftwareBitmap, Error> {
    let (w, h) = (pixmap.width(), pixmap.height());
    if w == 0 || h == 0 {
        return Err(Error::Failed("选区为空".to_string()));
    }
    // 放大后超过引擎上限就退回 1x（宁可识别率差，也不能直接失败）
    let k = scale_factor(w, h, max_dim);
    let (ow, oh) = (w * k, h * k);
    if ow.max(oh) > max_dim {
        return Err(Error::TooLarge { max: max_dim });
    }
    let len = ow as usize * oh as usize * 4;
    let buffer = Buffer::Create(len as u32).map_err(|e| Error::Failed(format!("分配位图缓冲失败：{e}")))?;
    // `Create` 只给容量，长度默认 0：不设长度时下游会按 0 字节拷贝（甚至读越界）
    buffer
        .SetLength(len as u32)
        .map_err(|e| Error::Failed(format!("设置位图长度失败：{e}")))?;
    // `Buffer` 的原始字节要通过 COM 互操作接口取（`IMemoryBufferByteAccess` 属于
    // `IMemoryBufferReference`，在 `Buffer` 上取不到——会返回 E_NOINTERFACE）
    let access: IBufferByteAccess = buffer
        .cast()
        .map_err(|e| Error::Failed(format!("访问位图缓冲失败：{e}")))?;
    let ptr = unsafe { access.Buffer() }.map_err(|e| Error::Failed(format!("取位图缓冲指针失败：{e}")))?;
    if ptr.is_null() {
        return Err(Error::Failed("位图缓冲区为空".to_string()));
    }
    // tiny-skia 是预乘 RGBA；截图不透明，直接换序并补满 alpha（引擎按 Ignore 处理 alpha）
    let src = pixmap.data();
    unsafe {
        for oy in 0..oh as usize {
            let sy = oy / k as usize;
            for ox in 0..ow as usize {
                let sx = ox / k as usize;
                let s = src.as_ptr().add((sy * w as usize + sx) * 4);
                let d = ptr.add((oy * ow as usize + ox) * 4);
                *d = *s.add(2);
                *d.add(1) = *s.add(1);
                *d.add(2) = *s;
                *d.add(3) = 0xFF;
            }
        }
    }
    SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        ow as i32,
        oh as i32,
        BitmapAlphaMode::Ignore,
    )
    .map_err(|e| Error::Failed(format!("创建位图失败：{e}")))
}
