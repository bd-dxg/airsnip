// SPDX-License-Identifier: GPL-3.0-only
//! DirectWrite 文本测量与光标查询（EDT-4），栅格化在 `text::draw`。
//! `measure` 得到文字尺寸（写入对象测量缓存），`draw` 把文本画进 tiny_skia 像素图。
//!
//! 量测与折行、光标都用同一个 `IDWriteTextLayout`（建立方式在 `create_layout`），
//! 所以「量出来的尺寸 / 光标位置 / 画出来的字形」三者天然一致。
//! 全程纯 CPU、无窗口依赖；DirectWrite 初始化失败时返回估算测量并静默跳过绘制（不崩溃）。
//!
//! **自动折行（issue #221）**：`*_wrapped` 一族多一个 `wrap: Option<f32>` 参数（可用宽度，
//! `None` = 不折行）。折行点、标点禁则、CJK 逐字全交给 DirectWrite；光标位置走
//! `HitTestTextPosition`（`caret_offset`），所以折行后的光标与字形天然对齐。

use std::sync::Arc;

use windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS;
use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;
use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{PCWSTR, w};

use crate::editor::Kind;

/// 默认字体族（与对象模型同源；Win10/11 自带中文字体）。
pub const DEFAULT_FAMILY: &str = crate::editor::DEFAULT_FONT_FAMILY;
/// 布局构造的高度上限与「不折行」时的宽度（单行文本用）。
const MAX_LAYOUT: f32 = 100_000.0;

/// 字体规格（EDT-4）：族名 + 字号 + 粗体/斜体。
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    pub family: Arc<str>,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

impl FontSpec {
    /// 界面文案字体：**按当前界面语言取族名**（I18N-3 / ADR 0047）。
    /// 简体中文仍是历史默认 `Microsoft YaHei UI`，所以中文用户看到的一切与今天完全一致；
    /// 日文 / 繁中若复用 YaHei 会得到「中文字形冒充日文汉字」。
    pub fn new(size: f32) -> Self {
        Self {
            family: Arc::from(crate::i18n::current().ui_family()),
            size,
            bold: false,
            italic: false,
        }
    }

    /// 标注内容字体（**不随界面语言变**）：线宽 / 字号 / 字形都属于导出结果，
    /// 与「标注不随界面缩放」同一原则。徽标数字、序号编辑框等没有族字段的对象用它。
    pub fn content(size: f32) -> Self {
        Self {
            family: Arc::from(DEFAULT_FAMILY),
            size,
            bold: false,
            italic: false,
        }
    }

    /// 由文本对象的字段构造。
    pub fn from_parts(family: &Arc<str>, size: f32, bold: bool, italic: bool) -> Self {
        Self {
            family: family.clone(),
            size,
            bold,
            italic,
        }
    }
}

