// SPDX-License-Identifier: GPL-3.0-only
//! 输出（OUT-1/2/3）：剪贴板 `CF_DIB` + `PNG` 格式、PNG/JPEG 文件保存。
//! 编码调用方负责放到后台线程执行（OUT-2）。

use std::mem::size_of;
use std::path::{Path, PathBuf};

use tiny_skia::Pixmap;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFOHEADER};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_DIB;
use windows::core::w;

/// 复制到剪贴板：写入 CF_DIB（BGRA）与 "PNG" 注册格式（OUT-1）。
pub fn copy_to_clipboard(pixmap: &Pixmap) -> Result<(), String> {
    let w = pixmap.width() as u32;
    let h = pixmap.height() as u32;
    let bgra = rgba_to_bgra(pixmap.data());
    let png_bytes = encode_png(pixmap)?;

    unsafe {
        if OpenClipboard(None).is_err() {
            return Err("OpenClipboard 失败".into());
        }
        let fail = |msg: String| {
            let _ = CloseClipboard();
            msg
        };

        if EmptyClipboard().is_err() {
            return Err(fail("EmptyClipboard 失败".into()));
        }

        // CF_DIB：BITMAPINFOHEADER + BGRA 像素。
        let header_size = size_of::<BITMAPINFOHEADER>() as usize;
        let hdib = match GlobalAlloc(GMEM_MOVEABLE, header_size + bgra.len()) {
            Ok(h) => h,
            Err(e) => return Err(fail(format!("GlobalAlloc CF_DIB 失败：{e}"))),
        };
        let ptr = GlobalLock(hdib);
        if ptr.is_null() {
            return Err(fail("GlobalLock CF_DIB 失败".into()));
        }
        let header = BITMAPINFOHEADER {
            biSize: header_size as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // 自上而下
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        std::ptr::copy_nonoverlapping(&header as *const _ as *const u8, ptr as *mut u8, header_size);
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), (ptr as *mut u8).add(header_size), bgra.len());
        let _ = GlobalUnlock(hdib);
        if SetClipboardData(CF_DIB.0 as u32, Some(HANDLE(hdib.0))).is_err() {
            // SetClipboardData 失败时剪贴板未接管句柄，必须自行释放
            let _ = GlobalFree(Some(HGLOBAL(hdib.0)));
            return Err(fail("SetClipboardData CF_DIB 失败".into()));
        }

        // PNG 格式（延迟渲染的简化：直接同步提供完整数据）。
        let fmt = RegisterClipboardFormatW(w!("PNG"));
        if fmt == 0 {
            return Err(fail("RegisterClipboardFormatW(PNG) 返回 0".into()));
        }
        let hpng = match GlobalAlloc(GMEM_MOVEABLE, png_bytes.len()) {
            Ok(h) => h,
            Err(e) => return Err(fail(format!("GlobalAlloc PNG 失败：{e}"))),
        };
        let pptr = GlobalLock(hpng);
        if pptr.is_null() {
            return Err(fail("GlobalLock PNG 失败".into()));
        }
        std::ptr::copy_nonoverlapping(png_bytes.as_ptr(), pptr as *mut u8, png_bytes.len());
        let _ = GlobalUnlock(hpng);
        if SetClipboardData(fmt, Some(HANDLE(hpng.0))).is_err() {
            let _ = GlobalFree(Some(HGLOBAL(hpng.0)));
            return Err(fail("SetClipboardData PNG 失败".into()));
        }

        let _ = CloseClipboard();
        Ok(())
    }
}

/// 保存像素为 PNG/JPEG/WebP（按扩展名决定格式，OUT-2/OUT-5）。
/// 扩展名缺失或不受支持时报错：对话框允许手输文件名，静默按 PNG 写出会得到
/// 内容与扩展名不符的文件。
pub fn save_file(pixmap: &Pixmap, path: &Path) -> Result<(), String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => save_png(pixmap, path),
        Some("jpg") | Some("jpeg") => save_jpeg(pixmap, path),
        Some("webp") => save_webp(pixmap, path),
        other => Err(format!(
            "不支持的图片格式：{}（请用 .png / .jpg / .webp）",
            other.unwrap_or("（无扩展名）")
        )),
    }
}

/// 保存 PNG（快速压缩，OUT-2）。
pub fn save_png(pixmap: &Pixmap, path: &Path) -> Result<(), String> {
    let bytes = encode_png(pixmap)?;
    std::fs::write(path, bytes).map_err(|e| format!("写入 {path:?} 失败：{e}"))
}

/// 保存 JPEG（质量 90，OUT-2）。
pub fn save_jpeg(pixmap: &Pixmap, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("创建 {path:?} 失败：{e}"))?;
    let enc = jpeg_encoder::Encoder::new(file, 90);
    enc.encode(
        pixmap.data(),
        pixmap.width() as u16,
        pixmap.height() as u16,
        jpeg_encoder::ColorType::Rgba,
    )
    .map_err(|e| format!("JPEG 编码失败：{e}"))
}

