// SPDX-License-Identifier: GPL-3.0-only
//! 输入法接线（EDT-4 / issue #139）：默认禁用 IME、文本输入期间恢复，并把候选框定位到自绘光标。
//!
//! 不禁用的话，中文输入法活动时按 `S`（长截图）/ 回车等键会被 IME 截走
//! （转为 `VK_PROCESSKEY` 并启动拼音组合），覆盖层快捷键全部失效。
//!
//! 文本不是系统控件（ADR 0004），IME 拿不到光标位置，候选框只能按自己的默认逻辑摆放
//! （实测微软拼音会飘到别处、且每次位置不一样）。所以这里用 `CANDIDATEFORM` /
//! `COMPOSITIONFORM` 把自绘光标的位置告诉 IME。

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM, HIMC, ImmAssociateContext, ImmCreateContext,
    ImmGetContext, ImmReleaseContext, ImmSetCandidateWindow, ImmSetCompositionWindow,
};

use super::Overlay;
use super::text_edit_state::TextEdit;

impl Overlay {
    /// 输入法开关：`true` 恢复关联的 IME 上下文（文本输入），`false` 断开（默认态）。
    pub(super) fn set_ime_enabled(&mut self, on: bool) {
        unsafe {
            if on {
                // 窗口原本就没关联上下文时新建一个，否则文本工具无法输入中文
                let ctx = if self.ime_old.0.is_null() {
                    ImmCreateContext()
                } else {
                    self.ime_old
                };
                if !ctx.0.is_null() {
                    let _ = ImmAssociateContext(self.hwnd, ctx);
                }
            } else {
                let old = ImmAssociateContext(self.hwnd, HIMC(std::ptr::null_mut()));
                if !old.0.is_null() {
                    self.ime_old = old;
                }
            }
        }
    }

    /// 把输入法候选框 / 组合串锚点挪到自绘光标处（issue #139）。
    ///
    /// 在组合开始（`WM_IME_STARTCOMPOSITION`，候选框要显示的时机）、组合更新与进入输入态时调用；
    /// 位置用 `CFS_CANDIDATEPOS` 一次性给定（不是 `CFS_DEFAULT`，那个等于「让 IME 自己猜」）。
    /// 没有输入态时直接返回：此时 IME 已断开（`set_ime_enabled(false)`），调用无意义。
    ///
    /// **坐标是客户区坐标**：覆盖层是无边框 popup（`WS_POPUP`，无标题栏/边框），客户区原点
    /// 就是窗口左上角、也就是画布原点，所以画布坐标直接可用，不需要叠加虚拟屏原点。
    /// 曾经按「屏幕坐标」传（画布 + `capture.origin_*`），普通截图下恰好等价 —— 覆盖层铺满虚拟屏、
    /// 原点 (0,0) —— 但贴图编辑（ADR 0028）的小窗口坐在 (814,463) 这种位置，候选框就整体偏出
    /// 一个窗口原点的量（issue #139 后续反馈）。实测微软拼音按客户区坐标才跟随。
    pub(super) fn sync_ime_window_pos(&self) {
        let Some(edit) = self.text_edit.as_ref() else { return };
        let (x, y) = caret_client_pos(edit);
        unsafe {
            let ctx = ImmGetContext(self.hwnd);
            if ctx.0.is_null() {
                return;
            }
            // 两个窗共用同一个锚点：候选框贴光标下方，组合串（IME 自己画的那份）与光标同点
            let candidate = CANDIDATEFORM {
                dwIndex: 0,
                dwStyle: CFS_CANDIDATEPOS,
                ptCurrentPos: POINT { x, y },
                rcArea: Default::default(),
            };
            let composition = COMPOSITIONFORM {
                dwStyle: CFS_POINT,
                ptCurrentPos: POINT { x, y },
                rcArea: Default::default(),
            };
            let cand_ok = ImmSetCandidateWindow(ctx, &candidate).as_bool();
            let comp_ok = ImmSetCompositionWindow(ctx, &composition).as_bool();
            let _ = ImmReleaseContext(self.hwnd, ctx);
            if std::env::var_os("AIRSNIP_DEBUG").is_some() {
                // 输入法「不跟随」的回报全靠这行定位：坐标是客户区，需与现实光标对比
                eprintln!("[airsnip] IME 锚点=({x},{y}) 候选框={cand_ok} 组合串={comp_ok}");
            }
        }
    }
}

/// 自绘光标底部的客户区坐标（物理像素）：`ptCurrentPos` 走客户区坐标系，而画布原点就是
/// 窗口客户区左上角，所以这里只用画布坐标、不碰屏幕/虚拟屏坐标（多显示器下也不会串位）。
/// 纯函数，几何与 `textinput::draw::draw_edit` 的光标计算一致。
pub(super) fn caret_client_pos(edit: &TextEdit) -> (i32, i32) {
    let pad = crate::editor::text::pad(edit.font_size);
    let x = edit.pos.x + edit.caret_x();
    let y = edit.pos.y + edit.caret_row() as f32 * edit.line_height() + pad * 0.4 + edit.font_size * 1.1;
    (x.round() as i32, y.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Point;

    fn edit(pos: Point) -> TextEdit {
        TextEdit {
            pos,
            obj_index: None,
            chars: "你好".chars().collect(),
            comp: Vec::new(),
            caret: 2,
            pending_high: None,
            font_size: 24.0,
            font_family: std::sync::Arc::from(crate::editor::DEFAULT_FONT_FAMILY),
            bold: false,
            italic: false,
            number: false,
        }
    }

    #[test]
    fn caret_pos_is_canvas_relative_and_below_text_top() {
        let e = edit(Point::new(100.0, 200.0));
        let (x, y) = caret_client_pos(&e);
        assert!(x > 100, "光标应在文字起始 x 之后：{x}");
        assert!(y > 200, "候选框锚点应在文字起始 y 下方：{y}");
        // 客户区坐标 == 画布坐标：同一份输入态在任何窗口位置上算出的锚点都一样，
        // 不需要（也不能）叠加虚拟屏原点 —— 否则贴图编辑的小窗口会整体偏一个窗口原点
        assert_eq!(
            caret_client_pos(&edit(Point::new(100.0, 200.0))),
            (x, y),
            "锚点只取决于输入态，与窗口/屏幕位置无关"
        );
    }

    #[test]
    fn caret_pos_moves_with_caret_and_lines() {
        let mut e = edit(Point::new(0.0, 0.0));
        let (_, y0) = caret_client_pos(&e);
        e.insert_char('\n');
        e.insert_char('a');
        let (x1, y1) = caret_client_pos(&e);
        assert!(y1 > y0, "换行后锚点应下移：{y0} → {y1}");
        e.insert_char('b'); // 同一行继续输入，光标右移
        let (x2, _) = caret_client_pos(&e);
        assert!(x2 > x1, "行内继续输入后锚点应右移：{x1} → {x2}");
    }
}
