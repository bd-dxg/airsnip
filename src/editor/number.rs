// SPDX-License-Identifier: GPL-3.0-only
//! 序号标记几何（EDT-12）：徽标圆 + 引线箭头的包围盒、引线起点与编号分配。
//! 从 `editor::mod` 拆出，保证单文件不超过 300 行。

use super::{Bounds, Kind, Object, Point, Style, arrow_head_len};

/// 徽标圆与引线箭头的并集包围盒（引线起点在徽标边缘，箭头头部外扩随线宽）。
pub(super) fn bounds(center: Point, tip: Point, radius: f32, style: &Style) -> Bounds {
    let half = style.width * 0.5 + 1.0;
    let badge = Bounds::around(
        center.x - radius,
        center.y - radius,
        center.x + radius,
        center.y + radius,
        half,
    );
    let start = number_arrow_start(center, tip, radius);
    let (dx, dy) = (tip.x - start.x, tip.y - start.y);
    if dx * dx + dy * dy < 1.0 {
        return badge; // 只有徽标（未拖动引线）
    }
    let pad = half.max(arrow_head_len(style.width) * 0.5);
    let arrow = Bounds::around(
        start.x.min(tip.x),
        start.y.min(tip.y),
        start.x.max(tip.x),
        start.y.max(tip.y),
        pad,
    );
    Bounds {
        x0: badge.x0.min(arrow.x0),
        y0: badge.y0.min(arrow.y0),
        x1: badge.x1.max(arrow.x1),
        y1: badge.y1.max(arrow.y1),
    }
}

/// 引线起点（EDT-12）：徽标圆边缘上朝 `tip` 方向的点，避免引线盖住数字；
/// `tip` 与圆心重合（只点一下没拖）时返回圆心。
pub fn number_arrow_start(center: Point, tip: Point, radius: f32) -> Point {
    let (dx, dy) = (tip.x - center.x, tip.y - center.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return center;
    }
    Point::new(center.x + dx / len * radius, center.y + dy / len * radius)
}

/// 徽标内容（EDT-12 / EDT-20）：编号、对勾、叉号。
///
/// 对勾 / 叉号**不占用编号**（`next_number` 只看 `Num`），所以在一张图里混着放也
/// 不会让 1、2、3 跳号。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    /// 自动编号。
    Num(u32),
    /// 对勾：批注「这里正确 / 通过」。
    Check,
    /// 叉号：批注「这里有问题 / 不通过」。
    Cross,
}

/// 徽标形态（EDT-20）：工具栏下拉与配置记忆用，不带具体编号。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BadgeMark {
    /// 自动编号（默认）。
    #[default]
    Num,
    Check,
    Cross,
}

impl BadgeMark {
    /// 下拉里的顺序。
    pub const ALL: [BadgeMark; 3] = [BadgeMark::Num, BadgeMark::Check, BadgeMark::Cross];

    /// 下拉项里的示意徽标（编号固定显示 1，只是个缩略图）。
    pub fn sample(self) -> Badge {
        match self {
            BadgeMark::Num => Badge::Num(1),
            BadgeMark::Check => Badge::Check,
            BadgeMark::Cross => Badge::Cross,
        }
    }
}

/// 覆盖层线程改过的徽标形态：主线程在截图结束时取出写回配置（同 `linestyle::line_store`）。
static BADGE: std::sync::Mutex<Option<BadgeMark>> = std::sync::Mutex::new(None);

/// 记录本次改过的形态（覆盖层线程调用）。
pub fn badge_store(m: BadgeMark) {
    if let Ok(mut g) = BADGE.lock() {
        *g = Some(m);
    }
}

/// 取出并清空记录的形态（主线程写回配置用）。
pub fn badge_take() -> Option<BadgeMark> {
    BADGE.lock().ok().and_then(|mut g| g.take())
}

impl Badge {
    /// 形态（下拉高亮当前项、选中对象改形态时比对用）。
    pub fn mark(self) -> BadgeMark {
        match self {
            Badge::Num(_) => BadgeMark::Num,
            Badge::Check => BadgeMark::Check,
            Badge::Cross => BadgeMark::Cross,
        }
    }

