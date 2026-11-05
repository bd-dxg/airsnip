// SPDX-License-Identifier: GPL-3.0-only
//! 识别语言（OCR-6）：引擎创建与已安装语言枚举。

use windows::Globalization::Language;
use windows::Media::Ocr::OcrEngine;
use windows::core::HSTRING;

use super::super::Error;

/// 系统已安装的 OCR 语言标签（调用前 COM 已就绪）。
pub(super) fn list() -> Result<Vec<String>, Error> {
    let langs =
        OcrEngine::AvailableRecognizerLanguages().map_err(|e| Error::Failed(format!("读取识别语言失败：{e}")))?;
    let n = langs.Size().map_err(|e| Error::Failed(e.to_string()))?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        if let Ok(lang) = langs.GetAt(i).and_then(|l| l.LanguageTag()) {
            out.push(lang.to_string_lossy());
        }
    }
    Ok(out)
}

/// 创建识别引擎：`None` 跟随系统首选语言，否则按 BCP-47 标签（如 `zh-Hans-CN`）。
///
/// 失败原因要区分开（OCR-6）：系统一个语言包都没有 → `NoLanguagePack`；
/// 指定的语言没装 → `LanguageNotInstalled`，并带上已安装清单供用户选择。
pub(super) fn create_engine(language: Option<&str>) -> Result<OcrEngine, Error> {
    match language {
        None => OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| Error::NoLanguagePack),
        Some(tag) => {
            let lang = Language::CreateLanguage(&HSTRING::from(tag))
                .map_err(|e| Error::Failed(format!("语言标签 {tag} 无效：{e}")))?;
            OcrEngine::TryCreateFromLanguage(&lang).map_err(|_| Error::LanguageNotInstalled {
                wanted: tag.to_string(),
                available: list().unwrap_or_default(),
            })
        }
    }
}
