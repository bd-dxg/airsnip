// SPDX-License-Identifier: GPL-3.0-only
//! 再次截图（issue #118）：截图过程中再按一次「再次截图」键（默认 `Shift+F1`），
//! 把覆盖层**当前画面**当作新底图重新框选 —— 用来截取本软件自己的界面
//! （工具栏、选区、标注都在画面里，见 issue #118 的场景）。
//!
//! 覆盖层本身铺满整个虚拟屏幕，`display` 就是此刻屏幕上的内容，所以「再次截图」
//! 不需要再抓一次屏：直接把 `display` 拷回 `original`、重新生成暗图即可
//! （无窗口闪烁、无额外抓屏开销，HDR / 混合 DPI 布局也不必再走一遍）。
//!
//! 快捷键由**覆盖层线程自己注册**：只在该窗口存活期间有效，覆盖层关闭 / 线程退出即失效；
//! 主线程不必维护生命周期，也不用跨线程找窗口句柄。

use tiny_skia::Pixmap;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::KillTimer;

use super::geometry::SelRect;
use super::interaction::Phase;
use super::toolbar::tip;
use super::{Overlay, cancel_hint, surface};
use crate::editor::Document;

/// 热键 ID（覆盖层线程内的资源 id 空间：定时器占了 2 / 3，热键取 201；
/// 与主窗口的 `app::ID_HOTKEY_*` 互不相干，两者按线程归属不会冲突）。
pub(super) const HOTKEY_ID: i32 = 201;

/// 注册「再次截图」键（默认 `Shift+F1`）。
/// 失败静默：这只是截图期间的便利键，被别的程序占用时退化成「取消后重新按截图键」，
/// 不该弹框打断正在进行的截图（`AIRSNIP_DEBUG=1` 时打印一行）。
pub(super) fn register(hwnd: HWND, key: u32, modifiers: u32) {
    // 未绑定（0）与鼠标键（RegisterHotKey 不支持，ADR 0019）都不注册
    if key == crate::settings::UNBOUND || crate::app::mouse::is_mouse_key(key) {
        return;
    }
    let ok = unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID, crate::app::hotkey::mod_flags(modifiers), key) }.is_ok();
    if !ok && crate::app::mouse::debug_enabled() {
        eprintln!(
            "[airsnip] 再次截图键注册失败（{key:#x} + {modifiers}）：{}",
            unsafe { windows::Win32::Foundation::GetLastError().0 }
        );
    }
}

/// 注销（显式做一遍；线程退出时系统也会自动清理本线程注册的热键）。
pub(super) fn unregister(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
    }
}

impl Overlay {
    /// 再次截图：当前画面 → 新底图，清空选区 / 标注 / 面板，回到「框选」阶段。
    pub(super) fn on_recapture(&mut self) {
        if self.locked_selection {
            return; // 贴图编辑模式（ADR 0028）：选区锁定
        }
        self.reset_interaction();
        rebase(&mut self.original, &mut self.dimmed, &mut self.display);
        // `display` 已是暗图，首帧不必再从 `dimmed` 回贴（与 `init` 的首帧同理）
        self.first_paint = true;
        let (w, h) = (self.display.width() as i32, self.display.height() as i32);
        self.repaint(SelRect { x: 0, y: 0, w, h });
        self.apply_cursor_now(self.hwnd);
    }

    /// 清空交互状态（输入 / 面板 / 提示 / 取消待命 / 标注 / 选区），
    /// 回到「刚打开覆盖层、还没框选」的样子。工具、颜色、线宽等偏好保留。
    fn reset_interaction(&mut self) {
        // 文本 / 序号输入：放弃未提交内容（与 Esc 取消输入一致），并重新禁用输入法
        if self.text_edit.take().is_some() {
            self.set_ime_enabled(false);
            self.doc.commit(false);
        }
        // 展开的面板与气泡直接丢弃（整屏重绘会一并擦掉），同时停掉两个一次性定时器
        self.font_list = None;
        self.color_panel = None;
        self.color_edit = false;
        self.color_edit_before = None;
        self.tip = tip::Tip::default();
        self.cancel_arm = None;
        self.cancel_hint = None;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), tip::TIMER_ID);
            let _ = KillTimer(Some(self.hwnd), cancel_hint::TIMER_ID);
        }
        // 标注与选区：全部清空，回到「框选」阶段
        self.doc = Document::new();
        self.draft = None;
        self.pending_click = None;
        self.down_started_draw = false;
        self.selected = None;
        self.ctrl_drag = None;
        self.obj_drag = None;
        self.drag = None;
        self.selection = None;
        self.bar = None;
        self.phase = Phase::Adjusting;
        self.viewport = None;
        self.cancelled = false;
    }
}

/// 把当前画面当作新底图：`original ← display`（当时的屏幕画面）、
/// `dimmed = dim(original)`、`display ← dimmed`。
/// 三份缓冲原地复制，不新分配全屏内存；抽成无窗口依赖的纯函数便于单测。
fn rebase(original: &mut Pixmap, dimmed: &mut Pixmap, display: &mut Pixmap) {
    // 三份 pixmap 由同一次抓屏尺寸初始化、没有任何 resize 路径，尺寸必须一致；
    // 不齐时 copy_from_slice 会在窗口过程里 panic（= 进程 abort），故先断言。
    debug_assert_eq!(
        original.data().len(),
        display.data().len(),
        "再次截图：三份缓冲尺寸必须一致"
    );
    debug_assert_eq!(
        dimmed.data().len(),
        display.data().len(),
        "再次截图：三份缓冲尺寸必须一致"
    );
    original.data_mut().copy_from_slice(display.data());
    dimmed.data_mut().copy_from_slice(display.data());
    surface::dim_pixmap(dimmed, surface::DIM_K);
    display.data_mut().copy_from_slice(dimmed.data());
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(Color::from_rgba8(c[0], c[1], c[2], c[3]));
        p
    }

    /// 再次截图的核心语义：`original` 取「当时的屏幕画面」，`display` 变成它的暗图，
    /// 而且**只暗一次**（重复暗化会让每次再次截图都越来越黑）。
    #[test]
    fn rebase_uses_current_frame_once_dimmed() {
        let (w, h) = (4, 3);
        let mut original = solid(w, h, [10, 20, 30, 255]);
        let mut dimmed = solid(w, h, [0, 0, 0, 255]);
        let mut display = solid(w, h, [200, 100, 50, 255]);

        rebase(&mut original, &mut dimmed, &mut display);

        assert_eq!(&original.data()[0..4], &[200, 100, 50, 255], "底图应取屏幕画面");
        let k = surface::DIM_K;
        let expect = [
            ((200 * k) >> 8) as u8,
            ((100 * k) >> 8) as u8,
            ((50 * k) >> 8) as u8,
            255,
        ];
        assert_eq!(&dimmed.data()[0..4], &expect, "暗图应为原图乘 {k}/256");
        assert_eq!(display.data(), dimmed.data(), "display 必须等于暗图本身（只暗一次）");
        assert_ne!(&display.data()[0..3], &original.data()[0..3], "非选区应确实变暗");
    }
}
