// SPDX-License-Identifier: GPL-3.0-only
//! 贴图编辑模式的输出（ADR 0028，issue #56）：完成 = 把烘焙结果写回贴图（跨线程交主窗口），
//! 以及「完成输出」分派（普通截图 → 复制保存，编辑 → 写回贴图）。
//! 从 `annotate` 拆出，控制单文件规模。

use windows::Win32::Foundation::{GlobalFree, LPARAM, WPARAM};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, PostMessageW};

impl super::Overlay {
    /// 完成输出（ADR 0028）：普通截图 = 复制并关闭；贴图编辑 = 写回贴图并关闭。
    /// `Enter` / `空格` / 选区内双击在编辑模式下走这里。
    pub(super) fn finish_output(&mut self) {
        if self.locked_selection {
            self.on_pin_edit_done();
        } else {
            self.on_copy();
        }
    }

    /// 贴图编辑完成（ADR 0028）：烘焙选区（贴图内容 + 新标注）跨线程交回主窗口写回贴图。
    /// 载荷与贴图同格式：[u32 w][u32 h][i32 x][i32 y][RGBA]；lparam 带目标贴图句柄。
    pub(super) fn on_pin_edit_done(&mut self) {
        let Some(sel) = self.selection else {
            return;
        };
        let Some(cropped) = self.baked_selection(sel) else {
            return;
        };
        let (w, h) = (cropped.width(), cropped.height());
        let px_total = w as usize * h as usize * 4;
        unsafe {
            let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, 16 + px_total) else {
                crate::app::message_box(None, "贴图更新内存分配失败。", MB_OK | MB_ICONERROR);
                return;
            };
            let ptr = GlobalLock(hg);
            if ptr.is_null() {
                let _ = GlobalFree(Some(hg));
                crate::app::message_box(None, "贴图更新内存锁失败。", MB_OK | MB_ICONERROR);
                return;
            }
            let hp = ptr as *mut u32;
            *hp = w;
            *hp.add(1) = h;
            let sp = hp.add(2) as *mut i32;
            *sp = sel.x;
            *sp.add(1) = sel.y;
            std::ptr::copy_nonoverlapping(cropped.data().as_ptr(), (ptr as *mut u8).add(16), px_total);
            let _ = GlobalUnlock(hg);
            let target = self.pin_edit_target.unwrap_or(0);
            let _ = PostMessageW(
                Some(self.main_hwnd),
                crate::app::WM_PIN_EDIT_DONE,
                WPARAM(hg.0 as usize),
                LPARAM(target as isize),
            );
        }
        self.cancelled = false;
        self.close();
    }
}
