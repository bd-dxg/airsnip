// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层窗口过程：消息分发。修饰键状态（Ctrl/Shift/空格）与 IME 读取在 `keys`。

mod keydown;
mod keys;
mod savekeys;

pub(super) use keys::{ctrl_down, ime_string, lbutton_down, shift_down, space_down, win_down};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_SNAPSHOT};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::Overlay;
use super::cancel_hint;
use super::geometry::SelRect;
use super::quicksave::{self};
use super::toast;

/// 自定义消息：显示后异步设置前台与输入焦点。
/// `SetForegroundWindow` 在窗口刚显示时同步调用会阻塞至 ~45ms（前台锁定），
/// 改到消息循环里执行，让画面先出现（M5 性能）。
pub(super) const WM_FOCUS_OVERLAY: u32 = WM_APP + 1;

/// 自定义消息：UIA worker 有回包了（控件级捕捉，ADR 0039）。
/// worker 线程不碰任何 `Overlay` 状态，只投递这个信号，状态在主线程取。
pub(super) const WM_SNAP_RESULT: u32 = WM_APP + 4;

pub(super) unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // 窗口销毁（DestroyWindow 同步发送，不经消息队列）：必须 PostQuitMessage(0)
    // 让消息循环的 GetMessageW 返回 0 并退出，否则覆盖层线程永久阻塞在消息循环，
    // Overlay（含 4 份全屏位图）永不释放，每次截图都泄漏一份（实测每张 +57MB）。
    if msg == WM_DESTROY {
        unsafe {
            let _ = PostQuitMessage(0);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
    }
    let Some(overlay) = app_from(hwnd) else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    // 快速保存提示可见（OUT-2 / ADR 0041）：本次截图已落盘，输入一律收尾、不再进标注交互。
    // 闸门放在这里而不是各分支里，滚轮 / 右键 / 双击 / 系统键 才不会各漏一处。
    if savekeys::toast_gate(overlay, msg, lparam) {
        return LRESULT(0);
    }
    match msg {
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            // 任何左键动作都解除取消待命（OUT-3 / issue #120）
            overlay.clear_cancel_arm();
            // 按下即隐藏悬浮提示（EDT-10）：拖拽时气泡不应挡住标注
            overlay.hide_tip();
            overlay.on_lbutton_down(x, y);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            overlay.on_mouse_move(x, y);
            // 窗口吸附（CAP-6 / ADR 0037）：没有选区时高亮光标下的窗口矩形。
            // 只有候选真的变了才重绘；不做时间节流（迟滞 vs. 帧开销已在 ADR 里权衡）
            overlay.update_snap(x, y);
            // 字体面板悬停高亮（EDT-4）
            overlay.on_font_list_hover(x, y);
            // 线型下拉悬停高亮（EDT-18）
            overlay.on_line_menu_hover(x, y);
            // 徽标形态下拉悬停高亮（EDT-20）
            overlay.on_badge_menu_hover(x, y);
            // 光标随鼠标位置变：手柄→方向光标、对象/曲线控制柄→移动光标（否则系统只在
            // 位置变化且命中窗口时发 WM_SETCURSOR，拖动手柄途中光标不会更新）
            overlay.apply_cursor_at(x, y);
            // 悬浮提示（EDT-10）：悬停目标变化时重设定时器
            overlay.on_tip_hover(x, y);
            LRESULT(0)
        }
        // 定时器全靠 wparam 区分用途（ID 唯一性由 `tests::timer_ids_are_unique` 守门）：
        // 提示 2 / 二次确认 3 / 淡入 4 / 候选框过渡 5 / 快速保存提示 6
        WM_TIMER => {
            match wparam.0 {
                id if id == super::toolbar::tip::TIMER_ID => overlay.on_tip_timer(),
                id if id == cancel_hint::TIMER_ID => overlay.on_cancel_timer(),
                id if id == toast::TIMER_ID => overlay.on_toast_timer(),
                id if id == super::fade::TIMER_ID => overlay.on_fade_timer(),
                id if id == super::snap::ANIM_TIMER_ID => overlay.on_snap_anim_timer(),
                _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
            LRESULT(0)
        }
        // 快速保存回包（OUT-2 / ADR 0041）：后台线程写完盘了
        quicksave::WM_QUICK_SAVED => {
            overlay.on_quick_saved();
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            overlay.on_lbutton_up();
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            // lparam 为屏幕坐标（非客户区），转覆盖层坐标由 on_mouse_wheel 完成
            let sx = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let sy = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16; // ±120 单位
            // 字体面板上的滚轮用于滚动列表（EDT-4）；取色面板上滚轮不调节字号
            let (ox, oy) = (sx - overlay.capture.origin_x, sy - overlay.capture.origin_y);
            // 滚轮也是用户新动作，解除取消待命（OUT-3 / issue #120）
            overlay.clear_cancel_arm();
            // 控件级捕捉（ADR 0039）：候选窗口上有控件层级时，滚轮改为爬层级
            if !overlay.on_font_list_wheel(ox, oy, delta)
                && !overlay.over_color_panel(ox, oy)
                && !overlay.over_line_menu(ox, oy)
                && !overlay.climb_snap(delta > 0)
            {
                overlay.on_mouse_wheel(sx, sy, delta, ctrl_down());
            }
            LRESULT(0)
        }
        // 右键取消：与 Esc 共用分层回退，避免误触一次性丢失标注（OUT-3/ADR 0027）
        WM_RBUTTONUP => {
            overlay.on_cancel(cancel_hint::KEY_RBUTTON);
            LRESULT(0)
        }
        // 双击：命中对象 → 编辑文本 / 恢复直线（EDT-3/EDT-4）；否则选区空白内双击 = 完成截图
        WM_LBUTTONDBLCLK => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            overlay.on_lbutton_dblclick(x, y);
            LRESULT(0)
        }
        // 再次截图（issue #118）：本线程只注册了这一个全局热键（`recapture::register`）
        WM_HOTKEY => {
            overlay.on_recapture();
            LRESULT(0)
        }
        // 主动绘制，不靠系统擦除；只重绘无效区域
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            unsafe {
                let _ = BeginPaint(hwnd, &mut ps);
            }
            let rc = ps.rcPaint;
            let w = overlay.display.width() as i32;
            let h = overlay.display.height() as i32;
            let x0 = rc.left.clamp(0, w);
            let y0 = rc.top.clamp(0, h);
            let x1 = rc.right.clamp(0, w);
            let y1 = rc.bottom.clamp(0, h);
            if x1 > x0 && y1 > y0 {
                overlay.present_region(SelRect {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                });
            }
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        // 显示后的异步焦点：避开 SetForegroundWindow 的前台锁定阻塞
        WM_FOCUS_OVERLAY => {
            unsafe {
                let _ = SetForegroundWindow(hwnd);
                let _ = SetFocus(Some(hwnd));
            }
            LRESULT(0)
        }
        // 控件级捕捉回包（ADR 0039）：worker 算完了，主线程取结果并细化候选
        WM_SNAP_RESULT => {
            overlay.on_uia_reply();
            LRESULT(0)
        }
        // 光标形状随鼠标位置决定（手柄方向 / 对象移动 / 十字），见 `cursor::cursor_at`
        WM_SETCURSOR => {
            overlay.apply_cursor_now(hwnd);
            LRESULT(1)
        }
        // 键盘分发（含保存组合键与 Alt 组合）：见 `keydown`
        WM_KEYDOWN => keydown::on_keydown(overlay, hwnd, wparam),
        WM_SYSKEYDOWN => keydown::on_syskeydown(overlay, hwnd, msg, wparam, lparam),
        // 文本字符（EDT-4）：普通字符经 TranslateMessage 转 WM_CHAR；IME 候选经组合消息
        WM_CHAR => {
            if overlay.is_editing_text() {
                overlay.on_text_char(wparam.0 as u16);
            }
            LRESULT(0)
        }
        // 中文 IME（EDT-4）：组合开始前把候选框锚到自绘光标（issue #139）；
        // 组合更新读取候选/结果；结束清空候选
        WM_IME_STARTCOMPOSITION => {
            overlay.sync_ime_window_pos();
            LRESULT(0)
        }
        WM_IME_COMPOSITION => {
            if overlay.is_editing_text() {
                overlay.on_text_composition(lparam.0 as u32);
            }
            LRESULT(0)
        }
        WM_IME_ENDCOMPOSITION => {
            if overlay.is_editing_text() {
                overlay.on_text_composition_end();
            }
            LRESULT(0)
        }
        WM_KEYUP => {
            let vk = wparam.0 as u32;
            // 空格松开：光标回到按当前位置判定的形状
            if vk == 0x20 {
                overlay.apply_cursor_now(hwnd);
            } else if vk == VK_SNAPSHOT.0 as u32 {
                // PrintScreen 系统只派发松键消息、没有 WM_KEYDOWN（#67）
                overlay.on_aux_key(vk);
            }
            LRESULT(0)
        }
        // 鼠标中键 / 侧键当热键（ADR 0019）：长截图键 / 贴选区 / 工具切换键绑到它们时直接生效
        WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
            overlay.clear_cancel_arm();
            if !overlay.is_editing_text()
                && let Some(vk) = crate::app::mouse::message_vk(msg, wparam.0)
            {
                overlay.on_aux_key(vk);
            }
            LRESULT(1) // XBUTTON 已处理
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn app_from(hwnd: HWND) -> Option<&'static mut Overlay> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if ptr == 0 {
            None
        } else {
            Some(&mut *(ptr as *mut Overlay))
        }
    }
}

#[cfg(test)]
mod tests {
    /// 覆盖层的 `WM_TIMER` 全靠 wparam 区分用途：**ID 撞车会让一个用途的定时器被另一个
    /// 用途的处理器吞掉**（ADR 0041 把快速保存提示的 ID 设成 5，与候选框过渡动画的 5 相同，
    /// 结果一到吸附动画就把覆盖层当成「提示到点」关掉 —— 表现为按 F1 截面一闪就没）。
    /// 新增任何定时器都要进这张表。
    #[test]
    fn timer_ids_are_unique() {
        let ids = [
            ("tip", crate::overlay::toolbar::tip::TIMER_ID),
            ("cancel_hint", crate::overlay::cancel_hint::TIMER_ID),
            ("fade", crate::overlay::fade::TIMER_ID),
            ("snap_anim", crate::overlay::snap::ANIM_TIMER_ID),
            ("toast", crate::overlay::toast::TIMER_ID),
        ];
        for (i, (name_a, va)) in ids.iter().enumerate() {
            for (name_b, vb) in &ids[i + 1..] {
                assert_ne!(va, vb, "{name_a} 与 {name_b} 的 WM_TIMER ID 撞车（都是 {va}）");
            }
        }
    }
}
