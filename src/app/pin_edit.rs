// SPDX-License-Identifier: GPL-3.0-only
//! 贴图编辑编排（ADR 0028，issue #56）：贴图窗口按空格 / 右键「编辑」后，主线程起一个
//! 只包住「贴图 + 工具栏」的小窗口覆盖层进入标注态，完成后写回同一张贴图。
//! 从 `app::mod` 拆出，控制单文件规模。

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{App, ID_HOTKEY_PIN, OVERLAY_RUNNING, WM_OVERLAY_DONE, message_box};

impl App {
    /// 贴图编辑（ADR 0028，issue #56）：把某张贴图的显示像素作为底图，起一个
    /// 只包住「贴图 + 工具栏」的小窗口覆盖层，直接进入标注态；完成后写回同一张贴图。
    pub fn start_pin_edit(&mut self, pin: HWND) {
        if OVERLAY_RUNNING.swap(true, Ordering::AcqRel) {
            return; // 覆盖层已在运行（含另一张贴图正在编辑）
        }
        let Some((cx, cy, cw, ch, pixels)) = crate::pin::read_for_edit(pin) else {
            OVERLAY_RUNNING.store(false, Ordering::Release);
            return;
        };
        // 先算工具栏尺寸（隐藏输出组）以确定小窗口矩形；工具栏默认在贴图下方。
        // 高度按**两行**预留：编辑中切到文本工具（或选中文本对象）会多出第二行，
        // 须提前容纳，否则工具栏会翻到选区上方压住贴图（审查 P1）。
        let ui = crate::overlay::ui::Ui::resolve(self.config.ui_scale);
        let probe = crate::overlay::geometry::SelRect {
            x: 0,
            y: 0,
            w: cw as i32,
            h: ch as i32,
        };
        let bar = crate::overlay::toolbar::layout_ex(
            probe,
            1 << 20,
            1 << 20,
            &crate::overlay::toolbar::Shown {
                vis: &self.config.toolbar,
                text_row: true,
                show_output: false,
                // 贴图编辑模式选区锁定：没有「移动」按钮
                show_move: false,
            },
            ui,
        );
        let (margin, offset) = (ui.px(4.0), ui.px(8.0));
        let (content_w, content_h) = (cw as i32, ch as i32);
        // 工具栏可见性（EDT-16）：元素全隐藏时根本没有工具栏，窗口只需包住贴图
        let (bar_w, bar_h) = bar.as_ref().map(|b| (b.rect.w, b.rect.h)).unwrap_or((0, 0));
        let win_w = content_w.max(bar_w) + margin * 2;
        let content_off_x = margin + (bar_w - content_w).max(0) / 2;
        let content_off_y = margin;
        // 高度：内容 + 间隔 + 工具栏（两行）+ 边距（编辑模式不画尺寸提示）
        let win_h = margin + content_h + if bar_h > 0 { offset + bar_h } else { 0 } + margin;
        let (vx, vy, vw, vh) = virtual_screen();
        // 贴图过大、编辑窗口放不下整个屏幕：直接拒绝并提示（否则工具栏会跑到屏外，审查 P2）
        if win_w > vw || win_h > vh {
            message_box(
                None,
                "该贴图过大，编辑窗口在当前屏幕上放不下。\n请先缩小贴图后再编辑。",
                MB_OK | MB_ICONERROR,
            );
            OVERLAY_RUNNING.store(false, Ordering::Release);
            return;
        }
        // 尽量贴着贴图原位，超出屏幕则夹回来（贴图已隐藏，位置无需严格对齐）
        let win_x = (cx - content_off_x).clamp(vx, vx + vw - win_w);
        let win_y = (cy - content_off_y).clamp(vy, vy + vh - win_h);

        // 抓屏后再隐藏贴图：底图或含贴图（或被 seed 覆盖），无时序竞态
        let cap = match crate::capture::capture_region(win_x, win_y, win_w as u32, win_h as u32) {
            Ok(c) => c,
            Err(e) => {
                message_box(None, &format!("抓屏失败：{e}"), MB_OK | MB_ICONERROR);
                OVERLAY_RUNNING.store(false, Ordering::Release);
                return;
            }
        };
        crate::pin::hide(pin);
        self.pin_edit_target = Some(pin);

        let hinstance = self.hinstance.0 as usize;
        let main_hwnd = self.hwnd.0 as usize; // HWND 非 Send，跨线程传原生值
        // 编辑期间撤销贴图全局热键（同 start_capture）
        let _ = unsafe { UnregisterHotKey(Some(self.hwnd), ID_HOTKEY_PIN) };
        let opts = crate::overlay::OverlayOpts {
            palette: crate::theme::palette(crate::theme::resolve(self.config.theme)),
            tool_keys: self.config.tool_keys.clone(),
            longshot_key: self.config.hotkey.longshot_key,
            visibility: self.config.toolbar.clone(),
            confirm_cancel: self.config.confirm_cancel,
            pin_key: self.config.hotkey.pin_key,
            // 贴图编辑模式不注册「再次截图」键（选区锁定）：显式传未绑定，避免看着像生效
            recapture_key: crate::settings::UNBOUND,
            recapture_modifiers: 0,
            double_click: self.config.double_click_action,
            colors: self.config.swatch_colors,
            save: crate::settings::SavePrefs {
                name_style: self.config.name_style,
                last_ext: self.config.last_save_ext.clone(),
                last_dir: self.config.last_save_dir.clone(),
            },
            ui_scale: self.config.ui_scale,
            pin_edit: Some(crate::overlay::PinEdit {
                rect: (content_off_x, content_off_y, cw, ch),
                pixels,
                target: pin.0 as usize,
            }),
        };
        std::thread::spawn(move || {
            let hinstance = HINSTANCE(hinstance as *mut _);
            let main_hwnd = HWND(main_hwnd as *mut _);
            let _ = crate::overlay::run(cap, opts, hinstance, main_hwnd);
            OVERLAY_RUNNING.store(false, Ordering::Release);
            // 请求主线程重注册贴图热键；若未走完成消息（取消/复制/保存），恢复贴图显示
            let _ = unsafe { PostMessageW(Some(main_hwnd), WM_OVERLAY_DONE, WPARAM(0), LPARAM(0)) };
        });
    }
}

