// SPDX-License-Identifier: GPL-3.0-only
//! UI Automation 探针（issue #62 二级 / 控件级捕捉的前期实测）。
//!
//! 目的：在动手实现「控件捕获」之前，先把 UIA 在这台机器上的真实表现量出来——
//! 可用性（哪些程序能拿到子元素）、耗时（鼠标连续移动跟不跟得上）、坐标口径
//! （UIA 给的是不是物理像素）、以及目标进程无响应时到底会阻塞多久。
//!
//! 用法（建议先 `chcp 65001`，否则中文窗口标题会乱码）：
//! ```text
//! cargo run --release --example uia-probe -- scan
//! cargo run --release --example uia-probe -- follow [秒数，默认 30]
//! ```
//!
//! - `scan`：遍历所有可见顶层窗口，按 DWM 扩展边框在中心 + 3×3 网格打点，输出每个窗口的
//!   最深元素、祖先链深度、查询耗时与坐标偏差，末尾给出四条门槛的判定。
//! - `follow`：每 200ms 打印光标下元素与祖先链（`Esc` 退出），用来量「鼠标连续移动」的真实开销。
//!
//! 四条门槛（跑完把结果记进 `docs/uia-probe.md`）：
//! 1. `ElementFromPointBuildCache` 单次 p95 ≤ 30ms（交互 60fps 下不超过 2 帧）；
//! 2. 祖先链 5 级累加 p95 ≤ 80ms（滚轮爬层级不能一顿一顿）；
//! 3. 窗口级元素的矩形与 DWM 扩展边框误差 0px（UIA 与项目「统一物理像素」一致）；
//! 4. ≥ 5 个常用程序能拿到比整窗更小的元素（否则这功能只是把整窗吸附重做一遍）。
//!
//! 另需实测：目标进程无响应时阻塞多久（配合 `examples/uia-hang.rs`，见其文档）。
//!
//! 注意：这是**一次性探针**，实测数据已记入 `docs/uia-probe.md`，不用于交付代码；
//! 因此不遵守「单文件 ≤300 行」的源码约定（它的价值在于把每一轮实测原样保留下来）。

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, AutomationElementMode_None, CUIAutomation, IUIAutomation, IUIAutomationCacheRequest,
    IUIAutomationElement, IUIAutomationTreeWalker, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId, UIA_ProcessIdPropertyId,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetCursorPos, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible,
};
use windows::core::BOOL;

/// 祖先链最多上溯几级（窗口级通常在 3–5 级以内；桌面根元素不再往上是靠重复矩形判定）。
const MAX_LEVELS: usize = 6;
/// 单次查询超过它就记作「疑似目标进程无响应」。
const HUNG_MS: f64 = 500.0;
/// `scan` 每窗口的采样点：中心 + 3×3 网格（相对边框的比例）。
const SAMPLES: [(f64, f64); 10] = [
    (0.5, 0.5),
    (0.25, 0.25),
    (0.5, 0.25),
    (0.75, 0.25),
    (0.25, 0.5),
    (0.75, 0.5),
    (0.25, 0.75),
    (0.5, 0.75),
    (0.75, 0.75),
    (0.5, 0.35),
];

/// 一个顶层窗口：(句柄, DWM 扩展边框, 类名)。Z 序判定用它。
type TopWindow = (HWND, (i32, i32, i32, i32), String);

/// 从 UIA 缓存里读出来的元素信息。
#[derive(Clone, Debug)]
struct Element {
    rect: (i32, i32, i32, i32),
    control: i32,
    name: String,
    class: String,
    pid: i32,
    hwnd: isize,
}

impl Element {
    fn size(&self) -> (i32, i32) {
        (self.rect.2, self.rect.3)
    }
}

