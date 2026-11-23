// SPDX-License-Identifier: GPL-3.0-only
//! 界面语言（I18N-1，issue #151 / ADR 0047）：配置偏好、系统语言解析、全局当前语言。
//!
//! 三条约定：
//! - **进程内只解析一次**，之后由 `set_lang` 更新全局值（面板内切换立即生效：查表是纯内存
//!   操作，没有「重载资源」这回事）。
//! - 语言名（简体中文 / 繁體中文 / English / 日本語）用各语言自称，**不随界面语言翻译**。
//! - 未支持的系统语言一律回落 English —— 非中文系统首次启动就是英文界面，不用先去设置。

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use windows::Win32::Globalization::GetUserDefaultUILanguage;

/// 实际生效的界面语言。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    /// 简体中文（源语言：文案逐字与引入 i18n 前一致）。
    #[default]
    ZhHans,
    /// 繁體中文。
    ZhHant,
    /// English。
    En,
    /// 日本語。
    Ja,
}

impl Lang {
    pub const ALL: [Lang; 4] = [Lang::ZhHans, Lang::ZhHant, Lang::En, Lang::Ja];

    /// 界面字体族。简体沿用历史默认（`Microsoft YaHei UI`），其余取系统自带的
    /// 该语言首选字体；缺字体时 DirectWrite 会走自己的 fallback 链，不会画出豆腐块。
    pub fn ui_family(self) -> &'static str {
        match self {
            Lang::ZhHans => "Microsoft YaHei UI",
            Lang::ZhHant => "Microsoft JhengHei UI",
            Lang::En => "Segoe UI",
            Lang::Ja => "Yu Gothic UI",
        }
    }

    /// 全局存储用的稳定序号（不依赖 `repr`，改枚举顺序不影响已有值）。
    fn code(self) -> u8 {
        match self {
            Lang::ZhHans => 0,
            Lang::ZhHant => 1,
            Lang::En => 2,
            Lang::Ja => 3,
        }
    }

    fn decode(v: u8) -> Lang {
        match v {
            1 => Lang::ZhHant,
            2 => Lang::En,
            3 => Lang::Ja,
            _ => Lang::ZhHans,
        }
    }
}

/// 当前界面语言（全局）。初值简体中文：**测试与未初始化路径都保持历史行为**，
/// 只有 `set_lang` 才改变它。
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// 当前界面语言。
pub fn current() -> Lang {
    Lang::decode(CURRENT.load(Ordering::Relaxed))
}

/// 设置当前界面语言（启动时一次；面板内切换时也用）。
pub fn set_current(lang: Lang) {
    CURRENT.store(lang.code(), Ordering::Relaxed);
}

/// 配置里的语言偏好（`[ui] language`）。
///
/// `Auto` = 跟随系统。**反序列化容错**：旧配置写的是 `zh-CN`，未来版本可能写入本版
/// 不认识的值；这两种都不能让 `toml::from_str` 失败 —— `Config::load` 失败会回退
/// 整份默认配置，用户的其它设置会一起丢。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LangPref {
    /// 跟随系统（默认）。
    #[default]
    Auto,
    ZhHans,
    ZhHant,
    En,
    Ja,
}

impl LangPref {
    /// 下拉选项顺序：跟随系统 + 四门语言。
    pub const ALL: [LangPref; 5] = [
        LangPref::Auto,
        LangPref::ZhHans,
        LangPref::ZhHant,
        LangPref::En,
        LangPref::Ja,
    ];

    /// 配置里落盘的稳定字符串（**改这个等于改配置格式**）。
    pub fn key(self) -> &'static str {
        match self {
            LangPref::Auto => "auto",
            LangPref::ZhHans => "zh-Hans",
            LangPref::ZhHant => "zh-Hant",
            LangPref::En => "en",
            LangPref::Ja => "ja",
        }
    }

    /// 显式指定时对应的语言；`Auto` 返回 None。
    pub fn explicit(self) -> Option<Lang> {
        Some(match self {
            LangPref::Auto => return None,
            LangPref::ZhHans => Lang::ZhHans,
            LangPref::ZhHant => Lang::ZhHant,
            LangPref::En => Lang::En,
            LangPref::Ja => Lang::Ja,
        })
    }

    /// 解析偏好 → 实际语言（`Auto` 走系统首选 UI 语言）。
    pub fn resolve(self) -> Lang {
        self.explicit().unwrap_or_else(system_lang)
    }

    /// 配置串 → 偏好（容错：无法识别的值回落到 `Auto`，`zh-CN` 等旧写法归简体）。
    pub fn from_config(raw: &str) -> LangPref {
        match raw {
            "zh-Hans" | "zh-CN" | "zh-SG" | "zh" => LangPref::ZhHans,
            "zh-Hant" | "zh-TW" | "zh-HK" | "zh-MO" => LangPref::ZhHant,
            "en" => LangPref::En,
            "ja" => LangPref::Ja,
            _ => LangPref::Auto,
        }
    }

    /// 语言自称（不随界面语言翻译）。
    pub fn native_name(self) -> &'static str {
        match self {
            LangPref::Auto => "",
            LangPref::ZhHans => "简体中文",
            LangPref::ZhHant => "繁體中文",
            LangPref::En => "English",
            LangPref::Ja => "日本語",
        }
    }
}

