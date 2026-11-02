// SPDX-License-Identifier: GPL-3.0-only
//! 下载端点（SYS-7）：GitHub 直连在国内多数网络不可用，优先走加速镜像。
//!
//! 策略：
//! - **清单与安装包都按同一顺序试**：镜像域名只是前缀，后面接 GitHub 规范地址，
//!   所以清单里存规范地址即可，换镜像不用重新发版（见 ADR 0018）。
//! - 顺序：三个 `gh-proxy` 镜像 → GitHub 直连兜底。任何一个成功就停止尝试。

/// GitHub 规范地址：最新版清单。
pub const LATEST_MANIFEST: &str = "https://github.com/bd-dxg/airsnip/releases/latest/download/latest.json";

/// 加速镜像前缀（镜像站把 GitHub 地址原样接在后面）。
pub const MIRRORS: [&str; 3] = [
    "https://gh-proxy.org/",
    "https://cdn.gh-proxy.org/",
    "https://v4.gh-proxy.org/",
];

/// 某个 GitHub 地址的全部候选下载地址（镜像优先，直连最后）。
/// 已经是非 GitHub 地址时不加前缀（清单里若写了完整镜像地址也能照常工作）。
pub fn candidates(github_url: &str) -> Vec<String> {
    let is_github = github_url.starts_with("https://github.com/");
    let mut list: Vec<String> = MIRRORS
        .iter()
        .filter(|_| is_github)
        .map(|m| format!("{m}{github_url}"))
        .collect();
    list.push(github_url.to_string());
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_come_first_and_direct_is_last() {
        let list = candidates(LATEST_MANIFEST);
        assert_eq!(list.len(), MIRRORS.len() + 1);
        // 每个镜像项 = 镜像前缀 + 原始 GitHub 地址
        let path = LATEST_MANIFEST.trim_start_matches("https://");
        for (i, mirror) in MIRRORS.iter().enumerate() {
            assert_eq!(list[i], format!("{mirror}https://{path}"), "第 {i} 个应为镜像地址");
        }
        assert_eq!(list.last().unwrap(), LATEST_MANIFEST, "直连必须兜底在最后");
    }

    #[test]
    fn non_github_url_is_untouched() {
        let url = "https://example.com/pkg.exe";
        assert_eq!(candidates(url), vec![url.to_string()], "非 GitHub 地址不加镜像前缀");
    }
}