/// 一次「点 → 元素 + 祖先链」的查询结果。
struct Query {
    deepest: Option<Element>,
    chain: Vec<Element>,
    /// 单次 `ElementFromPointBuildCache` 的耗时。
    point_ms: f64,
    /// 上溯整条祖先链的累加耗时。
    walk_ms: f64,
    error: Option<String>,
    /// 上溯失败的报错（`None` = 一直走到了根或重复矩形）。
    walk_error: Option<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("scan");
    if cmd == "-h" || cmd == "--help" {
        println!("用法：uia-probe [scan [--verbose] | follow [秒数] | point x,y]");
        return;
    }
    println!(
        "探针进程：提权={}（AirSnip 本体是 requireAdministrator，所以这里的提权结果才是真实运行环境）",
        match is_elevated() {
            Some(true) => "是",
            Some(false) => "否",
            None => "未知",
        }
    );
    let rt = Runtime::new();
    let (Ok(uia), Ok(cache_walk), Ok(walker)) = (&rt.uia, &rt.cache_walk, &rt.walker) else {
        eprintln!("UIA 初始化失败：{}", rt.error.as_deref().unwrap_or("未知错误"));
        std::process::exit(1);
    };
    if cmd == "point" {
        // 固定坐标单次查询：普通权限 / 提权两次跑同一个点，结果才可比
        let (x, y) = parse_point(args.get(1).map(String::as_str).unwrap_or(""));
        report_point(uia, cache_walk, walker, x, y);
    } else if cmd == "follow" {
        let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
        follow(uia, cache_walk, walker, secs);
    } else {
        scan(uia, cache_walk, walker);
    }
}

fn parse_point(s: &str) -> (i32, i32) {
    let (x, y) = s.split_once(',').unwrap_or(("", ""));
    (x.trim().parse().unwrap_or(0), y.trim().parse().unwrap_or(0))
}

/// 单点查询：打印元素链、该点最上层窗口、两者是否提权（权限类失败靠它定位）。
fn report_point(
    uia: &IUIAutomation,
    cache_walk: &IUIAutomationCacheRequest,
    walker: &IUIAutomationTreeWalker,
    x: i32,
    y: i32,
) {
    let q = query_at(uia, cache_walk, walker, x, y);
    println!(
        "点 ({x},{y})：单次 {:.1}ms，链深 {}，祖先链 {:.1}ms",
        q.point_ms,
        q.chain.len(),
        q.walk_ms
    );
    for (i, e) in q.chain.iter().enumerate() {
        println!(
            "{}[{}] '{}'  {}×{} @{},{}  class={} pid={}",
            "  ".repeat(i),
            control_name(e.control),
            truncate(&e.name, 24),
            e.rect.2,
            e.rect.3,
            e.rect.0,
            e.rect.1,
            truncate(&e.class, 28),
            e.pid
        );
    }
    if let Some(err) = &q.error {
        println!("UIA 失败：{err}");
    }
    if let Some(err) = &q.walk_error {
        println!("上溯停止：{err}");
    }
    match top_window_at(x, y) {
        Some((hwnd, class, pid)) => println!(
            "该点最上层窗口：class={} pid={pid} 提权={:?} hwnd={:#x}",
            truncate(&class, 28),
            is_process_elevated(pid),
            hwnd.0 as isize
        ),
        None => println!("该点没有可见窗口"),
    }
}

/// 该点最上层的可见窗口（`EnumWindows` 顺序即 Z 序）。
/// 与生产代码（`overlay::snap::hit::is_candidate`）保持同一过滤：跳过最小化与 DWM 隐身窗口，
/// 否则会报出一个“压着屏幕但实际不在当前桌面”的幽灵窗口。
fn top_window_at(x: i32, y: i32) -> Option<(HWND, String, i32)> {
    for hwnd in enum_windows() {
        if unsafe { IsIconic(hwnd).as_bool() } || is_cloaked(hwnd) {
            continue;
        }
        let Some(f) = get_window_rect(hwnd) else {
            continue;
        };
        if x >= f.0 && x < f.0 + f.2 && y >= f.1 && y < f.1 + f.3 {
            return Some((hwnd, window_class(hwnd), window_pid(hwnd)));
        }
    }
    None
}

/// DWM 隐身（不在当前虚拟桌面 / UWP 挂起）。
fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_CLOAKED,
            (&raw mut cloaked).cast(),
            size_of::<u32>() as u32,
        )
    }
    .is_ok();
    ok && cloaked != 0
}

/// 当前进程是否提权（`TokenElevation`）。
fn is_elevated() -> Option<bool> {
    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if windows::Win32::System::Threading::OpenProcessToken(
            windows::Win32::System::Threading::GetCurrentProcess(),
            windows::Win32::Security::TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return None;
        }
        let out = token_elevation(token);
        let _ = windows::Win32::Foundation::CloseHandle(token);
        out
    }
}

