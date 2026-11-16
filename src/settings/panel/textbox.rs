// SPDX-License-Identifier: GPL-3.0-only
//! 「保存」页路径模板的自绘单行输入框状态（OUT-2 / ADR 0041）。
//!
//! 面板此前没有任何文本输入（ADR 0004 的「自绘界面」一路自绘），这里最小化实现：
//! 字符数组 + 光标下标，支持插入 / 退格 / 删除 / 左右 / Home / End / 粘贴 / IME 结果串。
//! **不做选中与鼠标定位光标**（路径模板通常只是改几个变量，输入完就走），
//! 点击输入框 = 光标置于末尾。

use super::Panel;
use super::controls::{RowId, RowKind};
use tiny_skia::Pixmap;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::Ime::{
    CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_POINT, COMPOSITIONFORM, ImmGetContext, ImmReleaseContext,
    ImmSetCandidateWindow, ImmSetCompositionWindow,
};

/// 输入框内的编辑态。
pub(crate) struct Editing {
    chars: Vec<char>,
    caret: usize,
}

impl Editing {
    pub(super) fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        Self {
            caret: chars.len(),
            chars,
        }
    }

    pub(super) fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub(super) fn chars(&self) -> &[char] {
        &self.chars
    }

    pub(super) fn caret(&self) -> usize {
        self.caret
    }

    /// 插入一段文本（粘贴 / IME 结果串）：换行与控制字符一律拒收（单行输入框）。
    pub(super) fn insert_str(&mut self, s: &str) {
        let add: Vec<char> = s.chars().filter(|c| !c.is_control()).collect();
        if add.is_empty() {
            return;
        }
        let tail = self.chars.split_off(self.caret);
        self.chars.extend(add);
        self.caret = self.chars.len();
        self.chars.extend(tail);
    }

    pub(super) fn backspace(&mut self) {
        if self.caret > 0 {
            self.caret -= 1;
            self.chars.remove(self.caret);
        }
    }

    pub(super) fn delete(&mut self) {
        if self.caret < self.chars.len() {
            self.chars.remove(self.caret);
        }
    }

    pub(super) fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    pub(super) fn right(&mut self) {
        self.caret = (self.caret + 1).min(self.chars.len());
    }

    pub(super) fn home(&mut self) {
        self.caret = 0;
    }

    pub(super) fn end(&mut self) {
        self.caret = self.chars.len();
    }
}

/// 开始编辑：取当前生效模板作为初值（未配置时就是遗留命名模板推导出来的那个）。
pub(super) fn begin(panel: &mut Panel) {
    let text = super::savepage::file_name(panel);
    panel.editing = Some(Editing::new(&text));
    sync_ime_pos(panel);
}

/// 结束编辑并写回工作副本：清空模板 = 回到「未配置」（按遗留命名模板推导），
/// 而不是留一个空串把路径拼成空。
pub(super) fn commit(panel: &mut Panel) {
    let Some(e) = panel.editing.take() else {
        return;
    };
    let text = e.value();
    let next = if text.trim().is_empty() { None } else { Some(text) };
    if panel.config.quick_save.name != next {
        panel.config.quick_save.name = next;
        panel.config_dirty = true;
    }
}

/// 把输入法候选框锚到自绘光标处（ADR 0041；坐标为客户区，面板无边框时与窗口坐标同源）。
/// 与覆盖层文本输入（`overlay::ime`）同一套做法：`CFS_CANDIDATEPOS` + `CFS_POINT`。
pub(super) fn sync_ime_pos(panel: &Panel) {
    let Some(e) = panel.editing.as_ref() else {
        return;
    };
    let Some(row) = template_row(panel) else {
        return;
    };
    let s = panel.scale;
    let size = 15.0 * s;
    let spec = crate::render::text::FontSpec::new(size);
    let prefix: String = e.chars()[..e.caret()].iter().collect();
    let (pw, _) = crate::render::text::measure(&prefix, &spec);
    let pad = (8.0 * s) as i32;
    let x = row.control.x + pad + pw as i32;
    let y = row.control.y + row.control.h;
    unsafe {
        let ctx = ImmGetContext(panel.hwnd);
        if ctx.0.is_null() {
            return;
        }
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
        let _ = ImmSetCandidateWindow(ctx, &candidate);
        let _ = ImmSetCompositionWindow(ctx, &composition);
        let _ = ImmReleaseContext(panel.hwnd, ctx);
    }
}

/// 当前布局里「路径模板」那一行（IME 锚点与命中都用它）。
fn template_row(panel: &Panel) -> Option<super::controls::Row> {
    let layout = panel.layout();
    layout.items.into_iter().find_map(|it| match it {
        super::controls::Item::Row(r) if r.id == RowId::QuickName && r.kind == RowKind::TextInput => Some(r),
        _ => None,
    })
}

