// SPDX-License-Identifier: GPL-3.0-only
//! 长截图调试日志（SYS-9 / ADR 0046，issue #200）：把每步状态写入
//! `%TEMP%\airsnip\airsnip-longshot.log`，用于定位滚动/拼接链路问题（M4 排障）。
//!
//! **日志常开，没有开关**（ADR 0046）：它每拍只有一行状态文本（几 KB，不含画面），
//! 待机不写（调用点全在长截图会话里），出问题时用户不必先想着去开什么开关。
//! 普通用户唯一要做的动作是「打开日志文件夹」把文件发过来。
//!
//! 高成本的**帧抽样**（每 25 拍落一对全屏 PNG，编码几十 ms/张）只在环境变量
//! `AIRSNIP_DEBUG` 下做（[`verbose`]）；异常时刻的帧对落盘（「画面变了但判定为静止」/
//! 定位失败）不受它影响，见 [`dump_pair`]。
//!
//! **测试里一律不写**（[`enabled`]）：拼接用例的模拟帧源会在几毫秒内跑几十拍，
//! 不关会把真实用户的日志写成一团噪音，还会在 `%TEMP%\airsnip` 里丢一堆测试帧 PNG。
//!
//! 除了日志，还能在可疑时刻把两帧落成 PNG
//! （`airsnip-longshot-<tag>-<n>-prev/new.png`），用来判断到底是画面真没动、
//! 还是我们抓到的帧是旧的。日志与 PNG 都在同一个目录里，「打开日志文件夹」直接开它。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 打开中的日志文件与已写字节数（超过上限时轮转，避免长期开着无限增长）。
struct Sink {
    file: std::fs::File,
    bytes: u64,
}

static LOG: Mutex<Option<Sink>> = Mutex::new(None);
/// 本次运行第几次落盘（限制次数，避免一次会话写出上百张全屏图）。
static DUMPS: Mutex<u32> = Mutex::new(0);
/// 落盘上限。
const MAX_DUMPS: u32 = 6;
/// 日志体积上限：超过就轮转成 `.log.1`（覆盖更旧的一份）。
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;

/// 环境变量 `AIRSNIP_DEBUG`：只读一次（`verbose()` 在每拍上都会被调用）。
static ENV: OnceLock<bool> = OnceLock::new();

/// 日志 / 帧落盘是否启用：生产环境**常开**（ADR 0046），单元测试里关闭。
///
/// 关掉不是为了「省开销」，而是隔离——测试的模拟帧源几毫秒就能跑几十拍，
/// 写进真实路径的结果既是噪音、又会污染用户的日志文件（实测混过 258 行）。
pub fn enabled() -> bool {
    !cfg!(test)
}

/// 是否做高成本的采样（每 25 拍把一对帧落成 PNG）。
///
/// 文本日志常开；帧 PNG 在 4K 下编码几十毫秒/张，每几秒一次会被感知为卡顿，
/// 所以留一个环境变量台阶给开发者（普通用户复现问题靠异常帧落盘已够）。
pub fn verbose() -> bool {
    *ENV.get_or_init(|| std::env::var_os("AIRSNIP_DEBUG").is_some())
}

/// 日志目录名（`%TEMP%\airsnip`）。反馈文本也要报这个目录，
/// 两处写一份字面量会漂。
pub const DIR_NAME: &str = "airsnip";

/// 日志目录 `%TEMP%\airsnip`（日志与帧 PNG 都落在这里）。
/// 不存在就建出来——否则「打开日志文件夹」会落到一个不存在的路径上。
pub fn log_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(DIR_NAME);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn log_path() -> PathBuf {
    log_dir().join("airsnip-longshot.log")
}