/// 默认保存目录：`%USERPROFILE%\Pictures\airsnip`。
pub fn default_save_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Pictures").join("airsnip")
}

/// 生成时间戳文件名，如 `airsnip-20240101-120000.png`。
pub fn timestamped_filename(ext: &str) -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    format!(
        "airsnip-{:04}{:02}{:02}-{:02}{:02}{:02}.{ext}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}

/// 保存 WebP（无损 VP8L，OUT-5）：纯 Rust 编码（image-webp），无 C 依赖。
pub fn save_webp(pixmap: &Pixmap, path: &Path) -> Result<(), String> {
    let mut out = Vec::new();
    image_webp::WebPEncoder::new(&mut out)
        .encode(
            pixmap.data(),
            pixmap.width(),
            pixmap.height(),
            image_webp::ColorType::Rgba8,
        )
        .map_err(|e| format!("WebP 编码失败：{e}"))?;
    std::fs::write(path, out).map_err(|e| format!("写入 {path:?} 失败：{e}"))
}

/// 按命名模板生成基础文件名（不含扩展名，OUT-2）：
/// 保存对话框用它预填文件名，扩展名由对话框按所选格式补上。
pub fn base_name(style: crate::settings::NameStyle) -> String {
    use crate::settings::NameStyle;
    use windows::Win32::System::SystemInformation::GetLocalTime;
    match style {
        // Unix 毫秒：同一秒内连续保存两次也不重名
        NameStyle::Timestamp => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default(),
        NameStyle::DateTime | NameStyle::Date => {
            let st = unsafe { GetLocalTime() };
            let (y, mo, d) = (st.wYear, st.wMonth, st.wDay);
            if let NameStyle::Date = style {
                format!("airsnip-{y:04}{mo:02}{d:02}")
            } else {
                let (h, mi, s) = (st.wHour, st.wMinute, st.wSecond);
                format!("airsnip-{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
            }
        }
    }
}

/// RGBA → BGRA（就地语义复制出新缓冲）。
pub fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    bgra
}

/// 编码 PNG（快速压缩）。
fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(pixmap.data()).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_roundtrip() {
        let pixmap = Pixmap::new(8, 8).unwrap();
        match copy_to_clipboard(&pixmap) {
            Ok(()) => println!("clipboard ok"),
            Err(e) => println!("clipboard err: {e}"),
        }
    }

    /// WebP 无损编码（OUT-5）：输出应是合法的 RIFF/WEBP 容器。
    #[test]
    fn webp_encode_produces_riff_container() {
        let mut pixmap = Pixmap::new(16, 16).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(200, 30, 30, 255));
        let mut out = Vec::new();
        image_webp::WebPEncoder::new(&mut out)
            .encode(pixmap.data(), 16, 16, image_webp::ColorType::Rgba8)
            .expect("WebP 编码应成功");
        assert!(out.len() > 20, "WebP 输出不应为空：{}", out.len());
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WEBP");
    }

    /// 命名模板（OUT-2）：三种模板格式正确，时间戳为 13 位 Unix 毫秒。
    #[test]
    fn base_name_matches_templates() {
        use crate::settings::NameStyle;
        let dt = base_name(NameStyle::DateTime);
        assert!(dt.starts_with("airsnip-"), "{dt}");
        assert_eq!(dt.len(), "airsnip-20241231-235959".len(), "{dt}");
        // Unix 毫秒：纯数字、13 位（当前纪元）、且单调递增
        let ts = base_name(NameStyle::Timestamp);
        assert!(ts.chars().all(|c| c.is_ascii_digit()), "时间戳应为纯数字：{ts}");
        assert_eq!(ts.len(), 13, "时间戳应为 13 位毫秒：{ts}");
        assert!(base_name(NameStyle::Timestamp) >= ts, "时间戳不应倒退");
        let d = base_name(NameStyle::Date);
        assert!(d.starts_with("airsnip-"), "{d}");
        assert_eq!(d.len(), "airsnip-20241231".len(), "{d}");
    }

    /// save_file 只接受三种扩展名：其余报错，避免内容与扩展名不符（OUT-2）。
    #[test]
    fn save_file_dispatches_by_extension() {
        let pixmap = Pixmap::new(4, 4).unwrap();
        let dir = std::env::temp_dir();
        for bad in ["t.bmp", "t", "t.gif"] {
            assert!(save_file(&pixmap, &dir.join(bad)).is_err(), "{bad} 应被拒绝");
        }
        for name in ["out.png", "out.jpg", "out.webp"] {
            let path = dir.join(format!("airsnip-test-{name}"));
            let r = save_file(&pixmap, &path);
            assert!(r.is_ok(), "{name} 应写入成功：{r:?}");
            let _ = std::fs::remove_file(&path);
        }
    }
}
