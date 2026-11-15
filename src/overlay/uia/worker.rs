// SPDX-License-Identifier: GPL-3.0-only
//! UIA 查询线程（CAP-6 二级 / ADR 0039）。
//!
//! **所有 COM 对象都在这个线程里创建与使用**，跨线程只传纯数据（`Request` / `Reply`），
//! 因此没有任何 COM 编组问题。线程的生命周期 = 一次截图会话。
//!
//! 为什么必须是独立线程：实测目标进程无响应时 `ElementFromPointBuildCache` **会阻塞 10 秒**
//! （`docs/uia-probe.md` 第四轮），同步调用覆盖层的 UI 线程等于让鼠标键盘死 10 秒。

use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationTreeWalker, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId, UIA_ControlTypePropertyId,
    UIA_NamePropertyId, UIA_ProcessIdPropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use super::super::wndproc::WM_SNAP_RESULT;
use super::debug;
use super::geom;
pub(super) use super::geom::Rect4;

/// 向下最多钻几层（实测 Chromium 页面约 6 层，留点余量）。
const MAX_DEPTH: usize = 12;
/// 一次查询最多访问多少个元素（树可能很大，比如 Chromium 上千节点，必须能随时停）。
const NODE_BUDGET: usize = 400;

/// 一次查询请求。
pub(super) struct Request {
    /// 代次：覆盖层只认最新一次，旧回包直接丢弃。
    pub(super) seq: u64,
    /// 期望的顶层窗口（Z 序枚举得到的那个）。
    pub(super) hwnd: isize,
    /// 屏幕物理像素。
    pub(super) x: i32,
    pub(super) y: i32,
    /// 该窗口的 DWM 扩展边框：所有元素矩形都裁到它里面（见 ADR 0039 §5）。
    pub(super) frame: Rect4,
}

/// 查询回包。
pub(super) struct Reply {
    pub(super) seq: u64,
    pub(super) hwnd: isize,
    /// 由深到浅的层级矩形（已裁到窗口边框内、已去重）。空 = 这层没有更细的信息。
    pub(super) levels: Vec<Rect4>,
    pub(super) elapsed_ms: u64,
    /// 查询失败的原因（超时 / 拒绝访问 / 无效指针…）。
    pub(super) error: Option<String>,
}

/// 请求通道深度 1：覆盖层永远只留最新一条，worker 忙时旧请求直接丢。
pub(super) struct Worker {
    tx: SyncSender<Request>,
    /// 回包槽：worker 写、覆盖层在 `WM_SNAP_RESULT` 里取。
    pub(super) replies: Arc<Mutex<Option<Reply>>>,
    _thread: std::thread::JoinHandle<()>,
}

impl Worker {
    /// 起线程并**先做一次废查询预热**：实测首次查询要 20–40ms（provider 启动），
    /// 趁覆盖层淡入那 120ms 先付掉，别让用户第一次悬停撞上。
    pub(super) fn spawn(overlay: HWND) -> Self {
        let (tx, rx) = sync_channel::<Request>(1);
        let replies: Arc<Mutex<Option<Reply>>> = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&replies);
        // `HWND` 不是 `Send`：跨线程只传原生值，在 worker 里重建
        let raw = overlay.0 as isize;
        let thread = std::thread::spawn(move || {
            let overlay = HWND(raw as *mut core::ffi::c_void);
            run(rx, overlay, slot)
        });
        Self {
            tx,
            replies,
            _thread: thread,
        }
    }

    /// 投递查询（不阻塞）。返回是否真的进了通道：通道满说明 worker 正忙，
    /// 这一次直接放弃——调用方也就不该把它的代次当成「待回包」。
    pub(super) fn send(&self, req: Request) -> bool {
        self.tx.try_send(req).is_ok()
    }
}

