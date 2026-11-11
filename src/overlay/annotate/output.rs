// SPDX-License-Identifier: GPL-3.0-only
//! 标注输出入口（OUT-1/OUT-2、PIN-1、M4、OCR-3）：复制 / 保存 / 贴图 / 长截图 / OCR。
//! 从 `annotate` 拆出，控制单文件规模；状态、工具栏动作与取消见 `super`。

use windows::Win32::Foundation::{GlobalFree, LPARAM, WPARAM};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, PostMessageW, WM_CLOSE};

use crate::overlay::{geometry::SelRect, surface::crop_pixmap};
use crate::{output, render};

impl super::super::Overlay {
    /// 输出用像素图：裁剪选区 → 把标注对象烘焙进结果图（OUT-1/OUT-2）。
    /// 标注坐标是覆盖层坐标系，需按裁剪原点平移到裁剪图坐标。
    pub(in crate::overlay) fn baked_selection(&self, sel: SelRect) -> Option<tiny_skia::Pixmap> {
        let mut cropped = crop_pixmap(&self.original, sel)?;
        render::bake_objects(&mut cropped, self.doc.objects(), (sel.x, sel.y));
        Some(cropped)
    }

    /// PIN-1：F3 / 工具栏贴图按钮 → 选区（含标注）跨线程交给主窗口贴图，并关闭覆盖层。
    /// 载荷：GlobalAlloc[u32 w][u32 h][RGBA]，主窗口创建贴图窗口后释放（app/mod.rs WM_PIN_FROM_EDITOR）。
    pub(in crate::overlay) fn on_pin(&mut self) {
        if let Some(sel) = self.selection
            && let Some(cropped) = self.baked_selection(sel)
        {
            let (w, h) = (cropped.width(), cropped.height());
            let px_total = w as usize * h as usize * 4;
            unsafe {
                // 载荷头：u32 w + u32 h + i32 x + i32 y（选区在屏幕上的位置，PIN 定位用）
                let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, 16 + px_total) else {
                    // 分配失败：提示并保留覆盖层（用户可重按 F3 重试，审查 P2-1）
                    crate::app::message_box(None, "贴图内存分配失败，请关闭部分贴图后重试。", MB_OK | MB_ICONERROR);
                    return;
                };
                let ptr = GlobalLock(hg);
                if ptr.is_null() {
                    let _ = GlobalFree(Some(hg));
                    crate::app::message_box(None, "贴图内存锁失败。", MB_OK | MB_ICONERROR);
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
                let _ = PostMessageW(
                    Some(self.main_hwnd),
                    crate::app::WM_PIN_FROM_EDITOR,
                    WPARAM(hg.0 as usize),
                    LPARAM(0),
                );
            }
        }
        // 与复制/保存一致：贴图即完成，关闭覆盖层
        self.cancelled = false;
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    /// 长截图（M4，LNG-1）：记录屏幕坐标选区并关闭覆盖层，由 app 线程启动拼接。
    pub(in crate::overlay) fn on_longshot(&mut self) {
        if let Some(sel) = self.selection {
            self.viewport = Some((
                sel.x + self.capture.origin_x,
                sel.y + self.capture.origin_y,
                sel.w as u32,
                sel.h as u32,
            ));
            self.cancelled = false;
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// OCR-3/OCR-4：OCR——把选区**原图**（不含标注）交给主线程识别，并关闭覆盖层。
    /// 载荷与贴图同格式：[u32 w][u32 h][i32 x][i32 y][RGBA]，主线程负责 OCR 与气泡窗口。
    /// OCR 常开（issue #143）：没有总开关，是否显示按钮由工具栏可见性决定。
    pub(in crate::overlay) fn on_ocr(&mut self) {
        let Some(sel) = self.selection else {
            return;
        };
        // OCR-4：只读原图——用户画的箭头/马赛克不该污染识别结果
        let Some(cropped) = crop_pixmap(&self.original, sel) else {
            return;
        };
        let (w, h) = (cropped.width(), cropped.height());
        let px_total = w as usize * h as usize * 4;
        unsafe {
            let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, 16 + px_total) else {
                crate::app::message_box(None, "OCR 内存分配失败。", MB_OK | MB_ICONERROR);
                return;
            };
            let ptr = GlobalLock(hg);
            if ptr.is_null() {
                let _ = GlobalFree(Some(hg));
                crate::app::message_box(None, "OCR 内存锁失败。", MB_OK | MB_ICONERROR);
                return;
            }
            let hp = ptr as *mut u32;
            *hp = w;
            *hp.add(1) = h;
            // 选区在屏幕上的位置：气泡按它定位（OCR-7）
            let sp = hp.add(2) as *mut i32;
            *sp = sel.x + self.capture.origin_x;
            *sp.add(1) = sel.y + self.capture.origin_y;
            std::ptr::copy_nonoverlapping(cropped.data().as_ptr(), (ptr as *mut u8).add(16), px_total);
            let _ = GlobalUnlock(hg);
            let _ = PostMessageW(
                Some(self.main_hwnd),
                crate::app::WM_OCR_REQUEST,
                WPARAM(hg.0 as usize),
                LPARAM(0),
            );
        }
        // OCR-7：触发即结束截图会话（覆盖层立即关闭，气泡由主线程弹）
        self.cancelled = false;
        self.close();
    }

    /// Enter / Ctrl+C：复制到剪贴板并关闭（OUT-1）。
    pub(in crate::overlay) fn on_copy(&mut self) {
        if let Some(sel) = self.selection {
            let Some(cropped) = self.baked_selection(sel) else {
                return;
            };
            std::thread::spawn(move || {
                if let Err(e) = output::copy_to_clipboard(&cropped) {
                    crate::app::message_box(None, &format!("复制失败：{e}"), MB_OK | MB_ICONERROR);
                }
            });
        }
        self.cancelled = false;
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }

    /// Ctrl+S / 工具栏保存按钮：弹原生另存为对话框，按所选格式保存（OUT-2）。
    /// 用户取消时保留覆盖层（可继续标注），不算截图完成。
    pub(in crate::overlay) fn on_save(&mut self) {
        let Some(sel) = self.selection else {
            return;
        };
        // 预填文件名按设置里的命名模板；格式用上次保存的（首次为 PNG）
        let ext = self.save.last_ext.as_deref().unwrap_or("png").to_string();
        let name = format!("{}.{ext}", output::base_name(self.save.name_style));
        // 初始目录：上次保存过的目录优先（OUT-2），否则默认截图目录；
        // 都不存在时传 None，交给系统自己记住的位置。
        let dir = self
            .save
            .last_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(output::default_save_dir);
        let dir = dir.is_dir().then_some(dir);
        let chosen = crate::app::filedlg::pick_save_file(self.hwnd, dir.as_deref(), &name, &ext);
        // 对话框盖在覆盖层之上：关闭后整屏重绘，否则留下对话框残影
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        self.repaint(SelRect {
            x: 0,
            y: 0,
            w: vw,
            h: vh,
        });
        let Some(path) = chosen else {
            return;
        };
        // 记住本次格式与目录：下次打开对话框时默认选中/定位（OUT-2）
        if let Some(e) = path.extension().and_then(|e| e.to_str()) {
            crate::output::recent::last_save_ext_store(e);
        }
        if let Some(parent) = path.parent() {
            crate::output::recent::last_save_dir_store(parent);
        }
        let Some(cropped) = self.baked_selection(sel) else {
            return;
        };
        std::thread::spawn(move || {
            if let Err(e) = output::save_file(&cropped, &path) {
                // 保存失败必须提示：静默吞掉会让用户以为截图已保存（OUT-2）
                crate::app::message_box(None, &format!("保存失败：{e}"), MB_OK | MB_ICONERROR);
            }
        });
        self.cancelled = false;
        self.close();
    }
}
