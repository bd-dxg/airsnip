// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏样式的可用性判据（EDT-1/EDT-14/EDT-18）：填充与线型「对哪些对象有意义」。
//! 从 `annotate` 拆出，控制单文件规模；绘制置灰（`toolbar::draw` 读 `State`）与
//! 动作副作用（`apply_action`）共用同一判据，不会各自演化。

use crate::editor::{Kind, Object};

/// 填充开关是否可用（EDT-1/EDT-14）：当前工具能画可填充形状，
/// **或**选中对象本身就是矩形 / 椭圆（此时按钮仍需能改它的填充）。
///
/// 参数是 `Option<Tool>`：移动态（ADR 0036）没有待用工具，此时只看选中对象。
pub(in crate::overlay) fn fill_enabled_for(tool: Option<crate::editor::Tool>, selected: Option<&Object>) -> bool {
    matches!(tool, Some(crate::editor::Tool::Rect | crate::editor::Tool::Ellipse))
        || selected.is_some_and(|o| matches!(o.kind, Kind::Rect { .. } | Kind::Ellipse { .. }))
}

/// 该对象是否**有可见描边**（EDT-18）：填充矩形 / 椭圆不画描边（`render::draw_rect` 现状），
/// 文本 / 高亮 / 模糊 / 聚光也没有描边，所以线型对它们无意义、按钮置灰。
pub(in crate::overlay) fn has_stroke(kind: &Kind) -> bool {
    match kind {
        Kind::Rect { filled, .. } | Kind::Ellipse { filled, .. } => !*filled,
        Kind::Arrow { .. } | Kind::Line { .. } | Kind::Freehand { .. } | Kind::Number { .. } => true,
        Kind::Text { .. } | Kind::Highlight { .. } | Kind::Blur { .. } | Kind::Glow { .. } => false,
    }
}

/// 渐变是否可用（EDT-19）：只有**真能涂渐变**的对象——文本（DirectWrite 单色混色）与
/// 荧光笔（低对比标记工具）保持纯色，模糊 / 聚光无涂色。
/// 有选中对象时看它；没有选中时看当前工具，**移动态（`None`）视为可用**——
/// 此时渐变色块只是「设定下次绘制的样式」，与纯色块一致，不能灰掉。
pub(in crate::overlay) fn gradient_enabled_for(tool: Option<crate::editor::Tool>, selected: Option<&Object>) -> bool {
    if let Some(obj) = selected {
        return kind_supports_gradient(&obj.kind);
    }
    tool.is_none_or(|t| {
        matches!(
            t,
            crate::editor::Tool::Rect
                | crate::editor::Tool::Arrow
                | crate::editor::Tool::Line
                | crate::editor::Tool::Ellipse
                | crate::editor::Tool::Freehand
                | crate::editor::Tool::Number
        )
    })
}

/// 该对象类型能否用渐变涂色。
pub(in crate::overlay) fn kind_supports_gradient(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Rect { .. }
            | Kind::Arrow { .. }
            | Kind::Line { .. }
            | Kind::Ellipse { .. }
            | Kind::Freehand { .. }
            | Kind::Number { .. }
    )
}

/// 线型是否可用（EDT-18）：有选中对象时看它自己有没有描边（切换会直接改它）；
/// 没有选中对象时才看当前工具会不会画出描边。
/// `filled` 只对矩形 / 椭圆工具有意义：填充后不描边，这时线型确实无处可见。
pub(in crate::overlay) fn line_enabled_for(
    tool: Option<crate::editor::Tool>,
    selected: Option<&Object>,
    filled: bool,
) -> bool {
    if let Some(obj) = selected {
        return has_stroke(&obj.kind);
    }
    match tool {
        Some(crate::editor::Tool::Rect | crate::editor::Tool::Ellipse) => !filled,
        Some(
            crate::editor::Tool::Arrow
            | crate::editor::Tool::Line
            | crate::editor::Tool::Freehand
            | crate::editor::Tool::Number,
        ) => true,
        Some(
            crate::editor::Tool::Text
            | crate::editor::Tool::Highlight
            | crate::editor::Tool::Blur
            | crate::editor::Tool::Glow,
        )
        | None => false,
    }
}
