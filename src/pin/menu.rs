// SPDX-License-Identifier: GPL-3.0-only
//! 贴图右键菜单（PIN-4）：编辑、复制、保存、关闭、关闭全部。
//! 导出像素：去除阴影扩展、premultiplied → straight 还原（经 `pixmap_of`）。

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

use super::window::PinWindow;

const IDM_PIN_EDIT: usize = 41005;
const IDM_PIN_COPY: usize = 41001;
const IDM_PIN_SAVE: usize = 41002;
const IDM_PIN_CLOSE: usize = 41003;
const IDM_PIN_CLOSE_ALL: usize = 41004;

/// 显示右键菜单并分发动作（TrackPopupMenu 模态，内部自带消息循环）。
pub(super) fn on_context_menu(win: &mut PinWindow) {
    use crate::i18n::OverlayKey as O;
    let hwnd = win.hwnd();
    // 文案随界面语言（I18N-2 / ADR 0047）：`AppendMenuW` 要 NUL 结尾的宽串，
    // 不能再用编译期常量 `w!("编辑")`。先全部构造好，再逐个传指针（串要活到调用结束）。
    let labels = [
        O::PinMenuEdit.text(),
        O::PinMenuCopy.text(),
        O::PinMenuSave.text(),
        O::PinMenuClose.text(),
        O::PinMenuCloseAll.text(),
    ]
    .map(|s| s.encode_utf16().chain([0]).collect::<Vec<u16>>());
    let selected = unsafe {
        let mut pt = std::mem::zeroed();
        let _ = GetCursorPos(&mut pt);
        let menu = CreatePopupMenu().ok();
        let Some(menu) = menu else { return };
        let _ = AppendMenuW(menu, MF_STRING, IDM_PIN_EDIT, PCWSTR(labels[0].as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, IDM_PIN_COPY, PCWSTR(labels[1].as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, IDM_PIN_SAVE, PCWSTR(labels[2].as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, IDM_PIN_CLOSE, PCWSTR(labels[3].as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, IDM_PIN_CLOSE_ALL, PCWSTR(labels[4].as_ptr()));
        let cmd = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        cmd.0 as usize
    };
    match selected {
        // 进入标注编辑（ADR 0028，issue #56）：与贴图内按空格等价
        IDM_PIN_EDIT => super::request_edit(hwnd),
        IDM_PIN_COPY => {
            if let Some(pm) = win.export_content() {
                std::thread::spawn(move || {
                    if let Err(e) = crate::output::copy_to_clipboard(&pm) {
                        crate::app::message_box(
                            None,
                            &crate::i18n::fill(crate::i18n::OverlayKey::PinCopyFailed.text(), &[("e", &e.to_string())]),
                            MB_OK | MB_ICONERROR,
                        );
                    }
                });
            }
        }
        IDM_PIN_SAVE => {
            // 另存为（OUT-2）：交主窗口弹原生对话框（App 持有配置，可记住上次格式与目录）
            let _ = unsafe {
                let Ok(main) = FindWindowW(crate::app::WINDOW_CLASS, None) else {
                    return;
                };
                PostMessageW(
                    Some(main),
                    crate::app::WM_PIN_SAVE_REQUEST,
                    WPARAM(hwnd.0 as usize),
                    LPARAM(0),
                )
            };
        }
        IDM_PIN_CLOSE => {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        IDM_PIN_CLOSE_ALL => super::close_all(),
        _ => {}
    }
}