/// 指定进程是否提权（打不开就返回 `None`，比如对方是更高完整性级别）。
fn is_process_elevated(pid: i32) -> Option<bool> {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid as u32).ok()?;
        let mut token = windows::Win32::Foundation::HANDLE::default();
        let ok =
            windows::Win32::System::Threading::OpenProcessToken(h, windows::Win32::Security::TOKEN_QUERY, &mut token)
                .is_ok();
        let out = ok.then(|| token_elevation(token)).flatten();
        if ok {
            let _ = windows::Win32::Foundation::CloseHandle(token);
        }
        let _ = windows::Win32::Foundation::CloseHandle(h);
        out
    }
}

fn token_elevation(token: windows::Win32::Foundation::HANDLE) -> Option<bool> {
    unsafe {
        let mut info = windows::Win32::Security::TOKEN_ELEVATION::default();
        let mut len = 0u32;
        windows::Win32::Security::GetTokenInformation(
            token,
            windows::Win32::Security::TokenElevation,
            Some(&mut info as *mut _ as *mut core::ffi::c_void),
            size_of::<windows::Win32::Security::TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .ok()?;
        Some(info.TokenIsElevated != 0)
    }
}

/// COM / UIA 的初始化结果（失败也保留错误信息，便于打印后退出）。
struct Runtime {
    uia: Result<IUIAutomation, ()>,
    /// 打点用：只要缓存，不保留元素引用（一次跳进程往返拿全部属性）。
    cache: Result<IUIAutomationCacheRequest, ()>,
    /// 上溯用：必须是 `Full`（`None` 不保留 underlying element，树导航拿不到父级）。
    cache_walk: Result<IUIAutomationCacheRequest, ()>,
    walker: Result<IUIAutomationTreeWalker, ()>,
    error: Option<String>,
}

impl Runtime {
    fn new() -> Self {
        let mut rt = Self {
            uia: Err(()),
            cache: Err(()),
            cache_walk: Err(()),
            walker: Err(()),
            error: None,
        };
        unsafe {
            // UIA 客户端用 MTA：调用可能落到后台线程时不需要跨 apartment 编组
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(u) => u,
                Err(e) => {
                    rt.error = Some(format!("CoCreateInstance(CUIAutomation)：{}", e.message()));
                    return rt;
                }
            };
            let build = |mode| -> windows::core::Result<IUIAutomationCacheRequest> {
                let c = uia.CreateCacheRequest()?;
                for prop in [
                    UIA_BoundingRectanglePropertyId,
                    UIA_ControlTypePropertyId,
                    UIA_NamePropertyId,
                    UIA_ClassNamePropertyId,
                    UIA_ProcessIdPropertyId,
                    UIA_NativeWindowHandlePropertyId,
                ] {
                    let _ = c.AddProperty(prop);
                }
                let _ = c.SetAutomationElementMode(mode);
                let _ = c.SetTreeFilter(&uia.CreateTrueCondition()?);
                Ok(c)
            };
            let cache = match build(AutomationElementMode_None) {
                Ok(c) => c,
                Err(e) => {
                    rt.error = Some(format!("CreateCacheRequest：{}", e.message()));
                    return rt;
                }
            };
            // 树导航（上溯父级）需要 Full 模式：`None` 不保留 underlying element，父级拿不到
            let cache_walk = match build(AutomationElementMode_Full) {
                Ok(c) => c,
                Err(e) => {
                    rt.error = Some(format!("CreateCacheRequest(walk)：{}", e.message()));
                    return rt;
                }
            };
            let walker = match uia.ControlViewWalker() {
                Ok(w) => w,
                Err(e) => {
                    rt.error = Some(format!("ControlViewWalker：{}", e.message()));
                    return rt;
                }
            };
            rt.uia = Ok(uia);
            rt.cache = Ok(cache);
            rt.cache_walk = Ok(cache_walk);
            rt.walker = Ok(walker);
        }
        rt
    }
}

