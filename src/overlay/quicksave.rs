// SPDX-License-Identifier: GPL-3.0-only
//! 快速保存（OUT-2 / ADR 0041）：覆盖层内按模板路径直接落盘（不弹对话框）。
//!
//! 落盘（PNG/JPEG/WebP 编码 + 写盘）在后台线程完成，结果经「静态槽 + `PostMessage`」
//! 回到覆盖层线程——编码一张全屏图要几十毫秒，同步做会卡住交互线程（交互 60 FPS 红线）。
//! 成功 → 弹轻提示（`toast`，可点定位文件），失败 → 弹错误框并保留覆盖层（可重试）。
//!
//! 与 `on_save`（另存为对话框）共用同一份「烘焙选区」和模板；区别只在于路径从哪来。

use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, PostMessageW, WM_APP};

use super::Overlay;
use crate::output;
use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT};

/// 自定义消息：后台线程保存完毕（wparam/lparam 未用，结果在静态槽里）。
/// 占位要避开 `wndproc` 已用的 `WM_APP + 1`（异步焦点）与 `WM_APP + 4`（吸附回包），
/// 也不要复用 `app/mod.rs` 里标注「已退役、编号不复用」的 `+6` / `+7`。
pub(super) const WM_QUICK_SAVED: u32 = WM_APP + 0x30;

/// 后台线程的保存结果。
struct Reply {
    result: Result<std::path::PathBuf, String>,
}

static REPLY: Mutex<Option<Reply>> = Mutex::new(None);

/// 保存 / 快速保存的运行时状态（OUT-2 / ADR 0041）：键位、配置、在途标记与轻提示。
///
/// 收成一个字段而不散在 `Overlay` 里：这八项只有保存链路读写，聚在一起才看得出
/// 「一次保存需要哪些东西」，也让 `overlay/mod.rs` 保持在一屏内。
pub(super) struct QuickSave {
    /// 另存为（弹对话框）键与修饰键。
    pub(super) save_key: u32,
    pub(super) save_modifiers: u32,
    /// 快速保存键与修饰键。
    pub(super) key: u32,
    pub(super) modifiers: u32,
    /// 配置快照（模板 / 格式 / 同名策略 / 通知与剪贴板开关）。
    pub(super) config: crate::settings::QuickSaveConfig,
    /// 在途（防止连按重复落盘）。
    pub(super) pending: bool,
    /// 刚写出的文件路径（点提示定位时用）。
    pub(super) saved_path: Option<std::path::PathBuf>,
    /// 保存成功的轻提示（`toast`）。
    pub(super) toast: Option<super::toast::Toast>,
}

/// `AIRSNIP_DEBUG=1` 时打一行（与仓库其它调试输出同一口径）。
fn debug(msg: &str) {
    if crate::app::mouse::debug_enabled() {
        eprintln!("[airsnip] quicksave: {msg}");
    }
}

/// 丢弃上一轮没人取走的结果：覆盖层被关掉时回包消息会被丢掉，静态槽会留下一条
/// 陈旧记录；下一张截图开始时清掉，避免与本次的保存混在一起。
pub(super) fn discard_reply() {
    if let Ok(mut g) = REPLY.lock() {
        *g = None;
    }
}

/// 组合键是否匹配：键相同、且**当前按住的修饰键集合与配置完全一致**
/// （纯函数，便于单测）。`Ctrl+S` 不会误命中 `Alt+S`，`Shift+S` 也不会误命中裸 `S`。
pub(super) fn combo_match(key: u32, mods: u32, vk: u32, ctrl: bool, shift: bool, alt: bool, win: bool) -> bool {
    if key == crate::settings::UNBOUND || vk != key {
        return false;
    }
    let want_ctrl = mods & MOD_CTRL_BIT != 0;
    let want_shift = mods & MOD_SHIFT_BIT != 0;
    let want_alt = mods & MOD_ALT_BIT != 0;
    let want_win = mods & MOD_WIN_BIT != 0;
    want_ctrl == ctrl && want_shift == shift && want_alt == alt && want_win == win
}

