// SPDX-License-Identifier: GPL-3.0-only
//! OCR 设置（OCR-1/OCR-6）：总开关、识别语言循环切换、说明行。
//!
//! 语言不弹下拉框：系统通常只装 1～3 个 OCR 语言包，点一行循环切换足够，
//! 且不需要为动态列表改造行模型（`Row::label` 是 `&'static str`）。

use crate::settings::{OCR_LANG_AUTO, OcrConfig};

use super::Panel;

/// 识别语言的显示文案。
pub(super) fn language_text(cfg: &OcrConfig) -> String {
    match cfg.language_tag() {
        None => "跟随系统".to_string(),
        Some(tag) => tag.to_string(),
    }
}

/// 循环切换：跟随系统 → 已安装语言 1 → … → 跟随系统。返回新的显示文案。
///
/// 首次切换才枚举语言（`OCR-13`：功能不用时不为 WinRT 付成本），结果缓存在面板里。
pub(super) fn cycle(panel: &mut Panel) -> Result<String, String> {
    if panel.ocr_langs.is_none() {
        let langs = crate::ocr::available_languages().map_err(|e| e.message())?;
        panel.ocr_langs = Some(langs);
    }
    let langs = panel.ocr_langs.clone().unwrap_or_default();
    if langs.is_empty() {
        return Err(
            "系统未安装文字识别语言包。\n请到「设置 → 时间和语言 → 语言和区域」为所需语言添加可选功能「光学字符识别」。"
                .to_string(),
        );
    }
    let next = match panel.config.ocr.language_tag() {
        None => langs[0].clone(),
        Some(tag) => match langs.iter().position(|l| l == tag) {
            Some(i) if i + 1 < langs.len() => langs[i + 1].clone(),
            _ => OCR_LANG_AUTO.to_string(),
        },
    };
    panel.config.ocr.language = next;
    Ok(language_text(&panel.config.ocr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_text_falls_back_to_follow_system() {
        let cfg = OcrConfig::default();
        assert_eq!(language_text(&cfg), "跟随系统");
        let cfg = OcrConfig {
            language: "zh-Hans-CN".into(),
            ..Default::default()
        };
        assert_eq!(language_text(&cfg), "zh-Hans-CN");
    }

    /// `auto` 视为跟随系统（大小写与空白都容错）。
    #[test]
    fn auto_is_treated_as_follow_system() {
        for raw in ["auto", " AUTO ", ""] {
            let cfg = OcrConfig {
                enabled: true,
                language: raw.to_string(),
                ..Default::default()
            };
            assert_eq!(cfg.language_tag(), None, "{raw:?} 应视为跟随系统");
        }
    }
}
