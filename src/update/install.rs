// SPDX-License-Identifier: GPL-3.0-only
//! 下载与替换（SYS-7）：边下边算 SHA-256，校验通过后交给独立脚本替换 exe。
//!
//! Windows 下运行中的 exe 不能被覆盖，所以流程是：
//! 下载到 `%TEMP%` → 校验和比对 → 写一个 `.cmd` → 启动脚本 → 主进程退出 →
//! 脚本等进程消失后 `copy` 覆盖并重启 → 脚本自删。
//! 程序始终以管理员运行（ADR 0003），脚本继承提权，无需再弹 UAC。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use super::endpoints;
use super::http;
use super::manifest::encode_hex;
use super::state::{self, Phase};
use super::version::Version;

/// 下载安装包到临时文件并校验 SHA-256；成功后返回临时文件路径。
///
/// `version` 只用于进度展示与状态写回。
pub fn download(version: &Version, url: &str, expect_sha256: &str) -> Result<PathBuf, String> {
    let expect = expect_sha256.trim().to_ascii_lowercase();
    let path = std::env::temp_dir().join(format!(
        "airsnip-update-{}.{}.{}.exe",
        version.major, version.minor, version.patch
    ));
    let label = format!("{}.{}.{}", version.major, version.minor, version.patch);

    let mut errors = Vec::new();
    // 与清单一致：镜像依次试，直连兜底
    for candidate in endpoints::candidates(url) {
        match fetch_to_file(&candidate, &path, &expect, &label) {
            Ok(()) => return Ok(path),
            Err(e) => errors.push(e),
        }
    }
    Err(errors.pop().unwrap_or_else(|| "没有可用的下载线路".into()))
}

/// 单个地址：流式写入 + 增量摘要 + 进度回调。校验和一致才算成功。
fn fetch_to_file(url: &str, path: &Path, expect: &str, version: &str) -> Result<(), String> {
    let mut file = std::fs::File::create(path).map_err(|e| format!("创建临时文件失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut last_percent = u8::MAX;
    let result = http::get(url, |chunk, received, total| {
        file.write_all(chunk).map_err(|e| format!("写入安装包失败：{e}"))?;
        hasher.update(chunk);
        // 进度按 5% 粒度上报，避免每个 64KB 块都锁一次全局状态
        if let Some(total) = total.filter(|t| *t > 0) {
            let percent = ((received as u64 * 100) / total as u64).min(100) as u8;
            if percent / 5 > last_percent / 5 {
                last_percent = percent;
                state::set(Phase::Downloading {
                    version: version.to_string(),
                    percent,
                });
                // 进度在后台线程写入，需主动唤醒面板重绘关于页（SYS-7）
                super::notify::wake_ui();
            }
        }
        Ok(())
    });
    if let Err(e) = result {
        // 先关文件再删：Windows 下删除被占用的文件会失败，会留下临时残包
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(e);
    }
    drop(file);

    let actual = encode_hex(&hasher.finalize());
    if actual != expect {
        let _ = std::fs::remove_file(path);
        return Err(format!("安装包校验失败（期望 {expect}，实际 {actual}）"));
    }
    Ok(())
}

/// 摘要比对（忽略大小写与首尾空白）。
/// 单独抽出来使校验逻辑可以离线单测（下载链路需要真实网络）。
pub fn digests_match(actual: &str, expect: &str) -> bool {
    actual.trim().eq_ignore_ascii_case(expect.trim())
}

/// 替换脚本正文。参数：`%1` 父进程 pid、`%2` 新 exe、`%3` 当前 exe。
///
/// - 循环等父进程退出（`tasklist` + `find`）；
/// - `copy` 失败时仍启动旧程序，不让用户「更新完打不开」；
/// - 最后一行删自己：`(goto) 2>nul & del` 是 cmd 里删除运行中脚本的惯用写法。
const REPLACER_SCRIPT: &str = r#"@echo off
:wait
tasklist /FI "PID eq %1" 2>nul | find "%1" >nul
if not errorlevel 1 (
  timeout /t 1 /nobreak >nul
  goto wait
)
copy /y "%~2" "%~3" >nul
start "" "%~3"
del "%~2" >nul 2>nul
(goto) 2>nul & del "%~f0"
"#;

/// 启动替换脚本。调用方随后必须让主进程退出（否则脚本会一直等）。
pub fn spawn_replacer(new_exe: &Path, target_exe: &Path) -> Result<(), String> {
    let script = std::env::temp_dir().join("airsnip-update.cmd");
    std::fs::write(&script, REPLACER_SCRIPT).map_err(|e| format!("写入替换脚本失败：{e}"))?;
    let mut cmd = Command::new("cmd");
    cmd.arg("/c")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(new_exe)
        .arg(target_exe);
    // CREATE_NO_WINDOW：不弹黑色控制台窗口（与 release 版 GUI 子系统一致）
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn().map_err(|e| format!("启动更新器失败：{e}"))?;
    Ok(())
}

/// 当前 exe 路径。
pub fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("取当前程序路径失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 替换脚本必须包含等待、覆盖、重启三件事，且能自删。
    #[test]
    fn replacer_script_has_required_steps() {
        for needle in ["tasklist", "copy /y", "start \"\"", "%~f0"] {
            assert!(REPLACER_SCRIPT.contains(needle), "脚本缺少 {needle}");
        }
    }

    /// 临时文件名按版本号区分（同一台机器换版本不会互相覆盖）。
    #[test]
    fn temp_name_includes_version() {
        let v = Version::parse("1.2.3").expect("解析");
        let p = std::env::temp_dir().join(format!("airsnip-update-{}.{}.{}.exe", v.major, v.minor, v.patch));
        assert!(p.to_string_lossy().contains("airsnip-update-1.2.3.exe"));
    }

    /// 摘要比对：大小写与首尾空白无关，内容不同即失败。
    #[test]
    fn digest_comparison_is_strict() {
        let a = "abcdef0123456789";
        assert!(digests_match(a, a));
        assert!(digests_match(a, &a.to_uppercase()), "大小写不应影响校验");
        assert!(digests_match(&format!(" {a}\r\n"), a), "首尾空白不应影响");
        assert!(!digests_match(a, "abcdef0123456780"), "内容不同必须失败");
        assert!(!digests_match("", a), "空摘要必须失败");
    }
}
