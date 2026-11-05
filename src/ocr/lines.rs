// SPDX-License-Identifier: GPL-3.0-only
//! 多行 OCR 结果的文本拼接（OCR-11）。
//!
//! 两个坑都是 WinRT `Windows.Media.Ocr` 的实测行为：
//!
//! 1. `OcrLine::Text()` 会在每个词之间插空格，中文被逐字分词后就变成「你 好 世 界」。
//!    因此不用 `Text()`，而是自己按词拼：行内相邻词只有**两侧都不是 CJK** 时才补一个空格。
//! 2. **`OcrResult::Lines()` 的顺序不是阅读顺序**（实测按内部聚类乱序：同一视觉行的左段、
//!    另一行的左段、首行的中段、尾段……交错返回）。而且同一视觉行会因为字重/样式变化被切成
//!    多段。所以必须用每段的 `BoundingRect` 自己排：先按垂直中心归并成视觉行，
//!    再行内按左边界排序拼成一行。

use std::cmp::Ordering;

/// 一段识别结果及其在图像中的包围盒（像素）。
///
/// 一段 = 引擎返回的一个 `OcrLine`，未必等于一个视觉行（长行会被切成多段）。
pub struct Segment {
    pub left: f32,
    pub top: f32,
    pub bottom: f32,
    pub words: Vec<String>,
}

impl Segment {
    fn center_y(&self) -> f32 {
        (self.top + self.bottom) / 2.0
    }

    fn height(&self) -> f32 {
        (self.bottom - self.top).max(1.0)
    }
}

/// 把片段按视觉阅读顺序（从上到下、行内从左到右）拼成文本。
pub fn join_segments(mut segs: Vec<Segment>) -> String {
    // 1. 按垂直中心归并成视觉行（阈值取半个行高：行距通常 ≥ 1.2 倍字号，不会误合并）
    segs.sort_by(|a, b| a.center_y().partial_cmp(&b.center_y()).unwrap_or(Ordering::Equal));
    let mut rows: Vec<Vec<Segment>> = Vec::new();
    for seg in segs {
        let same_row = rows.last().is_some_and(|row| {
            let head = &row[0];
            (seg.center_y() - head.center_y()).abs() <= head.height().max(seg.height()) * 0.5
        });
        if same_row {
            if let Some(row) = rows.last_mut() {
                row.push(seg);
            }
        } else {
            rows.push(vec![seg]);
        }
    }

    // 2. 行内按左边界排序（同一视觉行的多段本来就是从左到右的）
    let mut out = String::new();
    for mut row in rows {
        row.sort_by(|a, b| a.left.partial_cmp(&b.left).unwrap_or(Ordering::Equal));
        let words: Vec<&str> = row
            .iter()
            .flat_map(|s| s.words.iter().map(|w| w.as_str()))
            .map(|w| w.trim())
            .filter(|w| !w.is_empty())
            .collect();
        if words.is_empty() {
            continue; // 空行不产出多余换行
        }
        if !out.is_empty() {
            out.push('\n');
        }
        for (i, w) in words.iter().enumerate() {
            if i > 0 && need_space(words[i - 1], w) {
                out.push(' ');
            }
            out.push_str(w);
        }
    }
    strip_cjk_spaces(&out)
}

