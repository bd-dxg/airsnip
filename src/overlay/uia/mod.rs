// SPDX-License-Identifier: GPL-3.0-only
//! 控件级捕捉的覆盖层侧状态（CAP-6 二级 / ADR 0039）：请求调度、回包取用与熔断。
//! 真正的 UIA 调用在 `worker` 线程里（见其文档：无响应目标会阻塞 10 秒）。
//!
//! 时序：鼠标换窗口 → 先亮整窗候选（立即可用）→ 异步发一次查询 →
//! 回包到了、且光标还在同一个窗口 → 换成最深一级控件；之后滚轮可上下爬层级。

mod geom;
mod worker;

use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;

/// 慢窗口的冷却时间：卡死过一次就先别问了，冷却后再试一次。
const BLOCK_FOR: Duration = Duration::from_secs(30);
/// 超过它就判定这个窗口「慢」（实测无响应会卡 10 秒，这里只要一个保守的界）。
const SLOW_MS: u64 = 1000;

/// `AIRSNIP_DEBUG=1` 时打一行调试日志（与仓库其它调试输出同一口径）。
fn debug(msg: &str) {
    if crate::app::mouse::debug_enabled() {
        eprintln!("[airsnip] uia: {msg}");
    }
}

/// 控件级捕捉的运行态。
#[derive(Default)]
pub(super) struct Uia {
    worker: Option<worker::Worker>,
    /// 已发出的请求：(代次, 目标窗口)。只留一条——新请求覆盖旧请求。
    pending: Option<(u64, isize)>,
    /// 熔断表：(窗口, 解封时刻)。
    blocked: Vec<(isize, Instant)>,
    next_gen: u64,
}

impl Uia {
    /// 起 worker（幂等）。COM 初始化与预热都在里面付掉，覆盖层不等它。
    pub(super) fn spawn(&mut self, overlay: HWND) {
        if self.worker.is_none() {
            debug("起 worker");
            self.worker = Some(worker::Worker::spawn(overlay));
        }
    }

    /// 异步查一次某个窗口上的控件层级。返回是否真的发出（熔断 / 并发未就绪时为 false）。
    pub(super) fn request(&mut self, hwnd: isize, x: i32, y: i32, frame: (i32, i32, i32, i32)) -> bool {
        if self.blocked(hwnd) {
            debug("目标窗口在熔断冷却中，跳过");
            return false;
        }
        let Some(w) = &self.worker else {
            debug("worker 未起，跳过");
            return false;
        };
        self.next_gen += 1;
        let seq = self.next_gen;
        if !w.send(worker::Request { seq, hwnd, x, y, frame }) {
            debug(&format!("seq={seq} 通道满（worker 忙），丢弃"));
            return false;
        }
        debug(&format!("seq={seq} 发请求 hwnd={hwnd:#x} 点({x},{y}) 边框={frame:?}"));
        self.pending = Some((seq, hwnd));
        true
    }

    /// 取回包：只有「待回包的那一代 + 那一个窗口」才算数，旧回包直接丢。
    /// 顺便维护熔断表（耗时超过 `SLOW_MS` 的窗口冷却 `BLOCK_FOR`）。
    pub(super) fn take(&mut self) -> Option<(isize, Vec<worker::Rect4>)> {
        // 锁中毒（实践上不会发生，因为锁内只有一次赋值）：把在途标记清掉，
        // 否则控件级会因为一次异常永久静默（`busy()` 恒为真，再也不发请求）。
        let Some(reply) = ({
            let w = self.worker.as_ref()?;
            w.replies.lock().ok().and_then(|mut slot| slot.take())
        }) else {
            self.pending = None;
            return None;
        };
        // 只认「待回包的那一代」：光标已经换窗口时，早先那次的回包直接丢
        if self.pending.map(|(seq, _)| seq) != Some(reply.seq) {
            debug(&format!(
                "seq={} 回包过期（pending={:?}），丢弃",
                reply.seq, self.pending
            ));
            return None;
        }
        self.pending = None;
        debug(&format!(
            "seq={} 回包 hwnd={:#x} 层级 {} 耗时 {}ms 错误 {:?}",
            reply.seq,
            reply.hwnd,
            reply.levels.len(),
            reply.elapsed_ms,
            reply.error
        ));
        if reply.elapsed_ms >= SLOW_MS {
            self.block(reply.hwnd);
        }
        if reply.error.is_some() {
            return None;
        }
        Some((reply.hwnd, reply.levels))
    }

    /// 是否有请求在途（worker 正在算上一次）：在途时不再发，避免堆积。
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// 该窗口是否处于熔断冷却中。过期的记录在这里顺手清掉（冷却后允许重试一次）。
    pub(super) fn blocked(&mut self, hwnd: isize) -> bool {
        let now = Instant::now();
        self.blocked.retain(|(_, until)| *until > now);
        self.blocked.iter().any(|(h, _)| *h == hwnd)
    }

    fn block(&mut self, hwnd: isize) {
        let until = Instant::now() + BLOCK_FOR;
        match self.blocked.iter_mut().find(|(h, _)| *h == hwnd) {
            Some(e) => e.1 = until,
            None => self.blocked.push((hwnd, until)),
        }
    }
}

impl super::Overlay {
    /// 早一点把 worker 起来：COM 初始化 + 预热查询（实测首次查询 20–40ms）
    /// 趁覆盖层淡入那 120ms 先付掉，别让用户第一次悬停撞上。
    pub(super) fn spawn_uia_worker(&mut self) {
        self.uia.spawn(self.hwnd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 熔断：卡过的窗口在冷却期内被拒，冷却过后允许重试。
    #[test]
    fn blocked_window_expires() {
        let mut u = Uia::default();
        assert!(!u.blocked(42));
        u.block(42);
        assert!(u.blocked(42), "刚熔断的窗口应被拒绝");
        assert!(!u.blocked(43), "熔断不应连坐同进程的其他窗口");
        // 手动把解封时间提前，模拟「冷却结束」
        u.blocked[0].1 = Instant::now();
        assert!(!u.blocked(42), "冷却结束后应允许重试");
    }
}