/// 共享工厂缓存（DWrite SHARED 工厂线程安全，`IDWriteFactory` 实现了 Send+Sync）。
pub(crate) fn factory() -> Option<&'static IDWriteFactory> {
    static F: std::sync::OnceLock<Option<IDWriteFactory>> = std::sync::OnceLock::new();
    F.get_or_init(|| unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok() })
        .as_ref()
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 宽字符串（尾部补 NUL）。
/// **必须**用于传给 `CreateTextFormat` 的字体族名：DirectWrite 不接收长度参数，
/// 只按 NUL 结尾读取；缺 NUL 会读到缓冲区之后的堆内存，导致字体名解析到垃圾
/// （表现为「换了字体却渲染成默认字体」），空串还会直接访问越界崩溃。
fn wide_z(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// DirectWrite 不可用时的估算（供 bounds 使用，保证不崩溃）。
fn approx(text: &str, font_size: f32) -> (f32, f32) {
    let n = text.encode_utf16().count() as f32;
    ((n * font_size * 0.9 + font_size).max(1.0), font_size * 1.4)
}

/// 估算版折行量测：按「总宽 / 可用宽」估行数（只在 DirectWrite 不可用时用）。
fn approx_wrapped(text: &str, spec: &FontSpec, wrap: Option<f32>) -> (f32, f32) {
    let (w, h) = approx(text, spec.size);
    match wrap {
        Some(limit) if limit > 0.0 && w > limit => (w.min(limit).max(1.0), h * (w / limit).ceil().max(1.0)),
        _ => (w, h),
    }
}

/// 建 layout：`wrap = None` 时宽度给 `MAX_LAYOUT`，等价于「不折行」（`\n` 仍会成行）。
fn create_layout(f: &IDWriteFactory, text: &str, spec: &FontSpec, wrap: Option<f32>) -> Option<IDWriteTextLayout> {
    let format = create_format(f, spec)?;
    let width = wrap.map_or(MAX_LAYOUT, |w| w.max(1.0));
    unsafe { f.CreateTextLayout(&utf16(text), &format, width, MAX_LAYOUT) }.ok()
}

fn create_format(f: &IDWriteFactory, spec: &FontSpec) -> Option<IDWriteTextFormat> {
    let family = wide_z(&spec.family);
    let weight = if spec.bold {
        DWRITE_FONT_WEIGHT_BOLD
    } else {
        DWRITE_FONT_WEIGHT_NORMAL
    };
    let style = if spec.italic {
        DWRITE_FONT_STYLE_ITALIC
    } else {
        DWRITE_FONT_STYLE_NORMAL
    };
    unsafe {
        f.CreateTextFormat(
            PCWSTR(family.as_ptr()),
            None,
            weight,
            style,
            DWRITE_FONT_STRETCH_NORMAL,
            spec.size,
            w!("zh-CN"),
        )
    }
    .ok()
}

/// 测量文本尺寸（物理像素）；`wrap` 为可用宽度（`None` = 不折行）。
/// 失败时给估算值。
pub fn measure_wrapped(text: &str, spec: &FontSpec, wrap: Option<f32>) -> (f32, f32) {
    let Some(f) = factory() else {
        return approx_wrapped(text, spec, wrap);
    };
    let Some(layout) = create_layout(f, text, spec, wrap) else {
        return approx_wrapped(text, spec, wrap);
    };
    let mut m = DWRITE_TEXT_METRICS::default();
    if unsafe { layout.GetMetrics(&mut m) }.is_ok() {
        // 多行（`\n` 或折行）时 width 为布局宽度（最长行），height 为总行高
        return (m.width.max(1.0), m.height.max(1.0));
    }
    approx_wrapped(text, spec, wrap)
}

/// 测量文本尺寸（不折行）。
pub fn measure(text: &str, spec: &FontSpec) -> (f32, f32) {
    measure_wrapped(text, spec, None)
}

/// 光标相对文字左上角的偏移 `(dx, top)`：`char_index` 是 `char` 下标（内部换成 UTF-16 码元）。
///
/// 走 `HitTestTextPosition` 而不是自己量宽 —— 软换行（issue #221）没有 `\n` 可数，
/// 自己按字符累加宽度既不准（字距 / 合字 / 标点禁则）也贵。
pub fn caret_offset(text: &str, spec: &FontSpec, wrap: Option<f32>, char_index: usize) -> (f32, f32) {
    let Some(f) = factory() else {
        return approx_caret(text, spec, char_index);
    };
    let Some(layout) = create_layout(f, text, spec, wrap) else {
        return approx_caret(text, spec, char_index);
    };
    // `textPosition` 是 UTF-16 码元偏移，我们的下标是 `char`，需要前缀映射（代理对占两个码元）
    let units: usize = text.chars().take(char_index).map(char::len_utf16).sum();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let mut m = DWRITE_HIT_TEST_METRICS::default();
    if unsafe { layout.HitTestTextPosition(units as u32, false, &mut x, &mut y, &mut m) }.is_ok() {
        // `m.top` 是光标所在行的行顶（layout 坐标），比「行号 × 行高」更准（软换行后行高可能不同）
        return (x, m.top);
    }
    approx_caret(text, spec, char_index)
}

/// 估算版光标位置（只在 DirectWrite 不可用时用）：只认硬换行，软换行不参与。
fn approx_caret(text: &str, spec: &FontSpec, char_index: usize) -> (f32, f32) {
    let prefix: String = text.chars().take(char_index).collect();
    let line = prefix.rsplit('\n').next().unwrap_or("");
    let rows = prefix.matches('\n').count();
    let (w, h) = approx(line, spec.size);
    (if line.is_empty() { 0.0 } else { w }, rows as f32 * h)
}

/// 重测受字号影响的几何缓存（EDT-4/EDT-12：字号 / 字体 / 编号变化后调用）。
/// 文本对象重算测量尺寸；序号标记重算徽标圆半径；其余对象无操作。
pub fn remeasure(kind: &mut Kind) {
    match kind {
        Kind::Text {
            text,
            font_size,
            font_family,
            bold,
            italic,
            wrap,
            size,
            ..
        } => {
            *size = measure_wrapped(
                text,
                &FontSpec::from_parts(font_family, *font_size, *bold, *italic),
                *wrap,
            );
        }
        Kind::Number {
            mark,
            font_size,
            radius,
            ..
        } => {
            *radius = super::number_radius(*font_size, *mark);
        }
        _ => {}
    }
}

mod draw;
pub(crate) use draw::{draw, draw_wrapped};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Point;
    use windows::Win32::Graphics::DirectWrite::DWRITE_FACTORY_TYPE_SHARED;

    fn spec(size: f32) -> FontSpec {
        FontSpec::new(size)
    }

    #[test]
    fn measure_handles_chinese_and_latin() {
        let (w1, h1) = measure("挺好", &spec(24.0));
        let (w2, h2) = measure("ab", &spec(24.0));
        assert!(w1 > 0.0 && h1 > 0.0, "中文测量应非零：{w1}x{h1}");
        assert!(w2 > 0.0, "拉丁字符测量应非零");
        assert!(w1 >= w2, "两个全角字应不窄于两个半角字符");
        assert!((h1 - h2).abs() < 8.0, "同行字号高度应接近：{h1} vs {h2}");
    }

    #[test]
    fn larger_font_measures_larger() {
        let (w1, _) = measure("测试", &spec(16.0));
        let (w2, _) = measure("测试", &spec(48.0));
        assert!(w2 > w1 * 2.0, "字号 3 倍宽度应近似 3 倍：{w1} vs {w2}");
    }

    #[test]
    fn different_families_measure_differently() {
        let yahei = FontSpec::new(24.0);
        let serif = FontSpec {
            family: std::sync::Arc::from("SimSun"),
            ..yahei.clone()
        };
        let (w1, _) = measure("字体测量 Family", &yahei);
        let (w2, _) = measure("字体测量 Family", &serif);
        assert!(
            w1 > 0.0 && w2 > 0.0 && (w1 - w2).abs() > 0.5,
            "不同字体族测量应不同：{w1} vs {w2}"
        );
    }

    #[test]
    fn remeasure_updates_text_cache_with_font() {
        let mut kind = Kind::Text {
            pos: Point::new(0.0, 0.0),
            text: std::sync::Arc::from("测量"),
            font_size: 24.0,
            font_family: std::sync::Arc::from(DEFAULT_FAMILY),
            bold: false,
            italic: false,
            wrap: None,
            size: (0.0, 0.0),
        };
        remeasure(&mut kind);
        let Kind::Text { size, .. } = kind else {
            panic!("应仍是文本")
        };
        assert!(size.0 > 0.0 && size.1 > 0.0, "尺寸缓存应被写入：{size:?}");
    }

    /// issue #221：给了折行宽度，宽变窄、高变大（行数增加）。
    #[test]
    fn wrapped_measure_narrows_and_grows() {
        let s = spec(24.0);
        let text = "这是一段足够长的中文，用来验证自动折行不会一路向右延伸。";
        let (w1, h1) = measure_wrapped(text, &s, None);
        let limit = 240.0;
        let (w2, h2) = measure_wrapped(text, &s, Some(limit));
        assert!(w2 <= limit + 1.0, "折行后应不超可用宽：{w2} vs {limit}");
        assert!(w1 > w2, "不折行时应该更宽：{w1} vs {w2}");
        assert!(h2 > h1, "折行后应该更高（行数变多）：{h1} vs {h2}");
    }

    /// issue #221：宽度足够时不折行，与不传 wrap 完全等价（回归旧行为）。
    #[test]
    fn wide_enough_wrap_matches_unwrapped() {
        let s = spec(24.0);
        let text = "短文本";
        let a = measure_wrapped(text, &s, None);
        let b = measure_wrapped(text, &s, Some(1000.0));
        assert!(
            (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5,
            "宽度富余时不应变：{a:?} vs {b:?}"
        );
    }

    /// issue #221：光标跨过折行点时落到下一行（y 变大、x 回落）。
    #[test]
    fn caret_offset_wraps_to_next_line() {
        let s = spec(20.0);
        // 每个汉字 20px、可用宽 62px → 每行 3 个字
        let text = "一二三四五六七八";
        let wrap = Some(62.0);
        let (_, top_a) = caret_offset(text, &s, wrap, 1);
        let (_, top_b) = caret_offset(text, &s, wrap, 2);
        let (x_wrap, top_c) = caret_offset(text, &s, wrap, 3);
        let (x_line2, _) = caret_offset(text, &s, wrap, 4);
        assert_eq!(top_a, top_b, "同一行内 top 应相同");
        assert!(top_c > top_b, "第 4 个字应折到下一行：top {top_b} → {top_c}");
        assert_eq!(x_wrap, 0.0, "折行点应落在行首（x = 0）");
        assert_eq!(x_line2, 20.0, "第二行第二个位置应为一个字宽");
    }

    /// issue #221：折行后光标 x 不超过可用宽度（行内累加不会一路向右）。
    #[test]
    fn caret_offset_stays_within_wrap_width() {
        let s = spec(20.0);
        let text = "abcdefghijklmnopqrstuvwxyz";
        let wrap = Some(80.0);
        for i in 0..=text.chars().count() {
            let (x, _) = caret_offset(text, &s, wrap, i);
            assert!(x <= wrap.unwrap() + 1.0, "第 {i} 个字符的光标越出可用宽：{x}");
        }
    }

    /// issue #221：`remeasure` 按对象上的 `wrap` 量测（而不是忽略它）。
    #[test]
    fn remeasure_respects_wrap() {
        let make = |wrap: Option<f32>| Kind::Text {
            pos: Point::new(0.0, 0.0),
            text: std::sync::Arc::from("很长很长的一段文字需要自动折行才会变高"),
            font_size: 24.0,
            font_family: std::sync::Arc::from(DEFAULT_FAMILY),
            bold: false,
            italic: false,
            wrap,
            size: (0.0, 0.0),
        };
        let mut free = make(None);
        remeasure(&mut free);
        let mut narrow = make(Some(200.0));
        remeasure(&mut narrow);
        let (Kind::Text { size: a, .. }, Kind::Text { size: b, .. }) = (&free, &narrow) else {
            panic!("应仍是文本")
        };
        assert!(b.1 > a.1, "折行的文本应更高：{a:?} vs {b:?}");
        assert!(b.0 <= 201.0, "折行后宽度应受限：{b:?}");
    }

    /// 未知 / 空字体族名必须回退到默认字体且不得崩溃。
    /// 回归：`CreateTextFormat` 的族名只能按 NUL 结尾读取，缺 NUL 时会读到缓冲区外的堆内存——
    /// 表现为换字体后仍渲染成默认字体，空串还会直接访问越界崩溃。
    #[test]
    fn unknown_or_empty_family_falls_back_without_crashing() {
        let empty = FontSpec {
            family: std::sync::Arc::from(""),
            ..FontSpec::new(24.0)
        };
        let unknown = FontSpec {
            family: std::sync::Arc::from("不存在的字体族名 xyz"),
            ..FontSpec::new(24.0)
        };
        let a = measure("回退测试 Family", &empty);
        let b = measure("回退测试 Family", &unknown);
        assert!(a.0 > 0.0 && b.0 > 0.0);
        assert_eq!(a, b, "未知与空族名应落到同一个默认字体：{a:?} vs {b:?}");
    }

    #[test]
    fn direct_write_factory_available() {
        let f = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) };
        assert!(f.is_ok(), "桌面会话应能创建 DirectWrite 工厂");
    }
}
