// SPDX-License-Identifier: GPL-3.0-only
//! 文本输入与系统剪贴板互通（issue #220 / ADR 0049）：`Ctrl+A` 全选、`Ctrl+C` 复制、`Ctrl+V` 粘贴，
//! 以及复制 / 粘贴的一次性轻提示（「已复制 12 字」）。
//!
//! 从 `textinput` 拆出：那边已接近单文件规模上限，而这里还要带提示的几何与绘制。
//! 字符级处理在 `text_edit_state`，按键分发在 `textinput::on_text_key`。
//!
//! 本批**只做「全选 / 无」两态**，没有任意区间选区（拖选 / `Shift` 扩选见 ADR 0049）：
//! 因此「复制选中」实际就是「复制全文」，`Ctrl+A` 只提供一层可见确认。

use tiny_skia::Pixmap;

use crate::editor::Point;
use crate::i18n::{self, OverlayKey as O};
use crate::overlay::geometry::{SelRect, expand_rect};
use crate::overlay::shape::rounded_rect;
use crate::overlay::text_edit_state::{TextEdit, TextHint};
use crate::overlay::ui::Ui;
use crate::render::text::{self as dwrite, FontSpec};

/// 粘贴字符数上限：文本标注走 DirectWrite 每帧量宽（`caret_x` / `region`），
/// 上万字会明显卡顿；4096 字足够放一段说明，超出截断并**明确告诉用户**（静默截断更糟）。
pub(super) const PASTE_LIMIT: usize = 4096;

/// 轻提示与输入框的间距（基准像素）。
const GAP_BASE: f32 = 4.0;
/// 提示水平 / 垂直内边距。
const PAD_X_BASE: f32 = 8.0;
const PAD_Y_BASE: f32 = 4.0;
/// 提示圆角半径。
const RADIUS_BASE: f32 = 6.0;
/// 提示字号（与 `toast` / `cancel_hint` 同档：小一号会看着像「缩水的小字」，实测被用户指出太小）。
const FONT_BASE: f32 = 14.0;
/// 离屏幕边缘的最小留白。
const MARGIN_BASE: f32 = 4.0;
/// 提示底色与文字色：深色不透明底 + 白字，任意截图内容上都清晰，不随主题变化。
const BG: [u8; 4] = [32, 32, 32, 220];
const FG: [u8; 4] = [255, 255, 255, 255];

/// 编辑态的剪贴板组合键。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ClipAction {
    SelectAll,
    Copy,
    Paste,
}

/// 按键 → 剪贴板动作；不是这三组之一返回 `None`（交回普通按键处理）。
pub(super) fn action(vk: u32, ctrl: bool) -> Option<ClipAction> {
    if !ctrl {
        return None;
    }
    match vk {
        0x41 => Some(ClipAction::SelectAll), // A
        0x43 => Some(ClipAction::Copy),      // C
        0x56 => Some(ClipAction::Paste),     // V
        _ => None,
    }
}

/// 归一化粘贴文本：统一换行为 `\n`、丢掉其它控制字符（含制表符 —— 自绘排版按字符量宽，
/// 制表位无处锚定），并按 `PASTE_LIMIT` 截断。返回（文本, 是否截断）。
pub(super) fn normalize_paste(raw: &str) -> (String, bool) {
    // 先统一换行再过滤，避免 `\r\n` 被当成两个换行（Windows 剪贴板全是 `\r\n`）
    let unified = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(unified.len().min(PASTE_LIMIT));
    let mut count = 0usize;
    let mut truncated = false;
    for c in unified.chars() {
        if c != '\n' && c.is_control() {
            continue;
        }
        if count >= PASTE_LIMIT {
            truncated = true;
            break;
        }
        out.push(c);
        count += 1;
    }
    (out, truncated)
}

/// 提示几何：优先贴在输入框**下方**左对齐；下方塞不下翻到上方，最后钳回屏幕内。
/// 纯函数（便于单测）。
pub(super) fn hint_rect(text: &str, anchor: SelRect, vw: i32, vh: i32, ui: Ui) -> SelRect {
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(text, &spec);
    let w = (tw + ui.f(PAD_X_BASE) * 2.0).ceil() as i32;
    let h = (th + ui.f(PAD_Y_BASE) * 2.0).ceil() as i32;
    let gap = ui.px(GAP_BASE);
    let margin = ui.px(MARGIN_BASE);
    let below = anchor.y + anchor.h + gap;
    let above = anchor.y - gap - h;
    let y = if below + h + margin <= vh {
        below
    } else if above >= margin {
        above
    } else {
        // 上下都塞不下（输入框几乎占满屏幕）：贴着输入框底边，宁可压住一点点内容
        (anchor.y + anchor.h).min(vh - h - margin).max(margin)
    };
    SelRect {
        x: anchor.x.clamp(margin, (vw - w - margin).max(margin)),
        y: y.clamp(margin, (vh - h - margin).max(margin)),
        w,
        h,
    }
}