impl App {
    /// 贴图右键「另存为」（OUT-2）：弹原生保存对话框，按所选格式保存贴图内容像素。
    /// 复用截图保存的偏好：文件名模板 + 上次格式 / 目录。
    pub fn save_pin_as(&mut self, pin: HWND) {
        let Some(pm) = crate::pin::export_content(pin) else {
            return;
        };
        let ext = self.config.last_save_ext.as_deref().unwrap_or("png").to_string();
        let name = format!("{}.{ext}", crate::output::base_name(self.config.name_style));
        let dir = self
            .config
            .last_save_dir
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(crate::output::default_save_dir);
        let dir = dir.is_dir().then_some(dir);
        let Some(path) = crate::app::filedlg::pick_save_file(pin, dir.as_deref(), &name, &ext) else {
            return;
        };
        // 记住本次格式与目录（同覆盖层保存，OUT-2）
        if let Some(e) = path.extension().and_then(|e| e.to_str()) {
            self.config.last_save_ext = Some(e.to_string());
        }
        if let Some(parent) = path.parent() {
            self.config.last_save_dir = Some(parent.to_string_lossy().into_owned());
        }
        self.config.save();
        // 文件写出（含 WebP 编码）丢到后台线程，避免阻塞主线程 UI（审查 P3）
        std::thread::spawn(move || {
            if let Err(e) = crate::output::save_file(&pm, &path) {
                message_box(None, &format!("保存失败：{e}"), MB_OK | MB_ICONERROR);
            }
        });
    }
}

/// 虚拟屏幕矩形（多显示器统一坐标）。
fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}
