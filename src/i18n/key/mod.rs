// SPDX-License-Identifier: GPL-3.0-only
//! 文案键（I18N-2，issue #151 / ADR 0047）：区域汇总与批量生成宏。
//!
//! 组织方式：**按区域拆子枚举**，每门语言对每个区域写一个完整 `match`（不写 `_ =>` 兜底）。
//! 这样漏一条文案是**编译错误**，不是「运行时英文界面里冒出一句中文」；同时每个语言的
//! 表文件保持在单文件 300 行以内。
//!
//! 与工具栏可见性键（`tool.rect` 等）**分属不同命名空间**：可见性键是落进配置的契约，
//! 文案键要能随措辞改。两者都叫 `tool.rect` 纯属巧合，不要合并。
//!
//! 插值只支持具名的 `{name}` 占位符（`i18n::t_with`），不引 ICU MessageFormat；
//! 语序差异就整句单独写一条。

/// 全部文案键（区域在此汇总；新增区域时加一个变体 + 一个 `match` 分支）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Settings(SettingsKey),
    Overlay(OverlayKey),
    App(AppKey),
}

impl Key {
    /// 全部键（供「每个键都有文案」单测遍历）。
    pub fn all() -> impl Iterator<Item = Key> {
        SettingsKey::ALL
            .iter()
            .copied()
            .map(Key::Settings)
            .chain(OverlayKey::ALL.iter().copied().map(Key::Overlay))
            .chain(AppKey::ALL.iter().copied().map(Key::App))
    }
}

/// 按键名批量生成枚举 + 全量清单的宏。
/// 只写一遍清单：漏进 `ALL` 与漏写变体都会立刻暴露（`ALL` 是遍历用的唯一来源）。
/// 第二个参数是 `Key` 里的包裹变体名（`key.text()` 用它拼回去）。
macro_rules! keys {
    ($name:ident, $variant:ident { $($(#[$meta:meta])* $key:ident),* $(,)? }) => {
        /// 界面文案键。
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $($(#[$meta])* $key),*
        }

        impl $name {
            /// 全部键，顺序即枚举顺序。
            pub const ALL: &'static [$name] = &[$($name::$key),*];

            /// 当前界面语言的文案（调用点最常见的写法：`K::NavGeneral.text()`）。
            pub fn text(self) -> &'static str {
                crate::i18n::t(crate::i18n::Key::$variant(self))
            }
        }
    };
}

mod app;
mod overlay;
mod settings;

pub use app::AppKey;
pub use overlay::OverlayKey;
pub use settings::SettingsKey;

#[cfg(test)]
mod tests {
    use super::*;

    /// 键清单不应有重复（宏展开后若有同名变体会先编译失败；这里守 `Key::all`
    /// 与各区域 `ALL` 的对应关系，新增区域忘接进 `Key::all` 会被这条拦下）。
    #[test]
    fn all_keys_are_listed_once() {
        assert_eq!(
            Key::all().count(),
            SettingsKey::ALL.len() + OverlayKey::ALL.len() + AppKey::ALL.len()
        );
        let mut seen = std::collections::HashSet::new();
        for k in SettingsKey::ALL {
            assert!(seen.insert(*k), "键重复：{k:?}");
        }
        let mut seen = std::collections::HashSet::new();
        for k in OverlayKey::ALL {
            assert!(seen.insert(*k), "键重复：{k:?}");
        }
        let mut seen = std::collections::HashSet::new();
        for k in AppKey::ALL {
            assert!(seen.insert(*k), "键重复：{k:?}");
        }
    }
}
