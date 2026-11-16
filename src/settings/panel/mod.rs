// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板（M3）：自绘设置窗口（tiny-skia 绘制 + DirectWrite 文字）。
//! 布局为左侧分类导航 + 右侧内容区；主线程模态运行，交互只改工作副本 `config`，
//! 关闭时经 `App::apply_panel_config` 统一写回、保存并重注册热键；
//! 主题与开机自启即时生效（面板自身重绘 + schtasks 调用）。
//! 窗口尺寸固定为横向矩形（宽 > 高），切换分类页不改变窗口大小。

mod controls;
mod copyinfo;
mod draw;
mod editkeys;
mod interact;
mod keycap;
mod link;
// 离屏渲染测试（ADR 0041）：只在测试构建里编译
#[cfg(test)]
mod render_tests;
mod reward;
mod savepage;
mod textbox;
mod theme_watch;
mod toolbar_page;
mod update;
mod widgets;
mod window;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, DeleteDC, DeleteObject, EndPaint, HBITMAP, HDC, InvalidateRect, PAINTSTRUCT, UpdateWindow,
};
use windows::Win32::UI::Input::Ime::HIMC;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::app::{App, message_box};
use crate::theme::{self, Palette};

use controls::{RowId, Section};

const WINDOW_CLASS: PCWSTR = w!("airsnip.settings");

/// 面板运行时状态。
pub struct Panel {
    pub palette: &'static Palette,
    /// 工作副本（关闭时写回）。
    pub config: crate::settings::Config,
    /// 界面缩放系数（与截图覆盖层同一套，ADR 0016）：改设置时立即重算并重排窗口，
    /// 面板自身当场就能反映新倍率。
    pub scale: f32,
    /// 内容区悬停行下标。
    pub hover: Option<usize>,
    /// 导航栏悬停分类。
    pub nav_hover: Option<Section>,
    /// 当前分类页。
    pub section: Section,
    /// 正在捕获的热键。
    pub capturing: Option<RowId>,
    /// 面板窗口句柄（IME 关联操作用）。
    pub(super) hwnd: HWND,
    /// 主程序窗口句柄（更新完成后需要通知它退出，SYS-7）。
    pub(super) main_hwnd: HWND,
    /// 捕获期间的 IME 上下文（禁输入法，防止按键被 IME 改写为 0xE5 之类的键码）。
    pub(super) ime_old: HIMC,
    /// 开机自启真实状态（任务计划是否存在，SYS-4）。
    pub autostart_on: bool,
    /// 配置有改动、待立即应用（热键/主题即时生效，不等面板关闭）。
    pub(super) config_dirty: bool,
    /// 「我要反馈」的通知（SYS-8）：文案；存在期间面板底部显示一条带「打开反馈页」按钮的提示。
    pub(super) toast: Option<String>,
    /// 路径模板的输入态（OUT-2 / ADR 0041）：`Some` 时面板吃掉字符与方向键。
    pub(super) editing: Option<textbox::Editing>,
    /// 「变量说明」浮层是否展开（OUT-2 / ADR 0041）。
    pub(super) var_help: bool,
    /// 快捷键页当前的子页签（ADR 0041）：纯视图状态，不落配置。
    pub(super) hotkey_group: controls::KeyGroup,
    /// 子页签悬停项（与行悬停分开：页签不是行）。
    pub(super) tab_hover: Option<controls::KeyGroup>,
    /// 展开中的下拉列表（OUT-2 / ADR 0041）：`格式` / `如果文件存在`。
    /// 有值时点击**只**用于选选项或关闭，绝不穿透到其它控件。
    pub(super) open_select: Option<controls::RowId>,
    /// 下拉列表的悬停项下标。
    pub(super) select_hover: Option<usize>,
    /// 预览用的时间样本（OUT-2 / ADR 0041）：**只在打开面板时采样一次**，不随重绘重算——
    /// 鼠标一动就重绘，若每次都取新时间，`$ss$` / `$ts$` 会让预览文字乱跳。
    /// 代价：改完模板后 `$ts$` 的预览仍是打开面板那一刻的值（真实保存时才取当前时间）；
    /// 预览的用途是「看模板拼出来长什么样」，不需要逐秒精确。
    pub(super) preview_time: crate::output::quick::TimeParts,
    /// 上屏缓冲（DIB section）。
    pub(super) dib_bmp: HBITMAP,
    pub(super) mem_dc: HDC,
    pub(super) dib_bits: *mut u8,
}

impl Panel {
    /// 当前页的布局（带快捷键页当前子页签）：绘制、命中、高度全走这一份，
    /// 避免「画的是 A 组、点的是 B 组」这种错位。
    pub(super) fn layout(&self) -> controls::Layout {
        controls::layout_with(self.section, self.scale, self.hotkey_group)
    }