/// 光标点 (x, y) 处的元素 + 祖先链。
fn query_at(
    uia: &IUIAutomation,
    cache_walk: &IUIAutomationCacheRequest,
    walker: &IUIAutomationTreeWalker,
    x: i32,
    y: i32,
) -> Query {
    let t0 = Instant::now();
    // 注意：这里用 **Full** 模式的缓存请求。用 `AutomationElementMode_None` 时返回的元素没有
    // underlying element 引用，随后的树导航（`GetParentElementBuildCache`）直接报「无效指针」。
    let hit = unsafe { uia.ElementFromPointBuildCache(POINT { x, y }, cache_walk) };
    let point_ms = elapsed_ms(t0);
    let root = match hit {
        Ok(e) => e,
        Err(err) => {
            return Query {
                deepest: None,
                chain: Vec::new(),
                point_ms,
                walk_ms: 0.0,
                error: Some(err.message()),
                walk_error: None,
            };
        }
    };
    let t1 = Instant::now();
    let mut chain = vec![read(&root)];
    let mut cur = root;
    let mut walk_error = None;
    for _ in 0..MAX_LEVELS {
        let parent = unsafe { walker.GetParentElementBuildCache(&cur, cache_walk) };
        let parent = match parent {
            Ok(p) => p,
            Err(e) => {
                // 走到树根时 windows-rs 会把「S_OK + 空出参」当成 Err(S_OK)，那不是错误
                if !e.code().is_ok() {
                    walk_error = Some(e.message());
                }
                break;
            }
        };
        let info = read(&parent);
        // 根元素（桌面）会与上一级矩形相同，不必再往上
        if chain.last().is_some_and(|last| last.rect == info.rect) {
            break;
        }
        chain.push(info);
        cur = parent;
    }
    let walk_ms = elapsed_ms(t1);
    Query {
        deepest: chain.first().cloned(),
        chain,
        point_ms,
        walk_ms,
        error: None,
        walk_error,
    }
}

/// 从缓存里读一个元素（`AutomationElementMode_None` 下只有缓存可用）。
fn read(el: &IUIAutomationElement) -> Element {
    unsafe {
        let rc = el.CachedBoundingRectangle().unwrap_or(RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        });
        Element {
            rect: (rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top),
            control: el.CachedControlType().map(|c| c.0).unwrap_or(0),
            name: el.CachedName().map(|b| b.to_string()).unwrap_or_default(),
            class: el.CachedClassName().map(|b| b.to_string()).unwrap_or_default(),
            pid: el.CachedProcessId().unwrap_or(0),
            hwnd: el.CachedNativeWindowHandle().map(|h| h.0 as isize).unwrap_or(0),
        }
    }
}

