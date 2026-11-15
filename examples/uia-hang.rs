// SPDX-License-Identifier: GPL-3.0-only
//! 无响应进程测试工具（配合 `examples/uia-probe.rs` 实测 UIA 阻塞时长）。
//!
//! 启动后先正常处理消息 2 秒（让窗口正常出现、标题栏正常），随后**永久停止处理消息**，
//! 变成一个「无响应」进程——窗口还在屏幕上（DWM 保留最后一帧），但所有窗口消息与 UIA 请求都无人应答。
//!
//! 用法：
//! ```text
//! cargo run --release --example uia-hang
//! # 另一个终端：
//! cargo run --release --example uia-probe -- follow 60
//! # 把鼠标移到那个「UIA 卡死测试窗口」上，看单次查询耗时（预期会卡到秒级）
//! ```
//!
//! 退出：控制台里 `Ctrl+C` 结束进程（窗口此时已不响应 `Alt+F4`）。

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, TranslateMessage,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::w;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn main() {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None).expect("GetModuleHandleW").into();
        let class = w!("airsnip.uia-hang");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("UIA 卡死测试窗口"),
            WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 | WS_VISIBLE.0),
            240,
            240,
            640,
            420,
            None,
            None,
            Some(hinstance),
            None,
        )
        .expect("CreateWindowExW");
        println!("窗口 {} 已创建：先正常处理消息 2 秒……", hwnd.0 as isize);
        let until = Instant::now() + Duration::from_secs(2);
        let mut msg = MSG::default();
        while Instant::now() < until {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        println!("已停止处理消息（进程不再响应）——现在用 uia-probe 指向这个窗口。Ctrl+C 退出。");
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}
