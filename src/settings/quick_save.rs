// SPDX-License-Identifier: GPL-3.0-only
//! 快速保存配置（OUT-2 / ADR 0041）：**目录 + 文件名模板**（分开）+ 格式 + 同名策略 +
//! 通知 / 剪贴板开关。
//!
//! 为什么把路径拆成两段：目录靠资源管理器选（用户不该手打一长串路径），文件名才需要
//! 手改与变量。渲染时拼成 `目录\文件名.扩展名`（见 `output::quick`）。

use serde::{Deserialize, Serialize};

/// 默认保存目录（`%USERPROFILE%\Pictures\airsnip`，沿用旧默认位置）。
pub const DEFAULT_DIR: &str = r"%USERPROFILE%\Pictures\airsnip";
/// 默认文件名模板（不含扩展名；与旧的「日期 + 时间」命名等价）。
pub const DEFAULT_NAME: &str = "airsnip_$yyyy$-$MM$-$dd$_$HH$-$mm$-$ss$";

/// 快速保存的图片格式（扩展名）（OUT-2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SaveFormat {
    /// `.png`（默认，无损）。
    #[default]
    #[serde(rename = "png")]
    Png,
    /// `.jpg`（质量 90）。
    #[serde(rename = "jpg")]
    Jpeg,
    /// `.webp`（无损 VP8L）。
    #[serde(rename = "webp")]
    Webp,
}

impl SaveFormat {
    pub const ALL: [SaveFormat; 3] = [SaveFormat::Png, SaveFormat::Jpeg, SaveFormat::Webp];

    /// 扩展名（与 `output::save_file` 的分派口径一致）。
    pub fn ext(self) -> &'static str {
        match self {
            SaveFormat::Png => "png",
            SaveFormat::Jpeg => "jpg",
            SaveFormat::Webp => "webp",
        }
    }

    /// 下拉按钮文字（带点，和路径预览里的写法一致）。
    pub fn title(self) -> &'static str {
        match self {
            SaveFormat::Png => ".png",
            SaveFormat::Jpeg => ".jpg",
            SaveFormat::Webp => ".webp",
        }
    }
}

/// 目标文件已存在时的处理（OUT-2 / ADR 0041）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExistingFile {
    /// 自动追加 `-1`、`-2`…（默认）：永不出错，也永不丢数据。
    #[default]
    #[serde(rename = "rename")]
    Rename,
    /// 直接覆盖。
    #[serde(rename = "overwrite")]
    Overwrite,
}

impl ExistingFile {
    pub const ALL: [ExistingFile; 2] = [ExistingFile::Rename, ExistingFile::Overwrite];

    pub fn title(self) -> &'static str {
        match self {
            ExistingFile::Rename => "自动加序号",
            ExistingFile::Overwrite => "覆盖",
        }
    }
}

/// 快速保存配置（OUT-2 / ADR 0041，设置面板「保存」页）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickSaveConfig {
    /// 保存目录（资源管理器选择）。`None` = 未设置，用 [`DEFAULT_DIR`]。
    /// 仍支持 `%VAR%` 展开（迁移自旧的整条路径模板时目录段可能带变量）。
    pub dir: Option<String>,
    /// 文件名模板（**不含扩展名**，可含 `$yyyy$` 等变量）。`None` = 用 [`DEFAULT_NAME`]。
    pub name: Option<String>,
    pub format: SaveFormat,
    pub on_exist: ExistingFile,
    /// 保存成功后弹覆盖层内的轻提示（可点：定位到刚存的文件）。
    pub notify: bool,
    /// 保存成功后同时把同一张图放进剪贴板。
    pub also_copy: bool,
    /// **遗留**：0.13 开发期只有一个「整条路径模板」字段，仅用于迁移拆分到 `dir` / `name`。
    /// 不再写入（`skip_serializing`），迁移后清空。
    #[serde(default, skip_serializing, rename = "template")]
    pub legacy_template: Option<String>,
}

impl Default for QuickSaveConfig {
    fn default() -> Self {
        Self {
            dir: None,
            name: None,
            format: SaveFormat::Png,
            on_exist: ExistingFile::Rename,
            notify: true,
            also_copy: false,
            legacy_template: None,
        }
    }
}

impl QuickSaveConfig {
    /// 生效目录（未设置时用默认）。
    pub fn dir_or_default(&self) -> String {
        self.dir
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or(DEFAULT_DIR)
            .to_string()
    }

    /// 生效文件名模板（未设置时用默认）。
    pub fn name_or_default(&self) -> String {
        self.name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(DEFAULT_NAME)
            .to_string()
    }

    /// 拼成渲染器要的完整路径模板（目录 + 文件名，**不含扩展名**）。
    /// 目录末尾的分隔符去掉，避免出现 `D:\shots\\name`。
    pub fn template(&self) -> String {
        let dir = self.dir_or_default();
        let dir = dir.trim_end_matches(['\\', '/']);
        format!("{dir}\\{}", self.name_or_default())
    }