/// 写一行日志（生产环境常开，单元测试里不写）。
pub fn log(msg: &str) {
    if !enabled() {
        return;
    }
    let mut guard = LOG.lock().unwrap_or_else(|p| p.into_inner());
    // 超过上限：关掉当前句柄、把旧文件改名让位，下一段从空文件重新写。
    // 轮转失败（文件被别的程序打开，Windows 上 rename 会拒绝）时把计数归零：
    // 不归零的话之后每行日志都会再试一次 rename + 重开文件。
    if guard.as_ref().is_some_and(|s| s.bytes > MAX_LOG_BYTES) {
        let path = log_path();
        *guard = None;
        if !rotate(&path)
            && let Some(mut s) = open(&path)
        {
            s.bytes = 0;
            *guard = Some(s);
        }
    }
    // 路径放在「确实要开文件」时才算：`log_dir()` 每次都 `create_dir_all`，
    // 每行日志都做一次系统调用没必要
    if guard.is_none() {
        *guard = open(&log_path());
    }
    let ts = super::window_start_ms();
    let line = format!("[{ts}] {msg}\n");
    if let Some(s) = guard.as_mut() {
        if s.file.write_all(line.as_bytes()).is_ok() {
            s.bytes += line.len() as u64;
        }
    }
}

/// 追加打开日志文件；已写字节数从文件现有大小起算（上一次运行留下的也算）。
fn open(path: &Path) -> Option<Sink> {
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
    let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some(Sink { file, bytes })
}

/// 把旧日志改名成 `<名字>.1`（已存在的 `.1` 直接覆盖），下一段从空文件起。
/// 返回是否轮转成功（失败时调用方不能把已写字节清零，否则会反复重试）。
fn rotate(path: &Path) -> bool {
    let mut old = path.as_os_str().to_owned();
    old.push(".1");
    std::fs::rename(path, PathBuf::from(old)).is_ok()
}

/// 把一对帧落成 PNG（`tag` 标明时机），最多 `MAX_DUMPS` 次。
///
/// 调用方只在**异常时刻**调（画面变了却判无位移 / 定位失败），或开发者打开了
/// [`verbose`] 的每 25 拍采样；这里不再判开关。
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
    let dir = log_dir();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 临时目录下的一次性路径（按用例名区分，跑完自己清）。
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("airsnip-debug-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 轮转：旧日志改名成 `.1`，同名 `.1` 被覆盖，原路径回到「不存在」状态。
    #[test]
    fn rotate_moves_log_aside() {
        let dir = scratch("rotate");
        let path = dir.join("airsnip-longshot.log");
        std::fs::write(&path, b"first").unwrap();
        std::fs::write(dir.join("airsnip-longshot.log.1"), b"stale").unwrap();

        rotate(&path);

        assert!(!path.exists(), "原名应被让位");
        assert_eq!(std::fs::read(dir.join("airsnip-longshot.log.1")).unwrap(), b"first");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 首次打开把文件现有大小算进已写字节数：上一次运行留下的 5MB 日志不会
    /// 被当成本段从 0 起，否则要再写 4MB 才轮转。
    #[test]
    fn open_starts_from_existing_size() {
        let dir = scratch("open");
        let path = dir.join("airsnip-longshot.log");
        std::fs::write(&path, vec![b'x'; 1024]).unwrap();
        let sink = open(&path).expect("应能追加打开");
        assert_eq!(sink.bytes, 1024);
        drop(sink);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 开关：生产常开、测试关闭；`verbose` 只跟环境变量。
    #[test]
    fn logs_are_off_in_tests_but_on_in_production() {
        assert!(!enabled(), "测试里不应写真实日志（会污染 %TEMP%\\airsnip）");
        assert_eq!(verbose(), std::env::var_os("AIRSNIP_DEBUG").is_some());
        assert_eq!(verbose(), verbose());
    }

    /// 日志目录必须是 `%TEMP%\airsnip`，且调用后确实存在（「打开日志文件夹」依赖它）。
    #[test]
    fn log_dir_is_created() {
        let dir = log_dir();
        assert!(dir.is_dir(), "{}", dir.display());
        assert_eq!(dir.file_name().unwrap(), "airsnip");
    }
}
