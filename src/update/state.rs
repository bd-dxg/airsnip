// SPDX-License-Identifier: GPL-3.0-only
//! 更新状态（SYS-7）：跨线程共享检查/下载进度，供设置面板「关于」页展示。
//!
//! 后台线程写、界面线程读，只在状态切换时加锁，不持有锁做 I/O。

use std::sync::{Mutex, OnceLock};

/// 更新流程所处阶段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// 尚未检查过。
    Idle,
    /// 正在检查（后台线程）。
    Checking,
    /// 已是最新版本。
    UpToDate,
    /// 发现新版本（尚未下载）。
    Available {
        /// 新版本号。
        version: String,
    },
    /// 正在下载安装包，`0..=100`。
    Downloading {
        /// 新版本号。
        version: String,
        /// 进度百分比。
        percent: u8,
    },
    /// 下载与校验均通过，正在替换并重启。
    Installing {
        /// 新版本号。
        version: String,
    },
    /// 检查或下载失败（展示给用户的原因）。
    Failed(String),
}

/// 全局状态（进程内单例）。
fn slot() -> &'static Mutex<Phase> {
    static PHASE: OnceLock<Mutex<Phase>> = OnceLock::new();
    PHASE.get_or_init(|| Mutex::new(Phase::Idle))
}

/// 读取当前阶段。
pub fn phase() -> Phase {
    slot().lock().map(|g| g.clone()).unwrap_or(Phase::Idle)
}

/// 写入当前阶段。
pub fn set(next: Phase) {
    if let Ok(mut g) = slot().lock() {
        *g = next;
    }
}

/// 是否处于可以发起操作的阶段（空闲 / 已是最新 / 失败 / 发现新版本）。
/// 检查中、下载中、安装中禁止重入。
pub fn is_busy() -> bool {
    is_busy_phase(&phase())
}

/// `is_busy` 的纯函数版本（测试用，不读全局单例）。
pub fn is_busy_phase(p: &Phase) -> bool {
    matches!(
        p,
        Phase::Checking | Phase::Downloading { .. } | Phase::Installing { .. }
    )
}

/// 关于页更新按钮：文案 + 是否可点（检查中/下载中/安装中置灰）。
pub fn button(phase: &Phase) -> (String, bool) {
    match phase {
        Phase::Idle => ("检查更新".to_string(), true),
        Phase::Checking => ("检查中…".to_string(), false),
        // 已是最新仍可点：过一段时间想手动再查一次
        Phase::UpToDate => ("已是最新".to_string(), true),
        Phase::Available { version } => (format!("更新到 v{version}"), true),
        Phase::Downloading { percent, .. } => (format!("下载中 {percent}%"), false),
        Phase::Installing { .. } => ("正在安装…".to_string(), false),
        Phase::Failed(_) => ("重试".to_string(), true),
    }
}

/// 是否用强调样式（有可执行动作时用强调底色，其余用素色）。
pub fn button_emphasis(phase: &Phase) -> bool {
    matches!(phase, Phase::Available { .. } | Phase::Failed(_))
}

/// 失败原因（按钮位置放不下详细文案，保留供调试与后续展示）。
pub fn failure_reason(phase: &Phase) -> Option<&str> {
    match phase {
        Phase::Failed(e) => Some(e),
        _ => None,
    }
}

/// 点击该行时是否应触发动作。
pub fn is_actionable(phase: &Phase) -> bool {
    matches!(
        phase,
        Phase::Idle | Phase::UpToDate | Phase::Failed(_) | Phase::Available { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_text_and_enabled_state() {
        assert_eq!(button(&Phase::Idle), ("检查更新".to_string(), true));
        assert!(!button(&Phase::Checking).1, "检查中不可点");
        assert_eq!(
            button(&Phase::Available {
                version: "0.7.0".into()
            }),
            ("更新到 v0.7.0".to_string(), true)
        );
        assert_eq!(
            button(&Phase::Downloading {
                version: "0.7.0".into(),
                percent: 42
            })
            .0,
            "下载中 42%"
        );
        assert!(!button(&Phase::Installing { version: "1".into() }).1);
        assert_eq!(button(&Phase::Failed("超时".into())), ("重试".to_string(), true));
        assert_eq!(failure_reason(&Phase::Failed("超时".into())), Some("超时"));
        assert!(!button_emphasis(&Phase::Idle), "空闲不强调");
        assert!(button_emphasis(&Phase::Available { version: "1".into() }));
    }

    #[test]
    fn busy_phases_block_reentry() {
        assert!(!is_busy_phase(&Phase::Idle));
        assert!(is_busy_phase(&Phase::Checking));
        assert!(is_busy_phase(&Phase::Downloading {
            version: "1".into(),
            percent: 1
        }));
        assert!(is_busy_phase(&Phase::Installing { version: "1".into() }));
        assert!(!is_busy_phase(&Phase::Available { version: "1".into() }));
        assert!(!is_busy_phase(&Phase::UpToDate));
    }
}