/// 绘制提示（屏幕显示层，不进导出烘焙）。
pub(super) fn draw_hint(pixmap: &mut Pixmap, hint: &TextHint, ui: Ui) {
    rounded_rect(pixmap, hint.rect, ui.f(RADIUS_BASE), BG, None);
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(&hint.text, &spec);
    let x = hint.rect.x as f32 + (hint.rect.w as f32 - tw) / 2.0;
    let y = hint.rect.y as f32 + (hint.rect.h as f32 - th) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x, y),
        &hint.text,
        &spec,
        FG,
        tiny_skia::Transform::identity(),
    );
}

impl super::super::Overlay {
    /// 处理编辑态的剪贴板组合键（issue #220）。
    ///
    /// 「改状态」与「算几何 / 重绘」分两步：前者需要 `&mut TextEdit` 的借用，
    /// 后者要 `&mut self`（画布尺寸、UI 缩放、重绘），不能同时握着。
    pub(super) fn handle_clip_key(&mut self, action: ClipAction) {
        if self.text_edit.is_none() {
            return;
        }
        // 旧提示先收掉（连同它原来的矩形一起重绘：清掉之后 `region()` 不再包含它）
        self.drop_text_hint();
        let hint_text = {
            let Some(edit) = self.text_edit.as_mut() else { return };
            // 徽标内容编辑（EDT-12 / EDT-20）只收单个数字或勾叉，剪贴板互不适用
            if edit.number {
                return;
            }
            match action {
                ClipAction::SelectAll => {
                    edit.select_all_text();
                    None
                }
                ClipAction::Copy => copy(edit),
                ClipAction::Paste => paste(edit),
            }
        };
        if let Some(text) = hint_text {
            self.set_text_hint(text);
        }
        self.repaint_whole_selection();
    }

    /// 收起轻提示（issue #220）。提示画在输入框**之外**，清掉后 `region()` 就不再包含它，
    /// 所以必须把它原来的矩形单独重绘一次，否则「已复制」会留在屏幕上。
    pub(super) fn drop_text_hint(&mut self) {
        let rect = self.text_edit.as_mut().and_then(|e| e.hint.take()).map(|h| h.rect);
        if let Some(r) = rect {
            self.repaint(expand_rect(r, 2));
        }
    }

    /// 按当前输入框位置算出提示矩形并记下（重绘由调用方统一做）。
    fn set_text_hint(&mut self, text: String) {
        let Some(edit) = self.text_edit.as_ref() else { return };
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let rect = hint_rect(&text, edit.region(), vw, vh, self.ui);
        if let Some(e) = self.text_edit.as_mut() {
            e.hint = Some(TextHint { rect, text });
        }
    }
}

/// `Ctrl+C`：复制全部已确认文本，返回提示文案（空文本时不复制也不提示）。
fn copy(edit: &mut TextEdit) -> Option<String> {
    let text = edit.all_text();
    if text.is_empty() {
        return None;
    }
    let n = text.chars().count().to_string();
    Some(match crate::output::copy_text_to_clipboard(&text) {
        Ok(()) => i18n::fill(O::TextCopied.text(), &[("n", &n)]),
        // 剪贴板被别的程序占着是常事（OpenClipboard 会失败），不弹框、只给一句提示
        Err(_) => O::TextCopyFailed.text().to_string(),
    })
}