    /// 测试用：不依赖窗口的 `Panel`（离屏渲染与控件逻辑的用例共用）。
    #[cfg(test)]
    pub(super) fn for_test(section: Section, theme: theme::Theme) -> Panel {
        Panel {
            palette: theme::palette(theme),
            config: crate::settings::Config::default(),
            scale: 1.0,
            hover: None,
            nav_hover: None,
            section,
            capturing: None,
            hwnd: HWND::default(),
            main_hwnd: HWND::default(),
            ime_old: HIMC(std::ptr::null_mut()),
            autostart_on: false,
            config_dirty: false,
            toast: None,
            editing: None,
            var_help: false,
            hotkey_group: controls::KeyGroup::Capture,
            tab_hover: None,
            open_select: None,
            select_hover: None,
            preview_time: crate::output::quick::TimeParts::now(),
            dib_bmp: Default::default(),
            mem_dc: Default::default(),
            dib_bits: std::ptr::null_mut(),
        }
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        unsafe {
            if !self.dib_bmp.0.is_null() {
                let _ = DeleteObject(self.dib_bmp.into());
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
    }
}

/// 打开设置面板（主线程模态，阻塞到窗口关闭）。
pub fn run(app: &mut App) {
    let hinstance = app.hinstance;
    let mut panel = Panel {
        palette: theme::palette(theme::resolve(app.config.theme)),
        config: app.config.clone(),
        scale: 1.0,
        hover: None,
        nav_hover: None,
        section: Section::General,
        capturing: None,
        hwnd: HWND::default(),
        main_hwnd: app.hwnd,
        ime_old: HIMC(std::ptr::null_mut()),
        // 自启开关显示任务计划真实状态，而非配置猜测（SYS-4）
        autostart_on: crate::app::autostart::autostart_enabled(),
        config_dirty: false,
        toast: None,
        editing: None,
        var_help: false,
        hotkey_group: controls::KeyGroup::Capture,
        tab_hover: None,
        open_select: None,
        select_hover: None,
        preview_time: crate::output::quick::TimeParts::now(),
        dib_bmp: Default::default(),
        mem_dc: Default::default(),
        dib_bits: std::ptr::null_mut(),
    };

    let hwnd = match window::create_window(hinstance, &mut panel) {
        Ok(h) => h,
        Err(e) => {
            message_box(None, &e, MB_OK | MB_ICONERROR);
            return;
        }
    };
    panel.hwnd = hwnd;
    // 后台线程（更新检查/下载）通过这个句柄通知面板刷新关于页（SYS-7）
    crate::update::notify::register(hwnd);
    // 系统深/浅色切换时刷新面板配色（SYS-6）：注册表通知线程，drop 即停
    let _theme_watch = theme_watch::ThemeWatch::spawn(hwnd);
    // 标题栏由 DWM 绘制：创建后先按当前主题着色（切换主题时在 `set_theme` 里更新）
    window::apply_dark_titlebar(hwnd, theme::resolve(panel.config.theme) == theme::Theme::Dark);
    let raw = Box::into_raw(Box::new(panel));
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = UpdateWindow(hwnd);
    }

    // 面板消息循环：WM_DESTROY 时 wndproc 调 PostQuitMessage(0) 结束。
    // 每轮检查 config_dirty：热键/主题改动立即写回应用（不等关闭），
    // 避免面板开着时旧热键仍生效、新热键无反应的困惑。
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 != 0 {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
            let p = &mut *raw;
            if p.config_dirty {
                p.config_dirty = false;
                let cfg = p.config.clone();
                app.apply_panel_config(&cfg);
            }
        }
    }