impl Overlay {
    /// 快速保存（OUT-2 / ADR 0041）：烘焙当前选区 → 后台按模板落盘。
    /// 贴图编辑模式（ADR 0028）不生效（与长截图 / 快速选区键口径一致）；没有选区时不响应。
    pub(super) fn on_quick_save(&mut self) {
        if self.locked_selection || self.quick.pending || self.quick.toast.is_some() {
            return;
        }
        let Some(sel) = self.selection else {
            return;
        };
        let Some(cropped) = self.baked_selection(sel) else {
            return;
        };
        self.quick.pending = true;
        let cfg = self.quick.config.clone();
        let also_copy = cfg.also_copy;
        let hwnd = self.hwnd.0 as usize;
        std::thread::spawn(move || {
            let t = output::quick::TimeParts::now();
            let result = output::quick::save_quickly(&cropped, &cfg, &t);
            // 剪贴板开关（设置面板，默认关）：存盘成功才复制，失败时不让剪贴板被半成品污染。
            // 复制失败不能静默吞掉——用户勾了「同时复制」却只拿到文件，得能从日志看出原因。
            if also_copy
                && result.is_ok()
                && let Err(e) = output::copy_to_clipboard(&cropped)
            {
                debug(&format!("also_copy 失败：{e}")); // i18n:keep 内部日志（不进语言表）
            }
            if let Ok(mut g) = REPLY.lock() {
                *g = Some(Reply { result });
            }
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_QUICK_SAVED, WPARAM(0), LPARAM(0));
            }
        });
    }

    /// 后台线程回包：成功弹提示（或直接收尾），失败提示错误并保留覆盖层可重试。
    pub(super) fn on_quick_saved(&mut self) {
        self.quick.pending = false;
        let Some(reply) = REPLY.lock().ok().and_then(|mut g| g.take()) else {
            return;
        };
        match reply.result {
            Err(e) => crate::app::message_box(
                None,
                &crate::i18n::fill(crate::i18n::OverlayKey::QuickSaveFailed.text(), &[("e", &e)]),
                MB_OK | MB_ICONERROR,
            ),
            Ok(path) => {
                self.quick.saved_path = Some(path.clone());
                if self.quick.config.notify {
                    self.show_saved_toast(&path);
                } else {
                    self.cancelled = false;
                    self.close();
                }
            }
        }
    }

    /// 收尾：收起提示；`reveal` 为真时先在资源管理器里定位刚存的文件，再关闭覆盖层。
    pub(super) fn finish_quick(&mut self, reveal: bool) {
        self.clear_toast();
        if reveal && let Some(p) = self.quick.saved_path.clone() {
            output::quick::reveal_in_explorer(&p);
        }
        self.cancelled = false;
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认键位：Ctrl+S 只命中保存，Alt+S 只命中快速保存，裸 S 两者都不命中。
    #[test]
    fn combo_match_requires_exact_modifiers() {
        let cfg = crate::settings::Config::default();
        let (save_k, save_m) = (cfg.hotkey.save_key, cfg.hotkey.save_modifiers);
        let (quick_k, quick_m) = (cfg.hotkey.quick_save_key, cfg.hotkey.quick_save_modifiers);

        // 保存：Ctrl+S
        assert!(combo_match(save_k, save_m, 0x53, true, false, false, false));
        // 快速保存：Alt+S
        assert!(combo_match(quick_k, quick_m, 0x53, false, false, true, false));

        // 交叉不命中
        assert!(!combo_match(save_k, save_m, 0x53, false, false, true, false));
        assert!(!combo_match(quick_k, quick_m, 0x53, true, false, false, false));
        // 裸 S 不命中任何保存组合（长截图键照旧）
        assert!(!combo_match(save_k, save_m, 0x53, false, false, false, false));
        assert!(!combo_match(quick_k, quick_m, 0x53, false, false, false, false));
        // 多按一个修饰键也不命中（Ctrl+Alt+S 不等于 Ctrl+S）
        assert!(!combo_match(save_k, save_m, 0x53, true, false, true, false));
        // 别的键不命中
        assert!(!combo_match(save_k, save_m, 0x43, true, false, false, false));
        // 未绑定的键不命中
        assert!(!combo_match(
            crate::settings::UNBOUND,
            0,
            crate::settings::UNBOUND,
            false,
            false,
            false,
            false
        ));
    }

    /// 无修饰键的绑定只在完全没按修饰键时命中（避免 `Shift+Q` 抢走裸 `Q`）。
    #[test]
    fn plain_binding_needs_no_modifiers() {
        assert!(combo_match(0x51, 0, 0x51, false, false, false, false));
        assert!(!combo_match(0x51, 0, 0x51, false, true, false, false));
        assert!(!combo_match(0x51, 0, 0x51, true, false, false, false));
    }

    /// 四个修饰位单独生效（Ctrl / Shift / Alt / Win 都能表达）。
    #[test]
    fn every_modifier_bit_is_honored() {
        for (bit, ctrl, shift, alt, win) in [
            (MOD_CTRL_BIT, true, false, false, false),
            (MOD_SHIFT_BIT, false, true, false, false),
            (MOD_ALT_BIT, false, false, true, false),
            (MOD_WIN_BIT, false, false, false, true),
        ] {
            assert!(combo_match(0x51, bit, 0x51, ctrl, shift, alt, win), "{bit:#x} 应命中");
            assert!(
                !combo_match(0x51, bit, 0x51, false, false, false, false),
                "{bit:#x} 不应裸命中"
            );
        }
    }
}