/// worker 主循环：COM 初始化一次，之后逐条处理请求直到通道关闭。
fn run(rx: Receiver<Request>, overlay: HWND, replies: Arc<Mutex<Option<Reply>>>) {
    let Some((uia, cache, walker)) = init_uia() else {
        return;
    };
    // 预热：拿一个必然失败的坐标做一次转移，让 provider 提前起来
    let _ = unsafe {
        uia.ElementFromPointBuildCache(
            POINT {
                x: i32::MIN,
                y: i32::MIN,
            },
            &cache,
        )
    };

    while let Ok(req) = rx.recv() {
        let t0 = Instant::now();
        let (levels, error) = query(&uia, &cache, &walker, &req);
        let reply = Reply {
            seq: req.seq,
            hwnd: req.hwnd,
            levels,
            elapsed_ms: t0.elapsed().as_millis() as u64,
            error,
        };
        debug(&format!(
            "seq={} 点({},{}) → 层级 {} 耗时 {}ms 错误 {:?}",
            reply.seq,
            req.x,
            req.y,
            reply.levels.len(),
            reply.elapsed_ms,
            reply.error
        ));
        if let Ok(mut slot) = replies.lock() {
            *slot = Some(reply);
        }
        debug(&format!("seq={} 已回包并唤醒覆盖层", req.seq));
        unsafe {
            // 唤醒覆盖层（不跨线程碰任何 Overlay 状态）
            let _ = PostMessageW(Some(overlay), WM_SNAP_RESULT, WPARAM(0), LPARAM(0));
        }
    }
}

/// COM + UIA 一次初始化（worker 线程与集成测试共用）。
fn init_uia() -> Option<(IUIAutomation, IUIAutomationCacheRequest, IUIAutomationTreeWalker)> {
    unsafe {
        // UIA 客户端用 MTA：不要求消息泵，且允许在 worker 线程里自由调用
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let uia: IUIAutomation = match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
        Ok(u) => u,
        // 初始化失败（极罕见）：退回「只有整窗」——不报错、不卡
        Err(e) => {
            debug(&format!("CoCreateInstance 失败：{}", e.message()));
            return None;
        }
    };
    let cache = match build_cache(&uia) {
        Some(c) => c,
        None => {
            debug("CreateCacheRequest 失败");
            return None;
        }
    };
    let walker = match unsafe { uia.ControlViewWalker() } {
        Ok(w) => w,
        Err(e) => {
            debug(&format!("ControlViewWalker 失败：{}", e.message()));
            return None;
        }
    };
    Some((uia, cache, walker))
}

/// 缓存请求：一次跨进程往返取回全部需要的属性。
/// **必须是 `Full`**：`None` 模式下元素不保留 underlying element，树导航（上溯父级）会报「无效指针」。
fn build_cache(uia: &IUIAutomation) -> Option<IUIAutomationCacheRequest> {
    unsafe {
        let c = uia.CreateCacheRequest().ok()?;
        for prop in [
            UIA_BoundingRectanglePropertyId,
            UIA_ControlTypePropertyId,
            UIA_NamePropertyId,
            UIA_ClassNamePropertyId,
            UIA_ProcessIdPropertyId,
        ] {
            let _ = c.AddProperty(prop);
        }
        let _ = c.SetAutomationElementMode(AutomationElementMode_Full);
        let _ = c.SetTreeFilter(&uia.CreateTrueCondition().ok()?);
        Some(c)
    }
}

/// 从**目标窗口自己的元素树**往下找「包含光标的那个子元素」，返回「由深到浅」的层级矩形。
///
/// 为什么不用 `ElementFromPoint`：它做的是**全局命中测试**，而截图覆盖层铺满屏幕且最上层，
/// 于是一定先命中覆盖层自己（实测日志：`根进程 …(airsnip.overlay)`）——控件级永远不会生效。
/// 从 `ElementFromHandle` 向下走则完全不依赖全局命中：覆盖层、全屏遮挡物都影响不到它。
fn query(
    uia: &IUIAutomation,
    cache: &IUIAutomationCacheRequest,
    walker: &IUIAutomationTreeWalker,
    req: &Request,
) -> (Vec<Rect4>, Option<String>) {
    let hwnd = HWND(req.hwnd as *mut core::ffi::c_void);
    let root = match unsafe { uia.ElementFromHandleBuildCache(hwnd, cache) } {
        Ok(e) => e,
        Err(e) => return (Vec::new(), Some(e.message())),
    };
    // 由浅到深走一遍，只进「包含光标、面积最小」的那个子元素
    let mut path: Vec<Rect4> = Vec::new();
    let mut cur = root;
    let mut budget = NODE_BUDGET;
    for _ in 0..MAX_DEPTH {
        let Some((child, rect)) = child_at(walker, cache, &cur, (req.x, req.y), &mut budget) else {
            break;
        };
        // 与窗口等大（或更大）的层没有信息量，到此为止
        let Some(c) = geom::clip(rect, req.frame) else {
            break;
        };
        if c.2 >= req.frame.2 && c.3 >= req.frame.3 {
            break;
        }
        path.push(c);
        cur = child;
    }
    // 一点子元素都没拿到（自绘界面 / UWP 宿主）：至少给一层「客户区」。
    // 截窗口内容而不要标题栏与边框是高频需求，而这层不靠 UIA，两个 Win32 调用就能拿。
    if path.is_empty()
        && let Some(ca) = geom::client_area(req.hwnd, req.frame)
    {
        path.push(ca);
    }
    debug(&format!(
        "点({},{}) 框 {:?} → 下钻 {} 层 {:?}",
        req.x,
        req.y,
        req.frame,
        path.len(),
        path
    ));
    // 由深到浅给覆盖层用（滚轮往上是父级）
    path.reverse();
    (path, None)
}

