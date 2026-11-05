// SPDX-License-Identifier: GPL-3.0-only
//! 取字（OCR-1～OCR-13）：本地 `Windows.Media.Ocr` 识别，图片不出本机（ADR 0022）。
//!
//! 分工：`local` 是 WinRT 实现（COM 初始化、SoftwareBitmap 转换、引擎创建与等待），
//! `lines` 把引擎返回的行/词结构拼成文本（CJK 空格规则）。
//!
//! 调用方负责把 [`recognize`] 放到后台线程（OCR-12）：本模块内部同步等待识别结果，
//! 但会自行 `RoInitialize` / `RoUninitialize`，不要求调用线程预先初始化 COM。
//! 引擎按需创建、用完释放，不在常驻路径上持有 WinRT 对象（OCR-13）。

mod lines;
mod local;

use tiny_skia::Pixmap;

pub use lines::{Segment, join_segments};

/// 识别结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Recognized {
    /// 合并后的文本（行结构保留，行内 CJK 之间不补空格，OCR-11）。
    pub text: String,
    /// 实际使用的识别语言标签（如 `zh-Hans-CN`）。
    pub language: String,
}

/// 取字失败原因（OCR-8：由气泡就地展示，不弹系统对话框）。
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// 系统没有安装任何 OCR 语言包。
    NoLanguagePack,
    /// 指定语言未安装：`available` 是系统已安装的语言标签。
    LanguageNotInstalled { wanted: String, available: Vec<String> },
    /// 选区过大，超出引擎单次处理上限。
    TooLarge { max: u32 },
    /// WinRT 初始化或调用失败。
    Failed(String),
}

impl Error {
    /// 气泡内展示的一句话（含下一步动作）。
    pub fn message(&self) -> String {
        match self {
            Error::NoLanguagePack => {
                "系统未安装文字识别语言包。\n请到「设置 → 时间和语言 → 语言和区域」为所需语言添加可选功能「光学字符识别」。"
                    .to_string()
            }
            Error::LanguageNotInstalled { wanted, available } if available.is_empty() => format!(
                "未安装 {wanted} 的文字识别语言包。\n请到「设置 → 时间和语言 → 语言和区域」为该语言添加可选功能「光学字符识别」。"
            ),
            Error::LanguageNotInstalled { wanted, available } => format!(
                "未安装 {wanted} 的文字识别语言包。\n系统已安装：{}。",
                available.join("、")
            ),
            Error::TooLarge { max } => {
                format!("选区过大：单次识别的最长边不能超过 {max} 像素，请缩小选区。")
            }
            Error::Failed(e) => format!("识别失败：{e}"),
        }
    }
}

/// 识别整张图（同步阻塞；调用方放后台线程，OCR-12）。///
/// `language` 为 `None` 时跟随系统首选语言（OCR-6），否则按 BCP-47 标签（如 `zh-Hans-CN`）创建引擎。
pub fn recognize(pixmap: &Pixmap, language: Option<&str>) -> Result<Recognized, Error> {
    local::recognize(pixmap, language)
}

/// 系统已安装的 OCR 语言标签（设置面板列选项用，OCR-6）。
pub fn available_languages() -> Result<Vec<String>, Error> {
    local::available_languages()
}

/// 语言标签 → 中文名（气泡上的「识别为 X」用）。
/// 只映射中文 / 英文 / 日文——目前也就这三种场景；其余标签原样返回，不编造名字。
pub fn language_name(tag: &str) -> String {
    let t = tag.to_ascii_lowercase();
    match t.split(['-', '_']).next().unwrap_or("") {
        // 中文需区分简繁：引擎返回 zh-Hans-CN / zh-Hant-TW 这类标签
        "zh" => {
            if t.contains("hant") || t.contains("-tw") || t.contains("-hk") || t.contains("-mo") {
                "繁体中文".to_string()
            } else {
                "中文".to_string()
            }
        }
        "en" => "英语".to_string(),
        "ja" => "日语".to_string(),
        _ => tag.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_language_tags_to_chinese_names() {
        assert_eq!(language_name("zh-Hans-CN"), "中文");
        assert_eq!(language_name("zh-Hant-TW"), "繁体中文");
        assert_eq!(language_name("en-US"), "英语");
        assert_eq!(language_name("ja"), "日语");
        // 未映射的标签原样返回（不编造名字）
        assert_eq!(language_name("ko-KR"), "ko-KR");
        assert_eq!(language_name(""), "");
    }

    /// 错误说明里要带下一步动作（OCR-6/OCR-8），不能只说“失败”。
    #[test]
    fn error_messages_are_actionable() {
        assert!(Error::NoLanguagePack.message().contains("光学字符识别"));
        assert!(
            Error::LanguageNotInstalled {
                wanted: "ja".into(),
                available: vec!["zh-Hans-CN".into()]
            }
            .message()
            .contains("zh-Hans-CN")
        );
        assert!(Error::TooLarge { max: 10000 }.message().contains("10000"));
    }
}