    /// 迁移（0.13 开发期 → 正式）：把遗留的整条路径模板拆成「目录 + 文件名」。
    /// 只在两段都没设置时生效，用户已经分别填过就不动。
    pub fn migrate(&mut self) {
        if self.dir.is_some() || self.name.is_some() {
            return;
        }
        let Some(t) = self.legacy_template.take() else {
            return;
        };
        let t = t.trim();
        if t.is_empty() {
            return;
        }
        match t.rsplit_once(['\\', '/']) {
            Some((dir, name)) => {
                if !dir.is_empty() {
                    self.dir = Some(dir.to_string());
                }
                if !name.is_empty() {
                    self.name = Some(name.to_string());
                }
            }
            // 只有文件名、没有目录：只用它做文件名，目录走默认
            None => self.name = Some(t.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认值：目录走 `Pictures\airsnip`、文件名是日期 + 时间模板；
    /// 同名自动加序号、显示通知、不复制剪贴板、PNG。
    #[test]
    fn defaults_match_decisions() {
        let d = QuickSaveConfig::default();
        assert_eq!(d.format, SaveFormat::Png);
        assert_eq!(d.on_exist, ExistingFile::Rename);
        assert!(d.notify, "通知默认开");
        assert!(!d.also_copy, "复制剪贴板默认关");
        assert_eq!(d.dir_or_default(), DEFAULT_DIR);
        assert_eq!(d.name_or_default(), DEFAULT_NAME);
        assert_eq!(
            d.template(),
            format!(r"{DEFAULT_DIR}\{DEFAULT_NAME}"),
            "完整模板应是目录 + 文件名"
        );
    }

    /// 目录末尾的分隔符不会拼出双斜杠；空串视为未设置。
    #[test]
    fn template_joins_dir_and_name() {
        let cfg = QuickSaveConfig {
            dir: Some(r"D:\shots\".into()),
            name: Some("shot_$ts$".into()),
            ..Default::default()
        };
        assert_eq!(cfg.template(), r"D:\shots\shot_$ts$");
        let cfg = QuickSaveConfig {
            dir: Some("   ".into()),
            name: Some("".into()),
            ..Default::default()
        };
        assert_eq!(cfg.dir_or_default(), DEFAULT_DIR, "空目录回退默认");
        assert_eq!(cfg.name_or_default(), DEFAULT_NAME, "空文件名回退默认");
    }

    /// 遗留的整条路径模板会拆成两段；用户已分别配置过则不动。
    #[test]
    fn legacy_template_migrates_into_dir_and_name() {
        let mut cfg = QuickSaveConfig {
            legacy_template: Some(r"%USERPROFILE%\Pictures\airsnip\$ts$".into()),
            ..Default::default()
        };
        cfg.migrate();
        assert_eq!(cfg.dir.as_deref(), Some(r"%USERPROFILE%\Pictures\airsnip"));
        assert_eq!(cfg.name.as_deref(), Some("$ts$"));
        assert!(cfg.legacy_template.is_none(), "迁移后应清空遗留字段");

        // 只有文件名、没有目录
        let mut cfg = QuickSaveConfig {
            legacy_template: Some("shot".into()),
            ..Default::default()
        };
        cfg.migrate();
        assert_eq!(cfg.dir, None);
        assert_eq!(cfg.name.as_deref(), Some("shot"));

        // 已分别配置：迁移不覆盖
        let mut cfg = QuickSaveConfig {
            dir: Some(r"D:\keep".into()),
            name: Some("keep".into()),
            legacy_template: Some(r"E:\old\old".into()),
            ..Default::default()
        };
        cfg.migrate();
        assert_eq!(cfg.dir.as_deref(), Some(r"D:\keep"));
        assert_eq!(cfg.name.as_deref(), Some("keep"));
    }

    /// 配置往返：两段分别落盘（稳定短值），遗留字段不再写入。
    #[test]
    fn roundtrip_and_legacy_defaults() {
        let cfg = crate::settings::Config {
            quick_save: QuickSaveConfig {
                dir: Some(r"D:\shots".into()),
                name: Some("shot_$ts$".into()),
                format: SaveFormat::Webp,
                on_exist: ExistingFile::Overwrite,
                notify: false,
                also_copy: true,
                legacy_template: None,
            },
            ..Default::default()
        };
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("on_exist = \"overwrite\""), "{text}");
        assert!(text.contains("format = \"webp\""), "{text}");
        assert!(!text.contains("template"), "遗留字段不应写入：{text}");
        let back: crate::settings::Config = toml::from_str(&text).unwrap();
        assert_eq!(back.quick_save.dir.as_deref(), Some(r"D:\shots"));
        assert_eq!(back.quick_save.name.as_deref(), Some("shot_$ts$"));
        assert_eq!(back.quick_save.format, SaveFormat::Webp);
        assert_eq!(back.quick_save.on_exist, ExistingFile::Overwrite);
        assert!(!back.quick_save.notify);
        assert!(back.quick_save.also_copy);

        // 旧配置（0.12 及以前）没有 [quick_save] 段
        let old: crate::settings::Config = toml::from_str("[hotkey]\nscreenshot_key = 0x70\n").unwrap();
        assert_eq!(old.quick_save.dir, None);
        assert_eq!(old.quick_save.dir_or_default(), DEFAULT_DIR);
    }

    /// 格式与同名策略的标签齐全（下拉按 `ALL` 渲染，标签为空会画出空格子）。
    #[test]
    fn labels_are_non_empty() {
        for f in SaveFormat::ALL {
            assert!(!f.title().is_empty() && !f.ext().is_empty());
            assert!(f.title().ends_with(f.ext()), "{:?} 的下拉文字应带扩展名", f);
        }
        for e in ExistingFile::ALL {
            assert!(!e.title().is_empty());
        }
    }
}