fn elapsed_ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// `scan`：逐窗口打点采样，最后给出四条门槛的判定。
fn scan(uia: &IUIAutomation, cache_walk: &IUIAutomationCacheRequest, walker: &IUIAutomationTreeWalker) {
    let verbose = std::env::args().any(|a| a == "--verbose" || a == "-v");
    let windows = enum_windows();
    let me = std::process::id() as i32;
    let mut points: Vec<f64> = Vec::new();
    let mut walks: Vec<f64> = Vec::new();
    let mut coord_delta: (i32, i32) = (0, 0);
    let mut covered: Vec<String> = Vec::new();
    let mut hung: Vec<String> = Vec::new();
    let mut failures = 0usize;
    let mut scanned = 0usize;
    let mut skipped = 0usize;
    let mut walk_errs: Vec<String> = Vec::new();
    let mut coord_examples: Vec<String> = Vec::new();

    // Z 序（`EnumWindows` 自上而下）用来判断采样点「真正属于」哪个窗口：
    // 元素自身的 `NativeWindowHandle` 大多是 0，靠它反查窗口不可靠。
    let zorder: Vec<TopWindow> = windows
        .iter()
        .filter(|h| window_pid(**h) != me)
        .filter_map(|h| dwm_frame(*h).map(|f| (*h, f, window_class(*h))))
        .collect();

    for hwnd in windows {
        let pid = window_pid(hwnd);
        if pid == me || unsafe { IsIconic(hwnd).as_bool() } {
            continue;
        }
        let Some(frame) = dwm_frame(hwnd) else {
            continue;
        };
        // 太小 / 奇形怪状的窗口（0 宽、1px 工具条）不采样：内侧 2px 的夹取会退化（clamp 上下界反了会 panic）
        if frame.2 < 8 || frame.3 < 8 {
            continue;
        }
        let (x_lo, x_hi) = (frame.0 + 2, frame.0 + frame.2 - 2);
        let (y_lo, y_hi) = (frame.1 + 2, frame.1 + frame.3 - 2);
        let raw = get_window_rect(hwnd).unwrap_or(frame);
        let title = window_title(hwnd);
        let class = window_class(hwnd);
        let mut win_points = 0usize;
        let mut win_min = f64::MAX;
        let mut win_max = 0f64;
        let mut depth = 0usize;
        let mut sub_control = false;
        let mut delta: (i32, i32) = (0, 0);
        let mut last_err: Option<String> = None;

        for (fx, fy) in SAMPLES {
            let x = frame.0 + (frame.2 as f64 * fx) as i32;
            let y = frame.1 + (frame.3 as f64 * fy) as i32;
            // 边框内侧 2px，避免正好落在描边上被相邻窗口接走
            let (x, y) = (x.clamp(x_lo, x_hi), y.clamp(y_lo, y_hi));
            let q = query_at(uia, cache_walk, walker, x, y);
            scanned += 1;
            if let Some(e) = &q.walk_error
                && !walk_errs.contains(e)
                && walk_errs.len() < 3
            {
                walk_errs.push(e.clone());
            }
            if let Some(err) = &q.error {
                failures += 1;
                last_err = Some(err.clone());
                continue;
            }
            // 只统计「采样点真的属于这个窗口」的那一次：窗口互相遮挡，
            // 采背景窗口的点实际拿到的是上面那个窗口的元素，比了也是白比。
            if zorder_owner(&zorder, x, y) != Some(hwnd) {
                skipped += 1;
                continue;
            }
            win_points += 1;
            points.push(q.point_ms);
            walks.push(q.walk_ms);
            win_min = win_min.min(q.point_ms);
            win_max = win_max.max(q.point_ms);
            depth = depth.max(q.chain.len());
            if q.point_ms > HUNG_MS {
                hung.push(format!("{class} ({x},{y}) 阻塞 {:.0}ms", q.point_ms));
            }
            if verbose {
                println!("      点 ({x},{y}) {:.1}ms：", q.point_ms);
                for (i, e) in q.chain.iter().enumerate() {
                    println!(
                        "        {}[{}] '{}' {}×{} @{},{} class={}",
                        "  ".repeat(i),
                        control_name(e.control),
                        truncate(&e.name, 20),
                        e.rect.2,
                        e.rect.3,
                        e.rect.0,
                        e.rect.1,
                        truncate(&e.class, 24)
                    );
                }
            }
            if let Some(deep) = &q.deepest {
                // 「比整窗更小的元素」= 这个窗口向 UIA 暴露了子控件
                let (w, h) = deep.size();
                if w + 8 < frame.2 || h + 8 < frame.3 {
                    sub_control = true;
                }
            }
            // 坐标口径：属于本窗口的那个元素（= owner）vs DWM 扩展边框 / GetWindowRect
            if let Some(win_el) = q.chain.iter().find(|e| e.hwnd == hwnd.0 as isize) {
                let d = ((win_el.rect.0 - frame.0).abs(), (win_el.rect.1 - frame.1).abs());
                if d != (0, 0) && coord_examples.len() < 8 {
                    coord_examples.push(format!(
                        "{}：UIA {}+{}  DWM {}+{}  GetWindowRect {}+{}",
                        truncate(&class, 24),
                        win_el.rect.0,
                        win_el.rect.1,
                        frame.0,
                        frame.1,
                        raw.0,
                        raw.1
                    ));
                }
                delta = (delta.0.max(d.0), delta.1.max(d.1));
            }
        }
        if win_points == 0 {
            println!(
                "[{}] pid={pid} {frame:?}  ← 采样点都被上层窗口遮住（或全部失败：{}）",
                class,
                last_err.unwrap_or_else(|| "无".into())
            );
            continue;
        }
        coord_delta = (coord_delta.0.max(delta.0), coord_delta.1.max(delta.1));
        if sub_control {
            covered.push(format!("{class} / {title} (pid={pid})"));
        }
        let raw_note = if raw != frame {
            format!(" GetWindowRect 多出 {},{}", raw.2 - frame.2, raw.3 - frame.3)
        } else {
            String::new()
        };
        println!(
            "[{}] pid={pid} 边框 {}×{} @{},{}{raw_note}\n    采样 {win_points} 点，单次 {win_min:.1}~{win_max:.1}ms，链深 {depth}，\
             坐标偏差 Δ{},{}{}",
            truncate(&class, 28),
            frame.2,
            frame.3,
            frame.0,
            frame.1,
            delta.0,
            delta.1,
            if sub_control {
                "，**有子控件**"
            } else {
                "，只有整窗"
            }
        );
    }

    if !walk_errs.is_empty() {
        println!("\n祖先链上溯报错（去重后前 3 条）：");
        for e in &walk_errs {
            println!("    · {e}");
        }
    }
    report(
        &points,
        &walks,
        coord_delta,
        &covered,
        &hung,
        failures,
        scanned,
        skipped,
        &coord_examples,
    );
}