/// 兜底：去掉 CJK 字符之间的空格。
///
/// 引擎偶尔把汉字拆成两个词（如「讠 兑」），某些语言包还会在词内塞空格；
/// 这里统一把「两侧都是 CJK」的空格吃掉，保证中文不会看起来被拆开。
fn strip_cjk_spaces(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == ' ' {
            let prev = i.checked_sub(1).and_then(|j| chars.get(j)).copied();
            let next = chars.get(i + 1).copied();
            if let (Some(a), Some(b)) = (prev, next)
                && is_cjk(a)
                && is_cjk(b)
            {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// 词间是否需要空格：任一侧是 CJK 就不补（中文之间、中英之间都不加），否则补一个。
fn need_space(prev: &str, next: &str) -> bool {
    match (prev.chars().last(), next.chars().next()) {
        (Some(a), Some(b)) => !(is_cjk(a) || is_cjk(b)),
        _ => false,
    }
}

/// CJK 字符（汉字、假名、CJK 标点、全角字符）。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3000..=0x303F   // CJK 标点（。、「」等）
        | 0x3040..=0x30FF // 平假名 / 片假名
        | 0x3400..=0x4DBF // 汉字扩展 A
        | 0x4E00..=0x9FFF // 基本汉字
        | 0xF900..=0xFAFF // 兼容汉字
        | 0xFF00..=0xFFEF // 全角字符
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一行（单个片段）：`top` 递增代表不同视觉行。
    fn seg(top: f32, words: &[&str]) -> Segment {
        Segment {
            left: 0.0,
            top,
            bottom: top + 16.0,
            words: words.iter().map(|w| w.to_string()).collect(),
        }
    }

    /// 同一视觉行里的一个片段（指定左边界）。
    fn at(left: f32, top: f32, words: &[&str]) -> Segment {
        Segment {
            left,
            ..seg(top, words)
        }
    }

    /// 中文逐字分词后不能变成「你 好 世 界」。
    #[test]
    fn chinese_words_join_without_spaces() {
        assert_eq!(join_segments(vec![seg(0.0, &["你", "好", "世界"])]), "你好世界");
    }

    /// 英文单词之间要补空格，否则会拼成 HelloWorld。
    #[test]
    fn english_words_join_with_single_space() {
        assert_eq!(join_segments(vec![seg(0.0, &["Hello", "World"])]), "Hello World");
    }

    /// 中英混排：中文侧不补空格，英文之间照常补。
    #[test]
    fn mixed_scripts_only_space_between_latin() {
        assert_eq!(
            join_segments(vec![seg(0.0, &["使用", "Windows", "11", "系统"])]),
            "使用Windows 11系统"
        );
    }

    /// 多行保留换行；空行被跳过（不产生连续换行）。
    #[test]
    fn rows_keep_newlines_and_skip_empty() {
        let text = join_segments(vec![
            seg(0.0, &["第一行"]),
            seg(20.0, &[]),
            seg(40.0, &["second", "line"]),
        ]);
        assert_eq!(text, "第一行\nsecond line");
    }

    /// **实测 bug 的回归守卫**：`Lines()` 返回的顺序不是阅读顺序，
    /// 必须按 bbox 重排（用户截图：先出第1行左段、第2行左段、第1行中段……）。
    #[test]
    fn out_of_order_segments_are_sorted_by_position() {
        let segs = vec![
            at(26.0, 58.0, &["第一行"]),
            at(25.0, 100.0, &["第二行"]),
            at(236.0, 58.0, &["中段"]),
            at(1647.0, 58.0, &["尾段"]),
        ];
        assert_eq!(join_segments(segs), "第一行中段尾段\n第二行");
    }

    /// 同一视觉行被切成多段时，行内按左边界排序，且合回**一行**（不是三段）。
    #[test]
    fn segments_on_same_row_merge_into_one_line() {
        let segs = vec![at(245.0, 163.0, &["work.top"]), at(46.0, 163.0, &["之前只夹了"])];
        let out = join_segments(segs);
        assert_eq!(out.lines().count(), 1, "同行应合并成一行：{out:?}");
        assert!(out.starts_with("之前只夹了"), "行内应按左边界排序：{out:?}");
    }

    /// 行距正常的相邻行不能被误归并（阈值是半个行高）。
    #[test]
    fn adjacent_rows_are_not_merged() {
        let out = join_segments(vec![seg(0.0, &["上行"]), seg(20.0, &["下行"])]);
        assert_eq!(out, "上行\n下行");
    }

    /// 日文假名与汉字之间不补空格。
    #[test]
    fn japanese_kana_joins_without_spaces() {
        assert_eq!(join_segments(vec![seg(0.0, &["こんにちは", "世界"])]), "こんにちは世界");
    }

    /// 词内多余空白被裁掉，避免拼接出双空格。
    #[test]
    fn trims_word_whitespace() {
        assert_eq!(join_segments(vec![seg(0.0, &["  Hello ", " World  "])]), "Hello World");
    }

    /// 引擎在词内塞了空格时（如「讠 兑」）也要吃掉：中文不能被拆开。
    #[test]
    fn strips_spaces_inside_cjk() {
        assert_eq!(join_segments(vec![seg(0.0, &["讠 兑", "你 好"])]), "讠兑你好");
        // 中英之间的空格保留（两侧不都是 CJK）
        assert_eq!(join_segments(vec![seg(0.0, &["使用 Windows"])]), "使用 Windows");
    }

    /// CJK 判定覆盖范围（汉字 / 假名 / 全角 / CJK 标点），不误伤拉丁与数字。
    #[test]
    fn cjk_ranges() {
        for c in ['中', 'あ', 'ア', '。', '，', 'Ａ'] {
            assert!(is_cjk(c), "{c} 应判为 CJK");
        }
        for c in ['A', 'z', '1', ',', '.'] {
            assert!(!is_cjk(c), "{c} 不应判为 CJK");
        }
    }
}
