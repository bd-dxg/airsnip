// SPDX-License-Identifier: GPL-3.0-only
//! 系统字体族枚举（EDT-4）：DirectWrite 系统字体集 → 族名列表。
//! 一次枚举后缓存（`OnceLock`），覆盖层字体面板每次重绘都读取缓存，不重复访问系统 API。
//! 族名优先取中文名（zh-CN），其次英文名（en-US），最后取第一项；
//! 过滤 `@` 开头的竖排别名（不可选作横排正文）。

use std::collections::BTreeSet;
use std::sync::OnceLock;
use windows::Win32::Graphics::DirectWrite::{IDWriteFactory, IDWriteFontFamily};
use windows::core::{BOOL, w};

use super::text::{DEFAULT_FAMILY, factory};

/// 系统字体族名（已排序去重）。DirectWrite 不可用时只返回默认字体。
pub fn families() -> &'static [String] {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE.get_or_init(|| enumerate().unwrap_or_else(|| vec![DEFAULT_FAMILY.to_string()]))
}

/// 枚举系统字体集（失败返回 None，由调用方回退到默认字体）。
fn enumerate() -> Option<Vec<String>> {
    let f: &IDWriteFactory = factory()?;
    let mut collection = None;
    unsafe { f.GetSystemFontCollection(&mut collection, false) }.ok()?;
    let collection = collection?;
    let count = unsafe { collection.GetFontFamilyCount() };
    let mut names = BTreeSet::new();
    for i in 0..count {
        let Ok(family) = (unsafe { collection.GetFontFamily(i) }) else {
            continue;
        };
        if let Some(name) = family_name(&family) {
            names.insert(name);
        }
    }
    if names.is_empty() {
        return None;
    }
    Some(names.into_iter().collect())
}

/// 单个字体族的中文/英文名；为空或竖排别名（`@` 开头）时返回 None。
fn family_name(family: &IDWriteFontFamily) -> Option<String> {
    let names = unsafe { family.GetFamilyNames() }.ok()?;
    let mut index = 0u32;
    let mut exists = BOOL(0);
    for locale in [w!("zh-CN"), w!("en-US")] {
        if unsafe { names.FindLocaleName(locale, &mut index, &mut exists) }.is_ok() && exists.as_bool() {
            break;
        }
    }
    let len = unsafe { names.GetStringLength(index) }.ok()?;
    let mut buf = vec![0u16; len as usize + 1];
    unsafe { names.GetString(index, &mut buf) }.ok()?;
    let name = String::from_utf16_lossy(&buf[..len as usize]);
    let name = name.trim();
    if name.is_empty() || name.starts_with('@') {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_are_sorted_unique_and_nonempty() {
        let list = families();
        assert!(!list.is_empty(), "至少应有默认字体");
        assert!(
            list.iter().any(|n| n == DEFAULT_FAMILY),
            "系统应装有默认字体 {DEFAULT_FAMILY}：{list:?}"
        );
        assert!(list.windows(2).all(|w| w[0] < w[1]), "族名应严格升序且不重复");
        assert!(list.iter().all(|n| !n.starts_with('@')), "不应含竖排别名");
    }
}
#[test]
fn scratch_enum_timing() {
    let t = std::time::Instant::now();
    let n = crate::render::fonts::families().len();
    eprintln!("字体枚举 {} 项：{:?}", n, t.elapsed());
}
