// SPDX-License-Identifier: GPL-3.0-only
//! `App` 消息处理器（SYS-3/7、OUT-2、PIN-1、ADR 0028）：截图、设置面板、配置应用等。
//! 从 `mod.rs` 拆出，控制单文件规模；入口 `run` 与窗口过程见 `super` 与 `super::wndproc`。

use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_ICONINFORMATION, MB_OK, PostMessageW};

use crate::settings::Config;

use super::{App, ID_HOTKEY_PIN, OVERLAY_RUNNING, WM_OVERLAY_DONE, message_box};

impl App {
    pub fn start_capture(&self) {
        // 互斥：覆盖层已在运行时不再进入（F1 连按与鼠标截图键都会走到这里，
        // 重入等于再抓一份全屏位图、再弹一个覆盖层）
        if OVERLAY_RUNNING.swap(true, Ordering::AcqRel) {
            return;
        }
        crate::perf::mark_capture_start();
        let hinstance = self.hinstance.0 as usize;
        let main_hwnd = self.hwnd.0 as usize; // HWND 非 Send，跨线程传原生值
        let max_height = self.config.longshot_max_height;
        // 截图期间撤销贴图全局热键：F3 在覆盖层内走「贴选区」（PIN-1），
        // 否则 WM_HOTKEY 与覆盖层 WM_KEYDOWN 同时收到 F3 会双重触发（一次贴选区一次贴剪贴板）。
        let _ = unsafe { UnregisterHotKey(Some(self.hwnd), ID_HOTKEY_PIN) };
        // 工具切换键配置 + 默认工具（记住上次使用，M3）
        let opts = crate::overlay::OverlayOpts {
            palette: crate::theme::palette(crate::theme::resolve(self.config.theme)),
            tool_keys: self.config.tool_keys.clone(),
            longshot_key: self.config.hotkey.longshot_key,
            // OCR 键与开关（OCR-1/OCR-3）：关闭时覆盖层不出现 OCR 入口
            ocr_key: self.config.hotkey.ocr_key,
            ocr_enabled: self.config.ocr.enabled,
            confirm_cancel: self.config.confirm_cancel,
            // 贴图键（覆盖层内贴选区，默认 F3；ADR 0019：可改为鼠标键）
            pin_key: self.config.hotkey.pin_key,
            // 再次截图键（issue #118，默认 Shift+F1）：截图期间当前画面作新底图
            recapture_key: self.config.hotkey.recapture_key,
            recapture_modifiers: self.config.hotkey.recapture_modifiers,
            default_tool: crate::settings::last_tool(),
            // 色块颜色（EDT-7，issue #73 后续）：可逐个双击改写
            colors: self.config.swatch_colors,
            // 命名模板与上次保存格式/目录（OUT-2）：保存对话框预填文件名与初始位置
            save: crate::settings::SavePrefs {
                name_style: self.config.name_style,
                last_ext: self.config.last_save_ext.clone(),
                last_dir: self.config.last_save_dir.clone(),
            },
            // 覆盖层界面缩放（CAP-5）：随配置快照传入覆盖层线程
            ui_scale: self.config.ui_scale,
            // 普通截图非贴图编辑（ADR 0028）
            pin_edit: None,
        };
        // 长截图完成后也要弹同一个对话框（「另存为」入口）
        let longshot_save = opts.save.clone();
        let ui_scale = opts.ui_scale;
        let palette = opts.palette;
        std::thread::spawn(move || {
            let hinstance = HINSTANCE(hinstance as *mut _);
            let main_hwnd = HWND(main_hwnd as *mut _);
            match crate::capture::capture_virtual_screen() {
                Ok(screen) => {
                    let result = crate::overlay::run(screen, opts, hinstance, main_hwnd);
                    // M4：工具栏长截图按钮 / 长截图键返回视口选区，进入长截图拼接。
                    if let crate::overlay::OverlayResult::Viewport((x, y, w, h)) = result {
                        crate::longshot::run(x, y, w, h, hinstance, max_height, palette, ui_scale, longshot_save);
                    }
                }
                Err(e) => message_box(None, &format!("抓屏失败：{e}"), MB_OK | MB_ICONERROR),
            }
            OVERLAY_RUNNING.store(false, Ordering::Release);
            // 通知主线程重注册贴图热键（截图已结束）
            let _ = unsafe { PostMessageW(Some(main_hwnd), WM_OVERLAY_DONE, WPARAM(0), LPARAM(0)) };
        });
    }

    /// 覆盖层送来的 OCR 请求（OCR-7）：弹气泡并在后台识别。
    pub fn open_ocr_bubble(&self, hg: windows::Win32::Foundation::HGLOBAL) {
        let language = self.config.ocr.language_tag().map(|t| t.to_string());
        crate::app::bubble::on_request(
            hg,
            crate::app::bubble::Opts {
                language,
                auto_copy: self.config.ocr.auto_copy,
                main_hwnd: self.hwnd,
                palette: crate::theme::palette(crate::theme::resolve(self.config.theme)),
                ui: crate::overlay::ui::Ui::resolve(self.config.ui_scale),
            },
        );
    }

    pub fn not_yet(&self, feature: &str, milestone: &str) {
        let text = format!("{feature}尚未实现（{milestone}）。");
        message_box(Some(self.hwnd), &text, MB_OK | MB_ICONINFORMATION);
    }

    /// 打开设置面板（M3）：主线程模态运行，防重入。
    pub fn open_settings(&mut self) {
        if self.settings_open {
            return;
        }
        self.settings_open = true;
        crate::settings::panel::run(self);
        self.settings_open = false;
    }

    /// 应用设置面板的工作副本：写回配置、保存、重注册热键。
    pub fn apply_panel_config(&mut self, cfg: &Config) {
        // 保存格式与目录是运行时记忆（面板里没有对应设置项），不能被面板的旧副本覆盖（OUT-2）
        let last_save_ext = self.config.last_save_ext.clone();
        let last_save_dir = self.config.last_save_dir.clone();
        self.config = cfg.clone();
        self.config.last_save_ext = last_save_ext;
        self.config.last_save_dir = last_save_dir;
        self.config.save();
        self.register_hotkeys();
        // 贴图内边框快捷键（M2b）：立即下发给贴图窗口（含已存在的）
        crate::pin::set_border_key(cfg.hotkey.border_key, cfg.hotkey.border_modifiers);
    }

    /// 贴图边框开关（贴图窗口内快捷键 → 本消息触发）：
    /// 切全部贴图的边框显示，并把新状态写入默认配置（后续新贴图跟随）。
    pub fn toggle_pin_border(&self) {
        let next = crate::pin::toggle_border();
        if let Some(next) = next {
            let mut cfg = self.config.clone();
            cfg.pin_border = next;
            cfg.save();
        }
    }

    /// 启动后静默检查更新（SYS-7）：失败不打扰用户（国内网络下加速站挂掉是常态）。
    pub fn check_update_silently(&mut self) {
        let skipped = self.config.skipped_version.clone();
        crate::update::check::check_in_background(skipped);
    }
}
