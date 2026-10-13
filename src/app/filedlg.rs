// SPDX-License-Identifier: GPL-3.0-only
//! 原生另存为对话框（OUT-2）：保存截图时自选路径、文件名与格式（PNG / JPEG / WebP）。
//! COM 接口，线程未初始化时先 `CoInitializeEx`（STA）；
//! windows-rs 未导出该对话框的 CLSID，故在本地定义。

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
};
use windows::core::{GUID, HSTRING, PCWSTR, w};

/// `CLSID_FileSaveDialog`。
const CLSID_FILE_SAVE_DIALOG: GUID = GUID::from_u128(0xC0B4E2F3_BA21_4773_8DBA_335EC946EB8B);

/// COM 初始化守卫：`RPC_E_CHANGED_MODE` 表示线程已按其它模式初始化，不重复反初始化。
struct ComGuard {
    uninit: bool,
}

impl ComGuard {
    fn new() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Self { uninit: hr.is_ok() }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.uninit {
            unsafe { CoUninitialize() };
        }
    }
}

/// 宽字符串（对话框接口只接受 `PCWSTR`，需在调用期间存活）。
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// 路径 → `IShellItem`（设置对话框初始位置用）。
fn shell_item(path: &Path) -> Option<IShellItem> {
    let h = HSTRING::from(path.to_string_lossy().as_ref());
    unsafe { SHCreateItemFromParsingName::<_, _, IShellItem>(&h, None).ok() }
}

/// `IShellItem` → 文件系统路径（`GetDisplayName` 分配的串需 `CoTaskMemFree`）。
unsafe fn item_path(item: &IShellItem) -> Option<PathBuf> {
    unsafe {
        let pw = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        if pw.is_null() {
            return None;
        }
        let out = pw.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(pw.0 as *const core::ffi::c_void));
        out
    }
}

/// 扩展名 → 筛选器索引（1 起）：PNG=1、JPEG=2、WebP=3，未知按 PNG。
fn filter_index(ext: &str) -> u32 {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => 2,
        "webp" => 3,
        _ => 1,
    }
}

/// 另存为对话框：返回用户选定路径（扩展名由对话框按格式补全）；取消或失败返回 None。
/// `name` 为预填文件名（含扩展名），`dir` 为初始目录（`None` 时用系统上次位置），
/// `default_ext` 为默认选中的格式（上次保存的格式，首次 PNG）。
pub fn pick_save_file(owner: HWND, dir: Option<&Path>, name: &str, default_ext: &str) -> Option<PathBuf> {
    let _com = ComGuard::new();
    unsafe {
        let dlg: IFileSaveDialog = CoCreateInstance(&CLSID_FILE_SAVE_DIALOG, None, CLSCTX_ALL).ok()?;
        if let Ok(opts) = dlg.GetOptions() {
            let _ = dlg.SetOptions(opts | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_OVERWRITEPROMPT);
        }
        let _ = dlg.SetTitle(w!("保存截图"));
        // 筛选器顺序即默认选中项（索引 1 = PNG）
        let filters = [
            COMDLG_FILTERSPEC {
                pszName: w!("PNG 图片"),
                pszSpec: w!("*.png"),
            },
            COMDLG_FILTERSPEC {
                pszName: w!("JPEG 图片"),
                pszSpec: w!("*.jpg;*.jpeg"),
            },
            COMDLG_FILTERSPEC {
                pszName: w!("WebP 图片（无损）"),
                pszSpec: w!("*.webp"),
            },
        ];
        let _ = dlg.SetFileTypes(&filters);
        let _ = dlg.SetFileTypeIndex(filter_index(default_ext));
        let ext = wide(default_ext);
        let _ = dlg.SetDefaultExtension(PCWSTR(ext.as_ptr()));
        let n = wide(name);
        let _ = dlg.SetFileName(PCWSTR(n.as_ptr()));
        if let Some(item) = dir.and_then(shell_item) {
            let _ = dlg.SetFolder(&item);
        }
        dlg.Show(Some(owner)).ok()?;
        item_path(&dlg.GetResult().ok()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 上次格式 → 筛选器索引（OUT-2）：三种格式各归其位，未知按 PNG。
    #[test]
    fn filter_index_maps_formats() {
        assert_eq!(filter_index("png"), 1);
        assert_eq!(filter_index("PNG"), 1);
        assert_eq!(filter_index("jpg"), 2);
        assert_eq!(filter_index("jpeg"), 2);
        assert_eq!(filter_index("webp"), 3);
        assert_eq!(filter_index("bmp"), 1, "未知扩展名回退 PNG");
    }
}
