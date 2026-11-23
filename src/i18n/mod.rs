// SPDX-License-Identifier: GPL-3.0-only
//! 国际化（I18N-1/2，issue #151 / ADR 0047）：界面文案查表与语言解析。
//!
//! 设计要点（详见 ADR 0047）：
//! - 资源**编译期嵌入**（每门语言一个 `match`），零文件依赖、零分配，符合「解压即用」；
//! - 查表成本是一次跳转表跳转，不进 IO、不进定时器，待机开销红线不受影响；
//! - 漏一条文案是**编译错误**（`match` 无 `_` 兜底），不是「英文界面里冒一句中文」；
//! - 语言在进程内只解析一次，`set_current` 之后面板立即重绘即生效（无资源重载）。
//!
//! 加一门语言 = 新建一个 `lang` 资源文件 + `Lang` 加变体 + `ui_family` 加一行，
//! 代码里没有 `if lang` 分支（布局按量宽自适应，不看语言）。

mod en;
mod ja;
mod key;
mod lang;
mod zh_hans;
mod zh_hant;

pub use key::{Key, OverlayKey, SettingsKey};
pub use lang::{Lang, LangPref, current, set_current};

/// 当前界面语言的文案。
pub fn t(key: Key) -> &'static str {
    text(current(), key)
}

/// 指定语言的文案。测试与需要固定语言的调用点走这里（避免并行测试改全局语言）。
pub fn text(lang: Lang, key: Key) -> &'static str {
    match lang {
        Lang::ZhHans => match key {
            Key::Settings(k) => zh_hans::settings(k),
            Key::Overlay(k) => zh_hans::overlay(k),
        },
        Lang::ZhHant => match key {
            Key::Settings(k) => zh_hant::settings(k),
            Key::Overlay(k) => zh_hant::overlay(k),
        },
        Lang::En => match key {
            Key::Settings(k) => en::settings(k),
            Key::Overlay(k) => en::overlay(k),
        },
        Lang::Ja => match key {
            Key::Settings(k) => ja::settings(k),
            Key::Overlay(k) => ja::overlay(k),
        },
    }
}

/// 当前语言的文案 + 具名占位符替换（`{name}`）。
/// 找不到的占位符原样留在结果里 —— 宁可界面上露出 `{count}`，也不静默吞掉一个字。
pub fn t_with(key: Key, args: &[(&str, &str)]) -> String {
    fill(t(key), args)
}

/// 具名占位符替换（按 `{key}` 精确替换）。
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 四门语言对每个键都给出非空文案。空串会让界面出现「什么都没有」的死角，
    /// 而编译器只保证「写了分支」，不保证「写了内容」。
    #[test]
    fn every_key_has_text_in_every_language() {
        for lang in Lang::ALL {
            for key in Key::all() {
                let s = text(lang, key);
                assert!(!s.trim().is_empty(), "{lang:?} 缺文案：{key:?}");
            }
        }
    }

    /// 英文文案里不该出现中日韩文字（漏翻最典型的表现）。
    #[test]
    fn english_has_no_cjk() {
        for key in Key::all() {
            let s = text(Lang::En, key);
            if let Some(c) = s.chars().find(|c| is_cjk(*c)) {
                panic!("英文文案里出现 CJK 字符 {c:?}：{key:?} → {s}");
            }
        }
    }

    /// 繁中与简中必须真的有差异（不是同一张表复制两遍）：抽样若干条常见异形词。
    #[test]
    fn traditional_differs_from_simplified() {
        let all: Vec<Key> = Key::all().collect();
        let differ = all
            .iter()
            .filter(|k| text(Lang::ZhHans, **k) != text(Lang::ZhHant, **k))
            .count();
        assert!(
            differ >= all.len() / 2,
            "繁中与简中仅有 {differ}/{} 条不同，疑似直接复制了简体表",
            all.len()
        );
        // 抽查几条确定的异形词，防止「大面积相同但个别词错」
        assert_eq!(text(Lang::ZhHant, Key::Settings(SettingsKey::NavSave)), "儲存");
        assert_eq!(text(Lang::ZhHant, Key::Settings(SettingsKey::ToolFreehand)), "手繪");
        assert_eq!(text(Lang::ZhHant, Key::Overlay(OverlayKey::TipUndo)), "復原");
        assert_eq!(text(Lang::ZhHant, Key::Overlay(OverlayKey::PinMenuSave)), "儲存");
    }

    /// 日文表里必须有假名（否则是直接抄了中文）。
    #[test]
    fn japanese_actually_contains_kana() {
        let all: Vec<Key> = Key::all().collect();
        let kana = all
            .iter()
            .filter(|k| {
                text(Lang::Ja, **k)
                    .chars()
                    .any(|c| matches!(c, '\u{3040}'..='\u{30ff}'))
            })
            .count();
        assert!(
            kana >= all.len() / 2,
            "日文表只有 {kana}/{} 条含假名，疑似抄了中文",
            all.len()
        );
    }

    /// 占位符替换精确到 `{name}`，不做模糊匹配（避免 `{count}` 撞上 `{count}2`）。
    #[test]
    fn placeholders_are_replaced_exactly() {
        let s = fill("共 {count} 项 / {counters}", &[("count", "20")]);
        assert_eq!(s, "共 20 项 / {counters}");
    }

    /// 四门语言的「界面字体」不重复且都不为空（加语言时忘配字体在这里暴露）。
    #[test]
    fn ui_families_are_distinct() {
        let mut seen = Vec::new();
        for lang in Lang::ALL {
            let f = lang.ui_family();
            assert!(!seen.contains(&f), "字体族重复：{f}");
            seen.push(f);
        }
    }

    fn is_cjk(c: char) -> bool {
        matches!(c,
            '\u{3000}'..='\u{303f}'   // CJK 标点（含「」、。）
            | '\u{3040}'..='\u{30ff}' // 假名
            | '\u{3400}'..='\u{4dbf}' // 汉字扩展 A
            | '\u{4e00}'..='\u{9fff}' // 汉字
            | '\u{f900}'..='\u{faff}' // 兼容汉字
            | '\u{ac00}'..='\u{d7af}' // 谚文
        )
    }
}
