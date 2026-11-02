// SPDX-License-Identifier: GPL-3.0-only
//! 版本比较（SYS-7）：只处理本项目的三段式版本号，不引入 semver crate。
//! 预发布后缀（`-beta`）一律视为小于同号正式版，避免测试包被当成新版本推送。

/// 解析后的版本号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// 是否有预发布后缀（`-` 之后的部分）。
    pub pre: bool,
}

impl Version {
    /// 解析 `1.2.3` / `1.2.3-beta.1`；非法返回 None。
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('v');
        let (core, pre) = match s.split_once('-') {
            Some((c, _)) => (c, true),
            None => (s, false),
        };
        let mut it = core.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it.next().unwrap_or("0").parse().ok()?;
        let patch = it.next().unwrap_or("0").parse().ok()?;
        if it.next().is_some() {
            return None; // 四段以上不认识
        }
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    /// 远端版本是否比当前版本新。
    /// 同号带预发布后缀时视为**不更新**（`0.7.0-beta` 不应推给 `0.7.0`）。
    pub fn is_newer_than(&self, current: &Self) -> bool {
        let (a, b) = (
            (self.major, self.minor, self.patch),
            (current.major, current.minor, current.patch),
        );
        if a != b {
            return a > b;
        }
        !self.pre && current.pre
    }
}

/// 当前运行版本。
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version {
        major: 0,
        minor: 0,
        patch: 0,
        pre: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or_else(|| panic!("应能解析 {s}"))
    }

    #[test]
    fn parses_three_segments_and_prefix() {
        assert_eq!(v("v0.7.0").major, 0);
        assert_eq!(v("1.2").patch, 0, "缺段按 0 补");
        assert!(Version::parse("1.2.3.4").is_none(), "四段应拒绝");
        assert!(Version::parse("abc").is_none());
    }

    #[test]
    fn newer_comparison() {
        assert!(v("0.7.0").is_newer_than(&v("0.6.0")));
        assert!(v("0.6.1").is_newer_than(&v("0.6.0")));
        assert!(!v("0.6.0").is_newer_than(&v("0.6.0")), "同版本不算更新");
        assert!(!v("0.5.9").is_newer_than(&v("0.6.0")), "旧版本不算更新（防降级）");
        // 预发布不推给正式版
        assert!(!v("0.7.0-beta").is_newer_than(&v("0.7.0")));
        assert!(v("0.7.0").is_newer_than(&v("0.7.0-beta")), "正式版覆盖预发布");
    }

    #[test]
    fn current_matches_cargo_version() {
        assert_eq!(current(), v(env!("CARGO_PKG_VERSION")));
    }
}
