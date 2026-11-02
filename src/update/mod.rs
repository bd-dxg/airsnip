// SPDX-License-Identifier: GPL-3.0-only
//! 自动更新（SYS-7 / 看板 #59）：启动静默检查 → 加速线路下载 → 验签 + 校验和 →
//! 独立更新器替换 exe 并重启。
//!
//! 模块分工：`manifest` 清单与验签、`version` 版本比较、`endpoints` 镜像候选、
//! `http` WinHTTP 客户端、`install` 下载与替换、`state` 跨线程状态。
//!
//! 性能约束：只在启动后触发一次 + 手动点击触发，不引入常驻轮询线程；
//! 所有网络请求在后台线程，不阻塞主消息循环。

pub mod check;
pub mod endpoints;
pub mod http;
pub mod install;
pub mod manifest;
pub mod notify;
pub mod state;
pub mod version;

use std::sync::{Mutex, OnceLock};

/// 已通过验签的待安装更新。
pub struct Update {
    /// 清单本体（下载时用其中的 url / sha256）。
    pub manifest: manifest::Manifest,
    /// 解析后的版本号。
    pub version: version::Version,
}

/// 待安装更新（检查时写入，安装时取走）。
fn pending() -> &'static Mutex<Option<Update>> {
    static PENDING: OnceLock<Mutex<Option<Update>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

/// 记录检查到的新版本。
pub fn set_pending(u: Update) {
    if let Ok(mut g) = pending().lock() {
        *g = Some(u);
    }
}

/// 取走待安装更新（一次性）。
pub fn take_pending() -> Option<Update> {
    pending().lock().ok().and_then(|mut g| g.take())
}

/// 一键更新（SYS-7）：下载 → 校验 → 启动替换脚本。
/// 调用方负责在返回 Ok 后让主进程退出（脚本会等待本进程结束）。
pub fn download_and_install(u: &Update) -> Result<(), String> {
    if state::is_busy() {
        return Err("已有更新任务在进行中".into());
    }
    let version = format!("{}.{}.{}", u.version.major, u.version.minor, u.version.patch);
    state::set(state::Phase::Downloading {
        version: version.clone(),
        percent: 0,
    });
    // 下载在当前线程（后台线程）完成：进度通过 `state` 回传界面
    let result = install::download(&u.version, &u.manifest.url, &u.manifest.sha256);
    let new_exe = match result {
        Ok(p) => p,
        Err(e) => {
            state::set(state::Phase::Failed(e.clone()));
            return Err(e);
        }
    };
    state::set(state::Phase::Installing {
        version: version.clone(),
    });
    let target = match install::current_exe() {
        Ok(t) => t,
        Err(e) => {
            state::set(state::Phase::Failed(e.clone()));
            return Err(e);
        }
    };
    if let Err(e) = install::spawn_replacer(&new_exe, &target) {
        state::set(state::Phase::Failed(e.clone()));
        return Err(e);
    }
    Ok(())
}