/// `follow`：贴光标连续查询，量真实跟随开销。
fn follow(uia: &IUIAutomation, cache_walk: &IUIAutomationCacheRequest, walker: &IUIAutomationTreeWalker, secs: u64) {
    let mut last_rect = None;
    let end = Instant::now() + Duration::from_secs(secs);
    println!("跟随光标（{secs}s，Esc 退出）——把鼠标放到目标控件上观察：");
    while Instant::now() < end {
        if unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } as u16 & 0x8000 != 0 {
            break;
        }
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_err() {
            break;
        }
        let q = query_at(uia, cache_walk, walker, p.x, p.y);
        // 元素没变就不刷屏：只在矩形变化或出错时打印
        let rect = q.deepest.as_ref().map(|e| e.rect);
        if rect != last_rect || q.error.is_some() {
            last_rect = rect;
            let dpi = monitor_dpi(p.x, p.y);
            println!(
                "--- 点 ({},{}) 单次 {:.1}ms，链深 {}，祖先链 {:.1}ms，显示器 {dpi}%",
                p.x,
                p.y,
                q.point_ms,
                q.chain.len(),
                q.walk_ms
            );
            for (i, e) in q.chain.iter().enumerate() {
                println!(
                    "{}[{}] '{}'  {}×{} @{},{}  class={} pid={} hwnd={:#x}",
                    "  ".repeat(i),
                    control_name(e.control),
                    truncate(&e.name, 24),
                    e.rect.2,
                    e.rect.3,
                    e.rect.0,
                    e.rect.1,
                    truncate(&e.class, 28),
                    e.pid,
                    e.hwnd
                );
            }
            if let Some(err) = &q.error {
                println!("    （查询失败：{err}）");
                if let Some((_, class, pid)) = top_window_at(p.x, p.y) {
                    println!(
                        "      该点最上层窗口：class={} pid={pid} 提权={:?}",
                        truncate(&class, 28),
                        is_process_elevated(pid)
                    );
                }
            }
            if let Some(err) = &q.walk_error {
                println!("    （上溯停止：{err}）");
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// 汇总 + 四条门槛判定。
#[allow(clippy::too_many_arguments)]
fn report(
    points: &[f64],
    walks: &[f64],
    coord_delta: (i32, i32),
    covered: &[String],
    hung: &[String],
    failures: usize,
    scanned: usize,
    skipped: usize,
    coord_examples: &[String],
) {
    println!("\n== 汇总 ==");
    println!("窗口采样 {scanned} 次，其中落在别个窗口上被跳过 {skipped} 次，查询失败 {failures} 次");
    if points.is_empty() {
        println!("没有任何有效采样，无法判定。");
        return;
    }
    let p95_point = percentile(points, 0.95);
    let p95_walk = percentile(walks, 0.95);
    let max_point = points.iter().cloned().fold(0f64, f64::max);
    println!(
        "① 单次查询 p50 {:.1}ms / p95 {p95_point:.1}ms / max {max_point:.1}ms（门槛 p95 ≤ 30ms）→ {}",
        percentile(points, 0.50),
        verdict(p95_point <= 30.0)
    );
    println!(
        "② 祖先链 5 级累加 p95 {p95_walk:.1}ms（门槛 ≤ 80ms）→ {}",
        verdict(p95_walk <= 80.0)
    );
    println!(
        "③ 窗口级元素 vs DWM 扩展边框：最大偏差 Δ{},{} px（门槛 0px）→ {}",
        coord_delta.0,
        coord_delta.1,
        verdict(coord_delta == (0, 0))
    );
    for c in coord_examples {
        println!("    · {c}");
    }
    println!(
        "④ 能拿到比整窗更小元素的目标：{} 个（门槛 ≥ 5）→ {}",
        covered.len(),
        verdict(covered.len() >= 5)
    );
    for c in covered {
        println!("    · {c}");
    }
    if hung.is_empty() {
        println!("未出现单次 >{HUNG_MS}ms 的查询。");
    } else {
        println!("疑似目标进程无响应（单次 >{HUNG_MS}ms）：");
        for h in hung {
            println!("    · {h}");
        }
    }
    let pass = p95_point <= 30.0 && p95_walk <= 80.0 && coord_delta == (0, 0) && covered.len() >= 5;
    println!(
        "\n判定：{}",
        if pass {
            "四条门槛全部通过 → 可以按常驻 worker + 完全异步的方案实现控件级捕捉。"
        } else {
            "有门槛未通过 → 先看是哪一条：耗时不行就缩范围（只在停手 120ms 后查一次）；\
             坐标不对就先做缩放校正；覆盖不够就重新考虑这功能值不值得做。"
        }
    );
}

/// 该点「真正属于」哪个窗口（Z 序里第一个包含它、且不是桌面壳的窗口）。
fn zorder_owner(zorder: &[TopWindow], x: i32, y: i32) -> Option<HWND> {
    for (hwnd, f, class) in zorder {
        if matches!(class.as_str(), "Progman" | "WorkerW") {
            continue;
        }
        if x >= f.0 && x < f.0 + f.2 && y >= f.1 && y < f.1 + f.3 {
            return Some(*hwnd);
        }
    }
    None
}

fn percentile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((s.len() - 1) as f64 * q).round() as usize;
    s[idx.min(s.len() - 1)]
}

fn verdict(ok: bool) -> &'static str {
    if ok { "通过" } else { "**不通过**" }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n).collect();
    format!("{head}…")
}

