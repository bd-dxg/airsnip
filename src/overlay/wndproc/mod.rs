// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层窗口过程：消息分发。修饰键状态（Ctrl/Shift/空格）与 IME 读取在 `keys`。

mod keys;

pub(super) use keys::{ctrl_down, ime_string, shift_down, space_down};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::UI::Input::Ime::ImmGetVirtualKey;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_SNAPSHOT};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::Overlay;
use super::geometry::SelRect;

/// 自定义消息：显示后异步设置前台与输入焦点。
/// `SetForegroundWindow` 在窗口刚显示时同步调用会阻塞至 ~45ms（前台锁定），
/// 改到消息循环里执行，让画面先出现（M5 性能）。
pub(super) const WM_FOCUS_OVERLAY: u32 = WM_APP + 1;

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
    match msg {
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            // 按下即隐藏悬浮提示（EDT-10）：拖拽时气泡不应挡住标注
            overlay.hide_tip();
            overlay.on_lbutton_down(x, y);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            overlay.on_mouse_move(x, y);
            // 字体面板悬停高亮（EDT-4）
            overlay.on_font_list_hover(x, y);
            // 光标随鼠标位置变：手柄→方向光标、对象/曲线控制柄→移动光标（否则系统只在
            // 位置变化且命中窗口时发 WM_SETCURSOR，拖动手柄途中光标不会更新）
            overlay.apply_cursor_at(x, y);
            // 悬浮提示（EDT-10）：悬停目标变化时重设定时器
            overlay.on_tip_hover(x, y);
            LRESULT(0)
        }
        // 悬浮提示延迟到时（EDT-10）：一次性定时器，见 `toolbar::tip`
        WM_TIMER if wparam.0 == super::toolbar::tip::TIMER_ID => {
            overlay.on_tip_timer();
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
            if !overlay.on_font_list_wheel(ox, oy, delta) && !overlay.over_color_panel(ox, oy) {
                overlay.on_mouse_wheel(sx, sy, delta, ctrl_down());
            }
            LRESULT(0)
        }
        // 右键取消：与 Esc 共用分层回退，避免误触一次性丢失标注（OUT-3/ADR 0027）
        WM_RBUTTONUP => {
            overlay.on_cancel();
            LRESULT(0)
        }
        // 双击：命中对象 → 编辑文本/恢复直线（EDT-3/EDT-4）；
        // 否则选区空白内双击 = 完成截图（复制，M3，替代回车）
        WM_LBUTTONDBLCLK => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            // 双击色块：展开取色面板改写该色块（EDT-7，issue #73 后续）；第一击已选用该色
            if overlay.on_swatch_dblclick(x, y) {
                return LRESULT(0);
            }
            let edited = overlay
                .hit_object(x, y)
                .map(|i| overlay.begin_edit_object(i))
                .unwrap_or(false);
            let handled = if edited { true } else { overlay.reset_curve_at(x, y) };
            if !handled {
                // 双击第一击在选区内空白处按下（start_draw），作废草稿并完成截图
                if let Some(sel) = overlay.selection {
                    if sel.contains(x, y) && overlay.down_started_draw {
                        overlay.draft = None;
                        overlay.finish_output();
                    }
                }
            }
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
        // 光标形状随鼠标位置决定（手柄方向 / 对象移动 / 十字），见 `cursor::cursor_at`
        WM_SETCURSOR => {
            overlay.apply_cursor_now(hwnd);
            LRESULT(1)
        }
        WM_KEYDOWN => {
            // 文本输入中：只处理输入相关按键（光标移动/退格/Enter 提交/Esc 取消），
            // 拦截全局快捷键与空格抓手切换（空格作为普通字符经 WM_CHAR 进入文本）
            if overlay.is_editing_text() {
                overlay.on_text_key(wparam.0 as u32, shift_down(), ctrl_down());
                return LRESULT(0);
            }
            // 中文输入法活动时，按键会被改写为 VK_PROCESSKEY(0xE5)，真实键另存。
            // 不禁用 IME（文本工具需要），改用 ImmGetVirtualKey 取回真实 vk，
            // 否则 IME 开启状态下按 S/F3/回车等全部失效（M4 实测缺陷）。
            let vk = if wparam.0 as u32 == 0xE5 {
                unsafe { ImmGetVirtualKey(hwnd) }
            } else {
                wparam.0 as u32
            };
            // 空格按下立即刷新光标：系统只在鼠标移动时发 WM_SETCURSOR，不能等它。
            // 贴图编辑模式（ADR 0028）下空格 = 完成（选区锁定，无需空格移动选区）。
            if vk == 0x20 {
                if overlay.locked_selection {
                    overlay.finish_output();
                } else {
                    overlay.apply_cursor_now(hwnd);
                }
                return LRESULT(0);
            }
            match vk {
                // Esc：与右键共用分层取消（先收面板 / 取消输入；有标注则二次确认，OUT-3/ADR 0027）
                0x1B => overlay.on_cancel(),
                0x0D => overlay.finish_output(), // Enter：普通=复制关闭，编辑=写回贴图（ADR 0028）
                0x43 if ctrl_down() => overlay.on_copy(), // Ctrl+C
                0x53 if ctrl_down() => overlay.on_save(), // Ctrl+S 保存（弹另存为对话框）
                // PIN-1：贴选区（键可配，默认 F3）；编辑模式下输出组不适用
                k if k == overlay.pin_key && !overlay.locked_selection => overlay.on_pin(),
                // 长截图键（默认 S，设置面板可改）：当前选区作为滚动视口（M4）
                k if k == overlay.longshot_key && !overlay.locked_selection => overlay.on_longshot(),
                // OCR 键（默认 T，设置面板可改，OCR-3）：功能关闭时不占用该键
                k if k == overlay.ocr_key && overlay.ocr_enabled && !overlay.locked_selection => overlay.on_ocr(),
                0x5A if ctrl_down() => {
                    // Ctrl+Z 撤销 / Ctrl+Shift+Z 重做（EDT-7）
                    if shift_down() {
                        overlay.on_redo()
                    } else {
                        overlay.on_undo()
                    }
                }
                0x59 if ctrl_down() => overlay.on_redo(), // Ctrl+Y
                _ => {
                    // 方向键微调（EDT-7 / CAP-2）：1px，Shift 5px；优先于工具切换键
                    if let Some((dx, dy)) = crate::app::hotkey::arrow_step(vk, shift_down()) {
                        overlay.nudge(dx, dy);
                        return LRESULT(0);
                    }
                    // 工具切换键（M3）：任意键都查映射（不限于数字键），
                    // 以便设置面板可把工具键改为字母等
                    if let Some(t) = overlay.tool_by_key(vk) {
                        overlay.switch_tool(t);
                    }
                }
            }
            LRESULT(0)
        }
        // F10 是系统菜单键、Alt+键 也走 `WM_SYSKEYDOWN`（普通键才走 `WM_KEYDOWN`，见 #90）：
        // 只接管「没按 Alt」（lparam bit 29）的动作键，其余透传保留 Alt+F4 / Alt+S 等组合。
        WM_SYSKEYDOWN => {
            let vk = wparam.0 as u32;
            let alt_down = (lparam.0 as u32 >> 29) & 1 != 0;
            let action = !overlay.locked_selection
                && (vk == overlay.pin_key
                    || vk == overlay.longshot_key
                    || (vk == overlay.ocr_key && overlay.ocr_enabled));
            if alt_down || overlay.is_editing_text() || !action {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            overlay.on_aux_key(vk);
            LRESULT(0)
        }
        // 文本字符（EDT-4）：普通字符经 TranslateMessage 转 WM_CHAR；IME 候选经组合消息
        WM_CHAR => {
            if overlay.is_editing_text() {
                overlay.on_text_char(wparam.0 as u16);
            }
            LRESULT(0)
        }
        // 中文 IME（EDT-4）：组合开始无需处理；组合更新读取候选/结果；结束清空候选
        WM_IME_STARTCOMPOSITION => LRESULT(0),
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