    /// 按形态生成新徽标内容：编号形态取文档内最大值 +1。
    pub fn create(mark: BadgeMark, objects: &[Object]) -> Self {
        match mark {
            BadgeMark::Num => Badge::Num(next_number(objects)),
            BadgeMark::Check => Badge::Check,
            BadgeMark::Cross => Badge::Cross,
        }
    }

    /// 双击符号徽标时的下一个内容（EDT-20）：✓ ⇄ ×。
    /// 编号不参与切换（返回 `None`）——编号是任意值，双击走编号编辑。
    pub fn toggled(self) -> Option<Self> {
        match self {
            Badge::Check => Some(Badge::Cross),
            Badge::Cross => Some(Badge::Check),
            Badge::Num(_) => None,
        }
    }

    /// 二次编辑时输入框里显示的文本：编号给数字，符号给字符（用户按键即可替换）。
    pub fn edit_text(self) -> String {
        match self {
            Badge::Num(n) => n.to_string(),
            Badge::Check => "✓".to_string(),
            Badge::Cross => "×".to_string(),
        }
    }

    /// 编辑框可接受的字符（EDT-20）：数字（编号）、`v` / `x` 与勾叉字符。
    /// 输入层过滤与 `parse_edit` 的映射**共用这份定义**，否则会出现「打得进去但解析不了」。
    pub fn accepts_char(c: char) -> bool {
        c.is_ascii_digit() || matches!(c, 'v' | 'V' | 'x' | 'X' | '√' | '✓' | '×' | '✕')
    }

    /// 一次编辑提交的文本 → 新徽标内容。
    ///
    /// 映射：`v` / `√` / `✓` = 对勾，`x` / `×` / `✕` = 叉号，数字 = 编号（≥1）；
    /// 其余字符忽略。**以最后出现的那个为准**：输入框里已有原内容（如 `✓`），
    /// 用户直接补一个 `x` 时应当变成叉号，而不是被前面的旧内容抢走。
    /// 返回 `None` 表示「没输入有效内容」。
    pub fn parse_edit(chars: &[char]) -> Option<Self> {
        let mut last_symbol: Option<(usize, Badge)> = None;
        let mut last_digit: Option<usize> = None;
        for (i, c) in chars.iter().enumerate() {
            match c {
                'v' | 'V' | '√' | '✓' => last_symbol = Some((i, Badge::Check)),
                'x' | 'X' | '×' | '✕' => last_symbol = Some((i, Badge::Cross)),
                c if c.is_ascii_digit() => last_digit = Some(i),
                _ => {}
            }
        }
        match (last_symbol, last_digit) {
            // 数字比符号晚出现：当作编号（用户想改号）
            (Some((si, _)), Some(di)) if di > si => Self::parse_digits(chars),
            (Some((_, sym)), _) => Some(sym),
            (None, Some(_)) => Self::parse_digits(chars),
            (None, None) => None,
        }
    }

    /// 数字字符拼成的编号（最多 4 位，零抬到 1）。
    fn parse_digits(chars: &[char]) -> Option<Self> {
        let digits: String = chars.iter().filter(|c| c.is_ascii_digit()).take(4).collect();
        digits.parse::<u32>().ok().map(|n| Badge::Num(n.max(1)))
    }
}