fn enum_windows() -> Vec<HWND> {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        if unsafe { IsWindowVisible(hwnd).as_bool() } {
            out.push(hwnd);
        }
        BOOL(1)
    }
    let mut out: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn window_pid(hwnd: HWND) -> i32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid as i32
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn get_window_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    let mut rc = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rc) }.ok()?;
    Some((rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top))
}

/// DWM 扩展边框（物理像素，不含不可见的缩放边框与阴影）——与覆盖层吸附用同一口径。
fn dwm_frame(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    let mut rc = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut rc).cast(),
            size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    (w > 0 && h > 0).then_some((rc.left, rc.top, w, h))
}

/// 该点所在显示器的有效缩放百分比（用于判断 UIA 给的是物理还是逻辑像素）。
fn monitor_dpi(x: i32, y: i32) -> u32 {
    unsafe {
        let m = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(m, &mut info).as_bool() {
            return 0;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
            return 0;
        }
        dx * 100 / 96
    }
}

/// UIA 控件类型 → 可读名字（只覆盖常见的，其余打印数字）。
fn control_name(id: i32) -> String {
    let name = match id {
        50000 => "按钮",
        50001 => "日历",
        50002 => "复选框",
        50003 => "组合框",
        50004 => "编辑框",
        50005 => "链接",
        50006 => "图像",
        50007 => "列表项",
        50008 => "列表",
        50009 => "菜单",
        50010 => "菜单栏",
        50011 => "菜单项",
        50012 => "进度条",
        50013 => "单选框",
        50014 => "滚动条",
        50015 => "滑块",
        50016 => "微调",
        50017 => "状态栏",
        50018 => "选项卡",
        50019 => "选项卡项",
        50020 => "文本",
        50021 => "工具栏",
        50022 => "提示",
        50023 => "树",
        50024 => "树项",
        50025 => "自定义",
        50026 => "组",
        50027 => "Thumb",
        50028 => "数据表格",
        50029 => "数据项",
        50030 => "文档",
        50031 => "拆分按钮",
        50032 => "窗口",
        50033 => "面板",
        50034 => "标题栏",
        50038 => "表头项",
        _ => return format!("#{id}"),
    };
    name.to_string()
}