/// `Ctrl+V`：把剪贴板文字插到光标处（有全选时替换全部），返回提示文案。
fn paste(edit: &mut TextEdit) -> Option<String> {
    let Some(raw) = crate::output::read_text_from_clipboard() else {
        return Some(O::TextPasteEmpty.text().to_string());
    };
    let (text, truncated) = normalize_paste(&raw);
    if text.is_empty() {
        // 剪贴板里只有图片 / 空白 / 全是控制字符：都按「没有文字」处理
        return Some(O::TextPasteEmpty.text().to_string());
    }
    let n = edit.insert_str(&text).to_string();
    truncated.then(|| i18n::fill(O::TextPasteTruncated.text(), &[("n", &n)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(x: i32, y: i32, w: i32, h: i32) -> SelRect {
        SelRect { x, y, w, h }
    }

    #[test]
    fn only_ctrl_combos_map_to_actions() {
        assert_eq!(action(0x41, true), Some(ClipAction::SelectAll));
        assert_eq!(action(0x43, true), Some(ClipAction::Copy));
        assert_eq!(action(0x56, true), Some(ClipAction::Paste));
        // 无 Ctrl：裸 A / C / V 是普通字符与工具键，不归这里管
        assert_eq!(action(0x41, false), None);
        assert_eq!(action(0x43, false), None);
        // Ctrl+其它（如 Ctrl+Z 取消输入）不归这里管
        assert_eq!(action(0x5A, true), None);
    }

    /// Windows 剪贴板里全是 `\r\n`：必须归一成一个 `\n`，否则每行之间多一个空行。
    #[test]
    fn normalize_unifies_newlines() {
        let (s, truncated) = normalize_paste("a\r\nb\r\nc");
        assert_eq!(s, "a\nb\nc");
        assert!(!truncated);
        // 纯 `\r`（老式 Mac / 某些程序）同样当换行
        let (s, _) = normalize_paste("a\rb");
        assert_eq!(s, "a\nb");
    }

    /// 控制字符（制表符 / NUL / 退格）丢弃，换行保留。
    #[test]
    fn normalize_drops_control_chars_but_keeps_newline() {
        let (s, _) = normalize_paste("a\tb\u{0}c\nd\u{8}");
        assert_eq!(s, "abc\nd");
    }

    /// 超出上限截断，并标记 `truncated`（调用方据此给提示，不静默丢字）。
    #[test]
    fn normalize_truncates_at_limit() {
        let raw = "字".repeat(PASTE_LIMIT + 10);
        let (s, truncated) = normalize_paste(&raw);
        assert_eq!(s.chars().count(), PASTE_LIMIT);
        assert!(truncated);
        // 刚好等于上限时不算截断
        let (s, truncated) = normalize_paste(&"字".repeat(PASTE_LIMIT));
        assert_eq!(s.chars().count(), PASTE_LIMIT);
        assert!(!truncated);
    }

    /// 提示优先贴在输入框下方左对齐，且完整落在屏幕内。
    #[test]
    fn hint_sits_below_box_left_aligned() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let anchor = sel(400, 300, 200, 40);
        let r = hint_rect("已复制 12 字", anchor, vw, vh, ui);
        assert!(r.y >= anchor.y + anchor.h, "应在输入框下方：{r:?}");
        assert_eq!(r.x, anchor.x, "应与输入框左对齐：{r:?}");
        assert!(r.x >= 0 && r.x + r.w <= vw && r.y + r.h <= vh);
    }

    /// 输入框贴屏幕底：下方塞不下 → 翻到上方。
    #[test]
    fn hint_flips_above_when_no_room_below() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let anchor = sel(400, 1020, 200, 60);
        let r = hint_rect("已复制 12 字", anchor, vw, vh, ui);
        assert!(r.y + r.h <= anchor.y, "应放在输入框上方：{r:?} / {anchor:?}");
    }

    /// 贴边 / 极小输入框：提示仍钳回屏幕内（不越界）。
    #[test]
    fn hint_clamps_into_screen() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1366, 768);
        for anchor in [sel(0, 0, 1, 1), sel(1350, 0, 16, 700), sel(0, 700, 1366, 68)] {
            let r = hint_rect("已粘贴（截断至 4096 字）", anchor, vw, vh, ui);
            assert!(r.x >= 0 && r.x + r.w <= vw, "越出屏幕：{r:?}（锚 {anchor:?}）");
            assert!(r.y >= 0 && r.y + r.h <= vh, "越出屏幕：{r:?}（锚 {anchor:?}）");
        }
    }

    /// 高 DPI 下提示同步放大（CAP-5）。
    #[test]
    fn hint_scales_with_dpi() {
        let anchor = sel(100, 100, 200, 40);
        let norm = hint_rect("已复制 12 字", anchor, 1920, 1080, Ui::from_dpi(96));
        let hidpi = hint_rect("已复制 12 字", anchor, 3840, 2160, Ui::from_dpi(192));
        assert!(hidpi.h > norm.h && hidpi.w > norm.w, "高 DPI 应放大");
    }
}