    let panel = unsafe { Box::from_raw(raw) };
    // 面板即将销毁：不再接收更新状态通知（否则消息会投到已销毁窗口）
    crate::update::notify::unregister();
    // 兜底：恢复输入法 + 最后同步一次配置（如改完直接关窗）
    let mut panel = panel;
    panel.restore_ime();
    // 关面板时正在编辑路径模板（OUT-2 / ADR 0041）：先把输入内容写回工作副本，
    // 否则用户敲完直接按 Alt+F4 关窗会丢掉刚改的模板
    textbox::commit(&mut panel);
    app.apply_panel_config(&panel.config);
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

/// 把已打开的面板窗口还原并置前（托盘再点一次 / 右键菜单「设置」）。
///
/// 面板是主线程上的模态泵（`run` 阻塞到窗口关闭），已打开时再调 `run` 会多开一个窗口，
/// 所以只能激活现有的那个。调用方与面板同线程（主窗口过程里调），这里直接操作窗口即可。
pub fn focus_existing() {
    unsafe {
        let Ok(hwnd) = FindWindowW(WINDOW_CLASS, None) else {
            return;
        };
        // 最小化时要先还原，否则 SetForegroundWindow 只改任务栏状态、窗口还是收着的
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

/// 创建设置窗口：初始按 scale=1 粗建，创建后按真实 DPI 校正。
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // 窗口销毁（DestroyWindow 同步发送）：必须 PostQuitMessage 结束面板消息循环，
    // 否则面板线程/主线程永久阻塞在 GetMessageW（见覆盖层同款修复，M3）。
    if msg == WM_DESTROY {
        unsafe {
            let _ = PostQuitMessage(0);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
    }
    let Some(panel) = window::app_from(hwnd) else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    // 路径模板输入态（OUT-2 / ADR 0041）：字符 / 编辑键 / IME 消息全给输入框，
    // 命中即重绘并吞掉（不再进热键捕获或默认处理）
    if editkeys::on_message(panel, msg, wparam.0, lparam.0) {
        window::repaint_full(hwnd, panel);
        return LRESULT(0);
    }
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            unsafe {
                let _ = BeginPaint(hwnd, &mut ps);
            }
            window::repaint_full(hwnd, panel);
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        // 「我要反馈」的通知到点：收起（不打开页面，SYS-8，一次性定时器）
        WM_TIMER if wparam.0 == copyinfo::TIMER_ID => {
            copyinfo::on_timer(hwnd);
            LRESULT(0)
        }
        // 后台更新线程的状态变更（SYS-7）：只重绘，状态从 `update::state` 现读
        crate::update::notify::WM_UPDATE_STATE => {
            window::repaint_full(hwnd, panel);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            let (nav_hover, hover) = panel.hover_at(x, y);
            // 下拉列表展开时行悬停无意义（改跟选项悬停，否则下层行会跟着亮）。
            // **比较也要用过滤后的值**：否则面板里存的是 `None`、每次比到 `Some(..)` 都算「变了」，
            // 鼠标在行上滑一次就全量重绘一次。
            let hover = if panel.open_select.is_some() { None } else { hover };
            let tab_hover = panel.hit_tab(x, y);
            let select_hover = if panel.open_select.is_some() {
                savepage::list_hover_at(panel, &panel.layout(), x, y)
            } else {
                None
            };
            if nav_hover != panel.nav_hover
                || hover != panel.hover
                || tab_hover != panel.tab_hover
                || select_hover != panel.select_hover
            {
                panel.nav_hover = nav_hover;
                panel.hover = hover;
                panel.tab_hover = tab_hover;
                panel.select_hover = select_hover;
                window::repaint_full(hwnd, panel);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            panel.on_click(x, y);
            panel.fit_window(hwnd);
            window::repaint_full(hwnd, panel);
            LRESULT(0)
        }
        // PrintScreen（VK_SNAPSHOT）系统只派发松键消息、不派发按下消息，
        // 只监听 WM_KEYDOWN 会导致「点键帽后按 PrtScn 无反应」（#67）
        WM_SYSKEYUP | WM_KEYUP if (wparam.0 as u32) == VK_SNAPSHOT.0 as u32 && panel.capturing.is_some() => {
            panel.on_key(VK_SNAPSHOT.0 as u32);
            if panel.capturing.is_none() {
                window::repaint_full(hwnd, panel);
            }
            LRESULT(0)
        }
        // 鼠标键也能当热键（ADR 0019）：仅在捕获态接收，非捕获态交默认处理
        WM_MBUTTONDOWN | WM_XBUTTONDOWN if panel.capturing.is_some() => {
            if let Some(vk) = crate::app::mouse::message_vk(msg, wparam.0) {
                panel.on_key(vk);
                window::repaint_full(hwnd, panel);
            }
            LRESULT(1) // XBUTTON 已处理，TRUE 阻止继续分发
        }
        WM_SYSKEYDOWN | WM_KEYDOWN => {
            // 下拉列表展开时：Esc 收起（其余键不处理，交默认）
            if panel.open_select.is_some() && (wparam.0 as u32) == 0x1B {
                panel.open_select = None;
                panel.select_hover = None;
                window::repaint_full(hwnd, panel);
                return LRESULT(0);
            }
            // WM_SYSKEYDOWN：Alt 组合键走系统键路径，同样进入热键捕获（否则 Alt+A 捕获不到）
            if panel.capturing.is_some() {
                panel.on_key(wparam.0 as u32);
                // 捕获结束（键已记录 / Esc 取消）时刷新按钮显示：从等待态切回正常文案
                if panel.capturing.is_none() {
                    window::repaint_full(hwnd, panel);
                }
                // 捕获态吞掉按键，防止触发系统菜单或其它默认行为
                LRESULT(0)
            } else {
                // 非捕获态交给默认处理（Alt+F4 关闭、Tab 切换焦点等不能吞掉）
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
        }
        // 系统深/浅色切换（SYS-6）：「跟随系统」时重解析调色板并重绘。
        // 管理员进程可能收不到用户级进程的广播（UIPI），故另有注册表监听保底。
        WM_SETTINGCHANGE => {
            if crate::theme::is_theme_change(lparam.0 as usize) && panel.refresh_system_theme() {
                window::repaint_full(hwnd, panel);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        theme_watch::WM_THEME_CHANGED => {
            if panel.refresh_system_theme() {
                window::repaint_full(hwnd, panel);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