/// 在 `parent` 的子元素里挑包含光标的那个（多个重叠时取**面积最小**的），返回元素与其矩形。
/// `budget` 是这次查询的总节点预算：树可能很浅也可能很大（Chromium 上千节点），要能随时停。
fn child_at(
    walker: &IUIAutomationTreeWalker,
    cache: &IUIAutomationCacheRequest,
    parent: &IUIAutomationElement,
    point: (i32, i32),
    budget: &mut usize,
) -> Option<(IUIAutomationElement, Rect4)> {
    let mut child = unsafe { walker.GetFirstChildElementBuildCache(parent, cache) }.ok()?;
    let mut best: Option<(IUIAutomationElement, Rect4)> = None;
    loop {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        if let Some(r) = rect_of(&child)
            && geom::contains(r, point)
        {
            let area = r.2 as i64 * r.3 as i64;
            let better = best.as_ref().is_none_or(|(_, b)| area < b.2 as i64 * b.3 as i64);
            if better {
                best = Some((child.clone(), r));
            }
        }
        match unsafe { walker.GetNextSiblingElementBuildCache(&child, cache) } {
            Ok(next) => child = next,
            // 走到最后一个兄弟（`Err(S_OK)`）或出错：结束
            Err(_) => break,
        }
    }
    best
}

fn rect_of(el: &IUIAutomationElement) -> Option<Rect4> {
    unsafe {
        let rc: RECT = el.CachedBoundingRectangle().ok()?;
        Some((rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        GW_HWNDNEXT, GetClassNameW, GetTopWindow, GetWindow, GetWindowRect, IsIconic, IsWindowVisible,
    };

    /// 真桌面集成测试（需手动触发）：逐个可见顶层窗口，取中心点跑一次 `query()`，
    /// 打印层级数与层级矩形。控件级不对劲时先跑它：
    /// 层级 0 = 该程序不暴露子元素（或窗口元素都拿不到），有层级 = 能下钻。
    /// 因为现在是从 `ElementFromHandle` 往下走，**即使窗口被遮住也能测**（与截图时的行为一致）。
    /// `cargo test -- --ignored --nocapture uia_query_each_window`
    #[test]
    #[ignore = "需要真实桌面"]
    fn uia_query_each_window() {
        let Some((uia, cache, walker)) = init_uia() else {
            panic!("UIA 初始化失败");
        };
        let mut hwnd = unsafe { GetTopWindow(None) }.unwrap_or_default();
        let mut seen = 0;
        while !hwnd.is_invalid() {
            if unsafe { IsWindowVisible(hwnd).as_bool() } && !unsafe { IsIconic(hwnd).as_bool() } {
                let mut buf = [0u16; 128];
                let n = unsafe { GetClassNameW(hwnd, &mut buf) };
                let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
                let mut rc = RECT::default();
                if unsafe { GetWindowRect(hwnd, &mut rc) }.is_ok() {
                    let frame = (rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top);
                    if frame.2 > 40 && frame.3 > 40 {
                        let req = Request {
                            seq: 1,
                            hwnd: hwnd.0 as isize,
                            x: frame.0 + frame.2 / 2,
                            y: frame.1 + frame.3 / 2,
                            frame,
                        };
                        let (levels, err) = query(&uia, &cache, &walker, &req);
                        println!(
                            "{} 框{}x{}@{},{} → 层级 {} {:?} {:?}",
                            class,
                            frame.2,
                            frame.3,
                            frame.0,
                            frame.1,
                            levels.len(),
                            levels.first(),
                            err
                        );
                        seen += 1;
                    }
                }
            }
            hwnd = match unsafe { GetWindow(hwnd, GW_HWNDNEXT) } {
                Ok(h) => h,
                Err(_) => break,
            };
        }
        println!("共 {seen} 个窗口");
    }

    // 矩形工具（`clip` / `contains` / 客户区兜底）的用例已随实现搬到 `uia::geom`。
}
