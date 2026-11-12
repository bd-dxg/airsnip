// SPDX-License-Identifier: GPL-3.0-only
//! 保存路径与文件名（OUT-3）：`$XDG_PICTURES_DIR/airsnip-YYYYMMDD-HHMMSS.png`。
//!
//! S2 不做「另存为」对话框（Wayland 上要用 xdg-desktop-portal 的 FileChooser，
//! 属于 S5 的范围），先固定落到图片目录并打印路径。
//!
//! 时间戳优先用系统 `date`（保证是**本地时间**），失败时退回自己算的 UTC
//! 民用历——不为了格式化日期引一个时区数据库依赖。

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// 读 `~/.config/user-dirs.dirs` 里的某个 XDG 目录。
fn xdg_dir(key: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(home().join(".config/user-dirs.dirs")).ok()?;
    let home_str = home().display().to_string();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(&format!("{key}=")) else {
            continue;
        };
        let v = rest.trim().trim_matches('"').replace("$HOME", &home_str);
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    None
}

/// 图片目录。
pub fn pictures_dir() -> PathBuf {
    xdg_dir("XDG_PICTURES_DIR")
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join("Pictures"))
}

/// 下一个保存路径（目录不存在则创建）。
pub fn next_path() -> PathBuf {
    let dir = pictures_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("airsnip-{}.png", stamp()))
}

/// 本地时间戳 `YYYYMMDD-HHMMSS`。
fn stamp() -> String {
    local_stamp().unwrap_or_else(utc_stamp)
}

fn local_stamp() -> Option<String> {
    let out = Command::new("date").arg("+%Y%m%d-%H%M%S").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.len() == 15 { Some(s) } else { None }
}

fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let tod = secs.rem_euclid(86400);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// 天数（自 1970-01-01）→ 民用历（Howard Hinnant 算法）。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19782), (2024, 2, 29)); // 闰日
        assert_eq!(civil_from_days(20000), (2024, 10, 4));
    }

    #[test]
    fn stamp_shape() {
        let s = utc_stamp();
        assert_eq!(s.len(), 15);
        assert_eq!(&s[8..9], "-");
    }

    #[test]
    fn path_lives_in_pictures() {
        let p = next_path();
        assert_eq!(p.extension().unwrap(), "png");
        assert!(p.file_name().unwrap().to_string_lossy().starts_with("airsnip-"));
    }
}
