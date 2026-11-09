// SPDX-License-Identifier: GPL-3.0-only
//! 应用壳（SYS-1/2/3/4/6）：单实例、隐藏消息窗口、托盘、热键、自启、主题变更。
//! 拆分：`run` 入口与 `App` 状态见本文件，消息处理器在 `actions`，窗口过程在 `wndproc`，
//! 托盘在 `tray`，热键在 `hotkey`，自启在 `autostart`。

mod actions;
pub mod autostart;
mod bubble;
pub mod filedlg;
pub(crate) mod hotkey;
pub(crate) mod mouse;
pub mod open;
mod pin_edit;
pub mod popmenu;
mod tray;
mod wndproc;

use std::sync::atomic::AtomicBool;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::settings::Config;
use wndproc::create_message_window;

/// 消息窗口类名（`FindWindowW` 唤起已有实例时用到）。
pub const WINDOW_CLASS: PCWSTR = w!("airsnip.hidden");
const MUTEX_NAME: PCWSTR = w!("Local\\airsnip.single.instance");
const WM_TRAY: u32 = WM_APP + 1;
const WM_ACTIVATE: u32 = WM_APP + 2; // 单实例唤起已有实例
/// 覆盖层线程 → 主窗口：贴图载荷（GlobalAlloc: u32 w + u32 h + RGBA）（PIN-1）。
pub const WM_PIN_FROM_EDITOR: u32 = WM_APP + 3;
/// 覆盖层线程 → 主窗口：截图已结束，重注册被撤销的贴图热键（PIN-1 防 F3 双触发）。
pub const WM_OVERLAY_DONE: u32 = WM_APP + 4;
/// 贴图窗口 → 主窗口：贴图内边框快捷键已切换（更新默认配置，M2b）。
pub const WM_PIN_BORDER_CHANGED: u32 = WM_APP + 5;
/// 覆盖层线程 → 主窗口：OCR 请求（GlobalAlloc: u32 w + u32 h + i32 x + i32 y + RGBA，OCR-7）。
pub const WM_OCR_REQUEST: u32 = WM_APP + 6;
/// OCR 卡片 → 主窗口：用户切了「自动复制」开关（wparam = 0 关 / 1 开）。
pub const WM_OCR_AUTOCOPY_CHANGED: u32 = WM_APP + 7;
/// 贴图窗口 → 主窗口：请求进入标注编辑（wparam = 贴图句柄，ADR 0028）。
pub const WM_PIN_EDIT_REQUEST: u32 = WM_APP + 8;
/// 覆盖层线程 → 主窗口：贴图编辑完成 / 取消（wparam = 载荷 HGLOBAL 或 0，lparam = 目标贴图句柄，ADR 0028）。
pub const WM_PIN_EDIT_DONE: u32 = WM_APP + 9;
/// 贴图窗口 → 主窗口：请求「另存为」（wparam = 贴图句柄，OUT-2）。
pub const WM_PIN_SAVE_REQUEST: u32 = WM_APP + 10;

/// 截图覆盖层是否在运行（跨线程；截图期间撤销全局贴图热键）。
/// 鼠标热键（`mouse`）共用：截图期间同样不响应鼠标贴图键。
static OVERLAY_RUNNING: AtomicBool = AtomicBool::new(false);

/// 覆盖层保存时选定的格式扩展名与目录（OUT-2）：见 `output::recent`。

pub const ID_TRAY: u32 = 1;
pub const ID_HOTKEY_SHOT: i32 = 101;
pub const ID_HOTKEY_PIN: i32 = 102;

/// 托盘菜单项 id（供 `tray` 子模块使用）。
pub const IDM_SHOT: usize = 40001;
pub const IDM_PIN: usize = 40002;
pub const IDM_SETTINGS: usize = 40003;
pub const IDM_EXIT: usize = 40004;

/// 启动后延迟检查更新的定时器 id（SYS-7）：一次性触发后立即 KillTimer，不常驻轮询。
pub const TIMER_UPDATE_CHECK: usize = 0x7B01;

/// 启动后延迟多久才检查更新（毫秒）：错开热键/托盘初始化，不影响启动耗时。
const UPDATE_CHECK_DELAY_MS: u32 = 3_000;

/// 应用运行时状态。
pub struct App {
    pub config: Config,
    pub hwnd: HWND,
    pub hinstance: HINSTANCE,
    /// 设置面板是否已打开（防托盘菜单重入，SYS-3）。
    pub settings_open: bool,
    /// 托盘图标补加重试次数（explorer 未就绪时用定时器重试，SYS-1）。
    pub tray_retry: u8,
    /// 正在被编辑的贴图（ADR 0028）：完成后写回；取消 / 复制 / 保存后恢复显示。
    pub pin_edit_target: Option<HWND>,
}