/// 下一个编号（EDT-12）：取已有编号里的最大值 +1，删除 / 撤销中间的不重排。
/// 对勾 / 叉号不参与，因此混用时编号仍连续。
pub fn next_number(objects: &[Object]) -> u32 {
    objects
        .iter()
        .filter_map(|o| match &o.kind {
            Kind::Number {
                mark: Badge::Num(n), ..
            } => Some(*n),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn badge(mark: Badge) -> Object {
        Object {
            kind: Kind::Number {
                center: Point::new(10.0, 10.0),
                tip: Point::new(10.0, 10.0),
                mark,
                font_size: 20.0,
                radius: 14.0,
            },
            style: Style::new([255, 0, 0, 255], 3.0),
        }
    }

    /// EDT-20：对勾 / 叉号不参与编号，混着放也不会让 1、2、3 跳号。
    #[test]
    fn symbols_do_not_consume_numbers() {
        let objs = vec![
            badge(Badge::Num(1)),
            badge(Badge::Check),
            badge(Badge::Cross),
            badge(Badge::Num(2)),
        ];
        assert_eq!(next_number(&objs), 3);
        // 只有符号时仍从 1 开始
        assert_eq!(next_number(&[badge(Badge::Check)]), 1);
        assert_eq!(next_number(&[]), 1);
    }

    /// EDT-20：一次编辑提交的三路映射（字母不用输法）。
    #[test]
    fn parse_edit_maps_three_ways() {
        let chars = |s: &str| s.chars().collect::<Vec<_>>();
        assert_eq!(Badge::parse_edit(&chars("v")), Some(Badge::Check));
        assert_eq!(Badge::parse_edit(&chars("V")), Some(Badge::Check));
        assert_eq!(Badge::parse_edit(&chars("√")), Some(Badge::Check));
        assert_eq!(Badge::parse_edit(&chars("x")), Some(Badge::Cross));
        assert_eq!(Badge::parse_edit(&chars("✕")), Some(Badge::Cross));
        assert_eq!(Badge::parse_edit(&chars("12")), Some(Badge::Num(12)));
        assert_eq!(Badge::parse_edit(&chars("0")), Some(Badge::Num(1)), "零也要抬到 1");
        assert_eq!(Badge::parse_edit(&chars("")), None, "空输入不改");
        assert_eq!(Badge::parse_edit(&chars("?!")), None, "无效字符不改");
        // 输入框里已有原内容时，后打的字符说了算（否则直接补一个 x 会被前面的 ✓ 抢走）
        assert_eq!(Badge::parse_edit(&chars("✓x")), Some(Badge::Cross));
        assert_eq!(Badge::parse_edit(&chars("×v")), Some(Badge::Check));
        assert_eq!(
            Badge::parse_edit(&chars("x1")),
            Some(Badge::Num(1)),
            "数字在后按编号处理"
        );
    }

    /// 形态与内容的互相映射：下拉高亮与选中对象改形态都靠它。
    #[test]
    fn mark_round_trips() {
        assert_eq!(Badge::Num(7).mark(), BadgeMark::Num);
        assert_eq!(Badge::Check.mark(), BadgeMark::Check);
        assert_eq!(BadgeMark::ALL.map(|m| m.sample().mark()), BadgeMark::ALL);
        assert_eq!(BadgeMark::default(), BadgeMark::Num, "默认仍是自动编号");
        // 编辑框初值：数字给数字，符号给可辨认的字符
        assert_eq!(Badge::Num(3).edit_text(), "3");
        assert_eq!(Badge::Check.edit_text(), "✓");
        assert_eq!(Badge::Cross.edit_text(), "×");
    }

    /// `accepts_char` 必须涵盖 `parse_edit` 认识的全部字符，否则输入层会提前拦掉它们。
    #[test]
    fn accepts_char_covers_parse_edit() {
        for s in ["v", "V", "x", "X", "√", "✓", "×", "✕", "7"] {
            let c = s.chars().next().expect("单字符");
            assert!(Badge::accepts_char(c), "应接受 {s}");
            assert!(Badge::parse_edit(&[c]).is_some(), "{s} 应能解析");
        }
        for s in ["a", "中", ".", "\\n"] {
            let c = s.chars().next().expect("单字符");
            assert!(!Badge::accepts_char(c), "不应接受 {s}");
            assert!(Badge::parse_edit(&[c]).is_none(), "{s} 不应被当成有效内容");
        }
    }

    /// 双击切换只对符号有效：✓ ⇄ ×，编号不参与（它走编号编辑）。
    #[test]
    fn toggled_swaps_symbols_only() {
        assert_eq!(Badge::Check.toggled(), Some(Badge::Cross));
        assert_eq!(Badge::Cross.toggled(), Some(Badge::Check));
        assert_eq!(Badge::Num(3).toggled(), None, "编号双击进编辑，不切换");
        assert_eq!(Badge::Check.toggled().and_then(Badge::toggled), Some(Badge::Check));
    }

    /// 跨截图记忆：写入后能取出，取出即清空（同 `line_store`）。
    #[test]
    fn badge_memory_roundtrip() {
        assert_eq!(badge_take(), None, "初始应为空");
        badge_store(BadgeMark::Cross);
        assert_eq!(badge_take(), Some(BadgeMark::Cross));
        assert_eq!(badge_take(), None, "取出后应清空");
    }
}