/// 输入框里「看得见的那一段」与光标偏移（纯函数，便于单测）：
/// 光标始终留在框内——先向左取够前缀，再向右补到宽度用完。
/// 起点用**二分**而不是逐字符向左收缩：逐字符是 O(n²) 的 DirectWrite 布局，
/// 上百字符的路径每按一次键都要跑一遍。
pub(super) fn visible_slice(e: &Editing, inner_w: f32, measure: &dyn Fn(&str) -> f32) -> (String, f32) {
    let chars = e.chars();
    let caret = e.caret();
    let prefix = |from: usize| -> String { chars[from..caret].iter().collect() };
    // 宽度随起点右移单调递减（空串必为 0），二分找最小的可行起点
    let budget = inner_w - 8.0;
    let (mut lo, mut hi) = (0usize, caret);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if measure(&prefix(mid)) <= budget {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let mut out = prefix(lo);
    let caret_x = measure(&out);
    // 光标右侧继续补到宽度用完（可见区通常只装得下几十个字符）
    let mut end = caret;
    while end < chars.len() {
        let mut cand = out.clone();
        cand.push(chars[end]);
        if measure(&cand) > inner_w {
            break;
        }
        out = cand;
        end += 1;
    }
    (out, caret_x)
}

/// 画输入框（供 `savepage::draw` 调用）：圆角底 + 边框（编辑中为强调色）+ 文本 + 光标。
pub(super) fn draw(pixmap: &mut Pixmap, panel: &Panel) {
    use crate::overlay::shape::rounded_rect;
    let p = panel.palette;
    let s = panel.scale;
    let Some(row) = template_row(panel) else {
        return;
    };
    let editing = panel.editing.is_some();
    let border = if editing { p.accent } else { p.border };
    rounded_rect(
        pixmap,
        row.control,
        (6.0 * s) as f32,
        [p.bg[0], p.bg[1], p.bg[2], 255],
        Some(border),
    );
    let size = 15.0 * s;
    let spec = crate::render::text::FontSpec::new(size);
    let pad = (8.0 * s) as i32;
    let (text, caret_w) = match &panel.editing {
        Some(e) => {
            let inner_w = (row.control.w - pad * 2) as f32;
            visible_slice(e, inner_w, &|t| crate::render::text::measure(t, &spec).0)
        }
        // 非编辑态：长模板尾部截断（与「预览」行同一套），否则文字会画出框外压到边框上
        None => {
            let inner_w = (row.control.w - pad * 2) as f32;
            (
                super::savepage::fit_tail(&super::savepage::file_name(panel), inner_w, &spec),
                0.0,
            )
        }
    };
    let x = row.control.x + pad;
    let y = row.control.y as f32 + (row.control.h as f32 - size * 1.2) / 2.0;
    super::draw::draw_text(pixmap, x as f32, y, &text, size, p.text);
    if editing {
        // 光标：细竖条，随文字颜色
        let caret_x = x as f32 + caret_w;
        let top = row.control.y as f32 + 6.0 * s;
        let h = row.control.h as f32 - 12.0 * s;
        if let Some(bar) = tiny_skia::Rect::from_xywh(caret_x, top, 1.5 * s, h) {
            pixmap.fill_rect(
                bar,
                &crate::overlay::shape::paint_of(p.text),
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measure(s: &str) -> f32 {
        s.chars().count() as f32 * 8.0
    }

    #[test]
    fn editing_moves_and_deletes_by_char() {
        let mut e = Editing::new("a中文");
        assert_eq!(e.caret(), 3, "光标应在末尾");
        e.left(); // 光标落在「中」「文」之间
        e.backspace(); // 删掉「中」
        assert_eq!(e.value(), "a文");
        assert_eq!(e.caret(), 1);
        e.delete(); // 删掉「文」
        assert_eq!(e.value(), "a");
        e.home();
        e.insert_str("x");
        assert_eq!(e.value(), "xa");
        e.end();
        assert_eq!(e.caret(), 2);
    }

    /// 粘贴 / IME 结果串里的换行与控制字符被拒收（单行输入框）。
    #[test]
    fn insert_str_drops_control_chars() {
        let mut e = Editing::new("");
        e.insert_str("D:\\shots\r\nx\u{7}");
        assert_eq!(e.value(), "D:\\shotsx");
    }

    /// 光标始终留在可见区内：长文本靠后编辑时，显式切出的是靠光标那一段。
    #[test]
    fn visible_slice_keeps_caret_in_view() {
        let mut e = Editing::new("D:\\a\\b\\c\\d\\e\\f\\g\\h\\i\\j\\k\\l\\m\\n\\o\\p");
        e.end();
        // 80px 只放得下 10 个字符
        let (shown, caret_x) = visible_slice(&e, 80.0, &measure);
        assert!(shown.chars().count() <= 10, "超出宽度：{shown:?}");
        assert!(caret_x <= 80.0, "光标应在框内：{caret_x}");
        // 光标在开头时从头显示
        e.home();
        let (head, caret_x) = visible_slice(&e, 80.0, &measure);
        assert!(head.starts_with("D:\\a"), "{head:?}");
        assert_eq!(caret_x, 0.0);
    }
}