impl Serialize for LangPref {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for LangPref {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(LangPref::from_config(&String::deserialize(d)?))
    }
}

/// 系统首选 UI 语言 → 支持的语言。
fn system_lang() -> Lang {
    // `GetUserDefaultUILanguage` 无窗口依赖，可在消息循环之前调用。
    lang_of_langid(unsafe { GetUserDefaultUILanguage() })
}

/// LANGID → 语言。只看 primary language（低 10 位），中文再按区域分简繁。
fn lang_of_langid(langid: u16) -> Lang {
    match langid & 0x03FF {
        0x04 => match langid {
            // zh-TW / zh-HK / zh-MO 用繁体；zh-CN / zh-SG 及其余中文区域用简体
            0x0404 | 0x0C04 | 0x1404 => Lang::ZhHant,
            _ => Lang::ZhHans,
        },
        0x11 => Lang::Ja,
        // 英文与所有未支持语言（德 / 法 / 韩…）一律 English：
        // 「先能看懂」比「猜一个近似语言」重要。
        _ => Lang::En,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn langid_maps_to_lang() {
        assert_eq!(lang_of_langid(0x0804), Lang::ZhHans, "zh-CN");
        assert_eq!(lang_of_langid(0x1004), Lang::ZhHans, "zh-SG");
        assert_eq!(lang_of_langid(0x0404), Lang::ZhHant, "zh-TW");
        assert_eq!(lang_of_langid(0x0C04), Lang::ZhHant, "zh-HK");
        assert_eq!(lang_of_langid(0x1404), Lang::ZhHant, "zh-MO");
        assert_eq!(lang_of_langid(0x0411), Lang::Ja, "ja-JP");
        assert_eq!(lang_of_langid(0x0409), Lang::En, "en-US");
        assert_eq!(lang_of_langid(0x0407), Lang::En, "德语回落 English");
        assert_eq!(lang_of_langid(0x0412), Lang::En, "韩语回落 English");
    }

    #[test]
    fn config_roundtrip_keeps_pref() {
        for pref in LangPref::ALL {
            assert_eq!(
                LangPref::from_config(pref.key()),
                pref,
                "配置串往返不应失真：{}",
                pref.key()
            );
        }
    }

    #[test]
    fn legacy_and_unknown_config_values_are_tolerated() {
        // 旧配置的写法不能报错（报错会让 Config::load 整份回退默认）
        assert_eq!(LangPref::from_config("zh-CN"), LangPref::ZhHans);
        assert_eq!(LangPref::from_config("zh-TW"), LangPref::ZhHant);
        // 未来版本写入的语言 / 垃圾值回落「跟随系统」
        assert_eq!(LangPref::from_config("ko"), LangPref::Auto);
        assert_eq!(LangPref::from_config(""), LangPref::Auto);
    }

    #[test]
    fn toml_deserialize_never_fails_on_language() {
        #[derive(Deserialize)]
        struct Holder {
            language: LangPref,
        }
        for raw in ["auto", "zh-CN", "zh-Hant", "ja", "ko", "0"] {
            let text = format!("language = \"{raw}\"");
            let h: Holder = toml::from_str(&text).expect("不应解析失败");
            if raw == "0" {
                continue;
            }
            assert!(LangPref::ALL.contains(&h.language), "{raw} → {}", h.language.key());
        }
    }

    #[test]
    fn every_lang_has_its_own_ui_family() {
        let mut seen = Vec::new();
        for lang in Lang::ALL {
            let f = lang.ui_family();
            assert!(!f.is_empty());
            assert!(!seen.contains(&f), "字体族重复：{f}");
            seen.push(f);
        }
    }

    /// 全局存储的编解码往返（**不碰 `set_current`**：并行测试会读到被改动的全局语言，
    /// 让其它断言中文文案的用例偶发失败）。
    #[test]
    fn lang_code_roundtrips() {
        for lang in Lang::ALL {
            assert_eq!(Lang::decode(lang.code()), lang);
        }
        // 越界值不应 panic，一律回落简体（全局存储只可能被自己的 `code()` 写入）
        assert_eq!(Lang::decode(200), Lang::ZhHans);
    }
}
