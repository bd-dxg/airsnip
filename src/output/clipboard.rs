// SPDX-License-Identifier: GPL-3.0-only
//! 文本剪贴板（SYS-8 / ADR 0041）：写入与读取 `CF_UNICODETEXT`。
//! 从 `output/mod` 拆出（该文件已接近单文件规模上限）；图片剪贴板仍留在父模块。

use std::mem::size_of;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// `CF_UNICODETEXT` 的字节序列：UTF-16 码元 + 结尾 NUL（漏了 NUL，粘贴方会读到垃圾）。
pub(crate) fn text_units(text: &str) -> Vec<u16> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    units
}

/// 复制纯文本到剪贴板（`CF_UNICODETEXT`，SYS-8「复制系统信息」用）。
///
/// 与图片路径同样的句柄规矩：`SetClipboardData` 成功后剪贴板接管 HGLOBAL，不可再释放；
/// 失败时必须自己释放，否则每次点击都漏一块。
pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    let units = text_units(text);
    let bytes = units.len() * size_of::<u16>();
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| format!("GlobalAlloc (text) failed: {e}"))?;
        let ptr = GlobalLock(h);
        if ptr.is_null() {
            let _ = GlobalFree(Some(h));
            return Err("GlobalLock (text) failed".into());
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), ptr as *mut u16, units.len());
        let _ = GlobalUnlock(h);

        if OpenClipboard(None).is_err() {
            let _ = GlobalFree(Some(h));
            return Err("OpenClipboard failed".into());
        }
        let fail = |msg: String| {
            let _ = CloseClipboard();
            msg
        };
        if EmptyClipboard().is_err() {
            let _ = GlobalFree(Some(h));
            return Err(fail("EmptyClipboard failed".into()));
        }
        if SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(h.0))).is_err() {
            let _ = GlobalFree(Some(h));
            return Err(fail("SetClipboardData (CF_UNICODETEXT) failed".into()));
        }
        let _ = CloseClipboard();
        Ok(())
    }
}

/// 读取剪贴板文本（`CF_UNICODETEXT`，ADR 0041：路径模板框的 Ctrl+V）。
pub fn read_text_from_clipboard() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return None;
        }
        if OpenClipboard(None).is_err() {
            return None;
        }
        let read = || -> Option<String> {
            let h = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            if h.0.is_null() {
                return None;
            }
            let ptr = GlobalLock(HGLOBAL(h.0)) as *const u16;
            if ptr.is_null() {
                return None;
            }
            // 剪贴板文本以 NUL 结尾（系统保证）；这里手动量长并设上限，
            // 不依赖 `wcslen` 风格的无界扫描（同 `theme::is_theme_change` 的教训）。
            let mut len = 0usize;
            while *ptr.add(len) != 0 && len < 1 << 20 {
                len += 1;
            }
            // **必须先拷成 `String` 再 `GlobalUnlock`**：解锁后那块内存随时可能被回收，
            // 拿一个指向它的切片去解码就是 use-after-free（实测会让测试进程
            // 以 STATUS_HEAP_CORRUPTION 随机崩掉）。
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(HGLOBAL(h.0));
            Some(text)
        };
        let out = read();
        let _ = CloseClipboard();
        out
    }
}

/// 测试用：剪贴板操作的全局锁（避免并行用例互相把句柄弄成悬野指针）。
#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 剪贴板是**进程级共享资源**：并行用例里一个 `EmptyClipboard` 会把另一个刚取到的
    /// `HGLOBAL` 变成悬野句柄，对悬野句柄 `GlobalLock` 会坏堆（实测全量并行跑约 20%
    /// 概率 `STATUS_HEAP_CORRUPTION`）。碰剪贴板的用例都先拿这把锁。
    #[test]
    fn text_units_end_with_nul() {
        let _guard = crate::output::clipboard::test_lock();
        let text = "AirSnip 轻截 v0.13 · 快速保存";
        let units = text_units(text);
        assert_eq!(units.last(), Some(&0), "必须以 NUL 结尾：{units:?}");
        assert_eq!(
            &units[..units.len() - 1],
            &text.encode_utf16().collect::<Vec<_>>()[..],
            "正文应是原串的 UTF-16 码元"
        );
    }

    /// 写入后读回（不硬断言：剪贴板是全局共享资源，与其它用例 / 其它程序竞争时
    /// 内容可能被覆盖，与既有图片剪贴板用例保持同一尺度）。
    #[test]
    fn text_clipboard_write_then_read() {
        let _guard = crate::output::clipboard::test_lock();
        let text = "AirSnip 轻截 · 快速保存 roundtrip";
        match copy_text_to_clipboard(text) {
            Ok(()) => match read_text_from_clipboard() {
                Some(got) if got == text => println!("text clipboard roundtrip ok"),
                other => println!("text clipboard read back: {other:?}"),
            },
            Err(e) => println!("text clipboard err: {e}"),
        }
    }
}