/// 入口：单实例检查 → 创建消息窗口 → 托盘 / 热键 → 消息循环。
/// 返回进程退出码。
pub fn run() -> i32 {
    if !acquire_single_instance() {
        // 已有实例在运行，已向其发送唤起消息，本进程直接退出。
        return 0;
    }

    let config = Config::load();
    let hinstance: HINSTANCE = unsafe { GetModuleHandleW(None) }.unwrap_or_default().into();

    let hwnd = match create_message_window(hinstance) {
        Ok(hwnd) => hwnd,
        Err(msg) => {
            message_box(None, &msg, MB_OK | MB_ICONERROR);
            return 1;
        }
    };

    let mut app = Box::new(App {
        config,
        hwnd,
        hinstance,
        settings_open: false,
        tray_retry: 0,
        pin_edit_target: None,
    });
    // 先存窗口用户数据：wndproc 通过它访问 App，而 ensure_tray / register_hotkeys
    // 都可能触发消息（如 explorer 刚就绪时的 TaskbarCreated），早于此刻会丢图标。
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut *app as *mut App as isize);
    }
    app.ensure_tray();
    app.maybe_warn_no_modifier_clash();
    app.register_hotkeys();
    // 鼠标键热键（SYS-2，ADR 0019）：RegisterHotKey 不支持，改订阅 Raw Input
    mouse::install(hwnd, mouse::wanted(&app.config.hotkey));
    // 更新检查的后台线程要通知主窗口弹气泡（SYS-7）
    crate::update::notify::register_main(hwnd);
    // 贴图内边框快捷键（M2b，非全局热键：仅贴图窗口聚焦时生效）
    crate::pin::set_border_key(app.config.hotkey.border_key, app.config.hotkey.border_modifiers);
    // 启动后延迟检查更新（SYS-7）：一次性定时器，不引入常驻轮询（其他性能红线不变）
    if app.config.auto_check_update {
        unsafe {
            SetTimer(Some(hwnd), TIMER_UPDATE_CHECK, UPDATE_CHECK_DELAY_MS, None);
        }
    }

    unsafe {
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 {
                break; // WM_QUIT
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
    // 窗口随 WM_CLOSE 销毁（WM_DESTROY → PostQuitMessage 退出循环），这时
    // 不能再经 GetWindowLongPtrW 取回 Box（句柄已无效，返回值不可靠），
    // 直接用局部变量恢复 Box 并移除托盘图标。
    app.remove_tray();
    0
}

/// 单实例：命名 Mutex 已存在说明已有实例，唤起后返回 false。
fn acquire_single_instance() -> bool {
    match unsafe { CreateMutexW(None, false, MUTEX_NAME) } {
        Ok(mutex) => {
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                // 句柄无 Drop（Copy 类型），let _ 保持句柄打开直到进程退出，
                // 确保 mutex 对象存活，单实例有效。
                let _ = mutex;
                let hwnd = unsafe { FindWindowW(WINDOW_CLASS, None) }.unwrap_or_default();
                let _ = unsafe { PostMessageW(Some(hwnd), WM_ACTIVATE, WPARAM(0), LPARAM(0)) };
                false
            } else {
                let _ = mutex; // 保持到进程退出
                true
            }
        }
        Err(_) => true, // 创建失败不阻塞运行，退化为无单实例保护
    }
}

/// 程序主图标资源（app.rc 中的 ID=1）。
pub fn icon_resource() -> PCWSTR {
    PCWSTR::from_raw(1 as *const u16)
}

/// 弹消息框（UTF-8 → UTF-16 缓冲随调用存活，避免临时指针悬垂）。
/// `owner` 为 None 时使用当前活动窗口；`flags` 为 MB_* 组合。
pub fn message_box(owner: Option<HWND>, text: &str, flags: MESSAGEBOX_STYLE) {
    let ws: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe { MessageBoxW(owner, PCWSTR(ws.as_ptr()), w!("AirSnip"), flags) };
}

/// 将字符串写入固定长度 u16 数组（用于 NOTIFYICONDATAW.szTip 等）。
pub fn wide_array<const N: usize>(s: &str) -> [u16; N] {
    let mut arr = [0u16; N];
    let mut i = 0;
    for u in s.encode_utf16().take(N - 1) {
        arr[i] = u;
        i += 1;
    }
    arr
}
