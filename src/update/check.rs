// SPDX-License-Identifier: GPL-3.0-only
//! 检查更新（SYS-7）：按镜像顺序取 `latest.json` → 验签 → 比版本。
//!
//! 在后台线程跑（网络 I/O 不能碰主消息循环），结果写入 `state`。

use super::Update;
use super::endpoints;
use super::http;
use super::manifest::Manifest;
use super::state::{self, Phase};
use super::version::{self, Version};

/// 检查结果。
pub enum Checked {
    /// 已是最新。
    UpToDate,
    /// 发现新版本（已验签）。
    Available(Update),
}

/// 同步检查：依次尝试各镜像，第一个成功解析并验签的清单生效。
/// 用户点了「跳过此版本」时，该版本的清单视为「已是最新」。
pub fn check(skipped: Option<&str>) -> Result<Checked, String> {
    let mut errors = Vec::new();
    for url in endpoints::candidates(endpoints::LATEST_MANIFEST) {
        match fetch_manifest(&url) {
            Ok(manifest) => {
                // 验签在最前面：不验签就不该相信清单里的任何字段
                manifest.verify()?;
                let remote = Version::parse(&manifest.version)
                    .ok_or_else(|| format!("清单版本号无法解析：{}", manifest.version))?;
                if let Some(skip) = skipped
                    && skip == manifest.version
                {
                    return Ok(Checked::UpToDate);
                }
                if !remote.is_newer_than(&version::current()) {
                    return Ok(Checked::UpToDate);
                }
                return Ok(Checked::Available(Update {
                    manifest,
                    version: remote,
                }));
            }
            // 单个镜像失败继续试下一个；全部失败时把最后一个原因带出去
            Err(e) => errors.push(e),
        }
    }
    Err(errors.pop().unwrap_or_else(|| "没有可用的下载线路".into()))
}

/// 取并解析清单（不做验签，由调用方统一处理）。
fn fetch_manifest(url: &str) -> Result<Manifest, String> {
    let bytes = http::get_bytes(url)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("清单格式非法：{e}"))
}

/// 后台检查并在结束后写回全局状态；发现新版本时回调 `on_found`（用于弹托盘提示）。
pub fn check_in_background(skipped: Option<String>) {
    if state::is_busy() {
        return;
    }
    state::set(Phase::Checking);
    std::thread::spawn(move || {
        let next = match check(skipped.as_deref()) {
            Ok(Checked::UpToDate) => Phase::UpToDate,
            Ok(Checked::Available(u)) => {
                let version = u.manifest.version.clone();
                super::set_pending(u);
                // 静默检查也要让用户知道有新版本（不弹模态框，走托盘气泡）
                super::notify::wake_main_on_update();
                Phase::Available { version }
            }
            Err(e) => Phase::Failed(e),
        };
        state::set(next);
        // 通知界面刷新（主窗口句柄由设置面板打开时登记）
        super::notify::wake_ui();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 清单解析：缺少必填字段应报错，多余字段应忽略（向前兼容）。
    #[test]
    fn manifest_json_shape() {
        let ok = r#"{"version":"0.7.0","url":"https://github.com/x/y.exe","sha256":"ab","signature":"cd","extra":1}"#;
        let m: Manifest = serde_json::from_str(ok).expect("应能解析");
        assert_eq!(m.version, "0.7.0");
        assert_eq!(m.notes, "", "可选字段缺省为空串");

        let bad = r#"{"version":"0.7.0"}"#;
        assert!(
            serde_json::from_str::<Manifest>(bad).is_err(),
            "缺 url/sha256/signature 应报错"
        );
    }
}
