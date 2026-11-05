// SPDX-License-Identifier: GPL-3.0-only
//! WinRT 线程初始化（OCR-12）：识别跑在后台线程，由本线程自行 `RoInitialize`。
//!
//! 覆盖层的消息循环线程不能承担 WinRT 初始化，否则识别期间会阻塞绘制。
//!
//! **刻意不调用 `RoUninitialize`**：windows-rs 把激活工厂缓存在进程级静态表里
//! （`imp::factory_cache`），一旦 `RoUninitialize` 拆掉公寓，缓存里的 `IActivationFactory`
//! 指针就悬空了，同一线程再次 `RoInitialize` 后复用缓存会直接访问冲突（实测 0xC0000005）。
//! 因此 MTA 保持到进程退出：每个识别线程只初始化一次（`thread_local` 去重），
//! 线程结束后其公寓引用随进程生命周期结束，不再回收。
//! 代价：每次 OCR（新线程）让 MTA 引用计数 +1 且永不回落；不占内存，
//! 溢出需要 2³² 次 OCR，可接受。

use std::cell::Cell;

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

use super::super::Error;

thread_local! {
    /// 本线程是否已初始化（避免同一线程重复 `RoInitialize` 累积引用计数）。
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

/// 确保当前线程已初始化 WinRT（MTA）。幂等。
pub(super) fn ensure() -> Result<(), Error> {
    if INITIALIZED.with(|done| done.get()) {
        return Ok(());
    }
    match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        Ok(()) => {}
        // 调用线程已用其它模式初始化过 COM：WinRT 仍可用
        Err(e) if e.code() == RPC_E_CHANGED_MODE => {}
        Err(e) => return Err(Error::Failed(format!("WinRT 初始化失败：{e}"))),
    }
    INITIALIZED.with(|done| done.set(true));
    Ok(())
}
