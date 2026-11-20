// SPDX-License-Identifier: GPL-3.0-only
//! 取消截图的**分层回退**决策（OUT-3 / ADR 0027，issue #120）。
//! 从 `annotate` 拆出，控制单文件规模；纯函数便于单测，调用方见 `annotate::on_cancel`。

/// 取消截图的分层步骤（OUT-3 / ADR 0027）：由内向外逐层回退，
/// 只有会丢标注时才需确认。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum CancelStep {
    /// 收起取色面板。
    ColorPanel,
    /// 收起线型下拉（EDT-18）。
    LineMenu,
    /// 收起徽标形态下拉（EDT-20）。
    BadgeMenu,
    /// 收起字体面板。
    FontPanel,
    /// 完成当前文本 / 序号输入（右键语义：保留已输入内容）。
    CommitText,
    /// 有未保存标注，进入二次确认（同键连按两次，见 `cancel_hint`）。
    Confirm,
    /// 直接退出。
    Exit,
}

/// 取消分层决策（纯函数，便于单测）：面板优先于输入，输入优先于退出；
/// 有标注时进入二次确认。
pub(super) fn cancel_step(
    color_panel: bool,
    line_menu: bool,
    badge_menu: bool,
    font_panel: bool,
    editing: bool,
    has_annotations: bool,
) -> CancelStep {
    if color_panel {
        CancelStep::ColorPanel
    } else if line_menu {
        CancelStep::LineMenu
    } else if badge_menu {
        CancelStep::BadgeMenu
    } else if font_panel {
        CancelStep::FontPanel
    } else if editing {
        CancelStep::CommitText
    } else if has_annotations {
        CancelStep::Confirm
    } else {
        CancelStep::Exit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 分层优先级：取色面板 > 线型下拉 > 徽标下拉 > 字体面板 > 文本输入 > （有标注？确认）> 退出。
    #[test]
    fn cancel_step_recedes_layer_by_layer() {
        assert_eq!(cancel_step(true, true, true, true, true, true), CancelStep::ColorPanel);
        assert_eq!(cancel_step(false, true, true, true, true, true), CancelStep::LineMenu);
        assert_eq!(cancel_step(false, false, true, true, true, true), CancelStep::BadgeMenu);
        assert_eq!(
            cancel_step(false, false, false, true, true, true),
            CancelStep::FontPanel
        );
        assert_eq!(
            cancel_step(false, false, false, false, true, true),
            CancelStep::CommitText
        );
        // 无面板 / 无输入：有标注需确认，否则直接退出
        assert_eq!(
            cancel_step(false, false, false, false, false, true),
            CancelStep::Confirm
        );
        assert_eq!(cancel_step(false, false, false, false, false, false), CancelStep::Exit);
        // 有标注不会绕过前面的层
        assert_eq!(
            cancel_step(false, false, false, true, false, true),
            CancelStep::FontPanel
        );
    }
}
