// SPDX-License-Identifier: GPL-3.0-only
//! 上次保存格式与目录的运行时记忆（OUT-2）：覆盖层线程写入，主线程在截图结束时
//! 取出并写回配置（`Config::last_save_ext` / `last_save_dir`）。
//! 从 `app` 拆出：这两项属于输出职责，且 `app/mod.rs` 已接近单文件规模上限。

/// 上次保存选定的格式扩展名。
static LAST_SAVE_EXT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 上次保存所在的目录。
static LAST_SAVE_DIR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 记录本次保存用的格式扩展名（覆盖层线程调用）。
pub fn last_save_ext_store(ext: &str) {
    if let Ok(mut g) = LAST_SAVE_EXT.lock() {
        *g = Some(ext.to_ascii_lowercase());
    }
}

/// 记录本次保存所在的目录（覆盖层线程调用）。
pub fn last_save_dir_store(dir: &std::path::Path) {
    if let Ok(mut g) = LAST_SAVE_DIR.lock() {
        *g = Some(dir.to_string_lossy().into_owned());
    }
}

/// 取出并清空记录的扩展名（主线程写回配置用）。
pub fn last_save_ext_take() -> Option<String> {
    LAST_SAVE_EXT.lock().ok().and_then(|mut g| g.take())
}

/// 取出并清空记录的目录（主线程写回配置用）。
pub fn last_save_dir_take() -> Option<String> {
    LAST_SAVE_DIR.lock().ok().and_then(|mut g| g.take())
}
