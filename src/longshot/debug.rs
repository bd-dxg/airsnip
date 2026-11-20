// SPDX-License-Identifier: GPL-3.0-only
//! 长截图调试日志：`AIRSNIP_DEBUG=1` 时把每步状态写入 `%TEMP%\airsnip-longshot.log`，
//! 用于定位滚动/拼接链路问题（M4 排障）。
//!
//! 除了日志，还能在「画面变了但判定为静止」等可疑时刻把两帧落成 PNG
//! （`airsnip-longshot-<tag>-<n>-prev/new.png`），用来判断到底是画面真没动、
//! 还是我们抓到的帧是旧的。

use std::io::Write;
use std::sync::Mutex;

static LOG: Mutex<Option<std::fs::File>> = Mutex::new(None);
/// 本次运行第几次落盘（限制次数，避免一次会话写出上百张全屏图）。
static DUMPS: Mutex<u32> = Mutex::new(0);
/// 落盘上限。
const MAX_DUMPS: u32 = 6;

/// 调试开关是否打开。
pub fn enabled() -> bool {
    std::env::var_os("AIRSNIP_DEBUG").is_some()
}

/// 写一行日志（无 AIRSNIP_DEBUG 时为空操作）。
pub fn log(msg: &str) {
    if !enabled() {
        return;
    }
    let mut guard = LOG.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        let path = std::env::temp_dir().join("airsnip-longshot.log");
        *guard = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
    }
    let ts = super::window_start_ms();
    if let Some(f) = guard.as_mut() {
        let _ = f.write_all(format!("[{ts}] {msg}\n").as_bytes());
    }
}

/// 把一对帧落成 PNG（`tag` 标明时机），最多 `MAX_DUMPS` 次。
/// 只有 `AIRSNIP_DEBUG=1` 时才写盘。
pub fn dump_pair(tag: &str, prev: &[u8], new: &[u8], w: u32, h: u32) {
    if !enabled() {
        return;
    }
    let mut n = DUMPS.lock().unwrap_or_else(|p| p.into_inner());
    if *n >= MAX_DUMPS {
        return;
    }
    *n += 1;
    let seq = *n;
    drop(n);
    let dir = std::env::temp_dir();
    let Some(size) = tiny_skia::IntSize::from_wh(w, h) else {
        return;
    };
    for (name, data) in [("prev", prev), ("new", new)] {
        if data.len() != (w as usize * h as usize * 4) {
            continue;
        }
        if let Some(px) = tiny_skia::Pixmap::from_vec(data.to_vec(), size) {
            let path = dir.join(format!("airsnip-longshot-{tag}-{seq}-{name}.png"));
            let _ = px.save_png(&path);
        }
    }
}
