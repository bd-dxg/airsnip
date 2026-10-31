# 13. 提权进程的托盘图标：UIPI 放行与自愈

- **Status**: Accepted
- **Context**: 本程序始终以管理员权限运行（ADR 0003），而 explorer 以普通权限运行。Windows 的 UIPI 规定**低完整性进程不能向高完整性进程的窗口发送消息**，默认静默丢弃。由此产生两个真实故障：
  1. explorer 重启后广播 `TaskbarCreated` → **被 UIPI 拦截，程序收不到** → 不重新 `NIM_ADD` → 托盘图标消失且**永远不会自愈**（`init_tray` 里的 `let _ = Shell_NotifyIconW(...)` 还吞掉了失败，无日志无重试）。
  2. 即使图标显示，explorer 点击回调的 `WM_TRAY` 同样被拦截 → **点击无响应**。

  实机复现：进程 `18:31:43` 启动、explorer `18:31:44` 重启（系统 08:00 开机，中间 10.5 小时），托盘图标消失。对照实验：**非管理员** PowerShell 广播 `TaskbarCreated` 无效、**管理员**广播后图标立即恢复——确认是 UIPI 方向性拦截，而非 `NIM_ADD` 参数问题。

  另一个独立缺陷：`run()` 里 `init_tray()` 在 `SetWindowLongPtrW(GWLP_USERDATA)` **之前**调用，该窗口期内若广播到达，`app_from()` 返回 `None`，图标同样丢失。

- **Decision**:
  1. **创建窗口后立即放行 UIPI**（`wndproc::create_message_window`）：`ChangeWindowMessageFilterEx(hwnd, TaskbarCreated, MSGFLT_ALLOW, None)` 与 `ChangeWindowMessageFilterEx(hwnd, WM_TRAY, MSGFLT_ALLOW, None)`。两条都要，缺第一条图标不恢复，缺第二条点击无响应。
  2. **`init_tray` 返回成功与否**，不再吞掉 `Shell_NotifyIconW` 的结果；`JIETU_DEBUG` 下打印 `GetLastError`。
  3. **自愈重试**：新增 `App::ensure_tray()`，失败时 `SetTimer(TIMER_TRAY_RETRY, 500ms)` 补加，上限 5 次，成功即 `KillTimer` 并清零计数。首次启动、`TaskbarCreated`、休眠唤醒三条路径统一走它。
  4. **补 `WM_POWERBROADCAST`**：`PBT_APMRESUMEAUTOMATIC` / `PBT_APMRESUMESUSPEND` 时重新 `ensure_tray()`。休眠唤醒会重建通知区但**不重启 explorer**，不会触发 `TaskbarCreated`，必须单独处理。
  5. **调整初始化顺序**：`SetWindowLongPtrW(GWLP_USERDATA)` 提到 `ensure_tray()` 与 `register_hotkeys()` 之前，消除「窗口期消息到达却取不到 `App`」的窗口。
  6. 两个 `PBT_APMRESUME*` 常量在 windows 0.62 的 `Win32::UI::WindowsAndMessaging`（不是 `System::Power`），无需额外 feature。

- **Consequences**:
  - 更容易：explorer 崩溃/重启、休眠唤醒、开机自启早于 explorer 就绪三种场景下托盘图标都能自动恢复；失败有日志可查。
  - 更难／代价：放行 `WM_TRAY` 意味着低权限进程可以向本程序投递该消息（本程序对 `WM_TRAY` 只解析鼠标事件、不执行特权操作，风险可接受）。
  - **陷阱**（改动时先看这里）：
    - **必须用 release 版验证**：dev 版 `app.dev.manifest` 不提权，UIPI 问题不复现，`cargo run` 测不出来。
    - `ChangeWindowMessageFilterEx` 必须在 `CreateWindowExW` 之后调用（要真实 `HWND`），且早于任何可能收到广播的操作。
    - `TIMER_TRAY_RETRY` 与程序中其他 `SetTimer` ID 不能撞车；`WM_TIMER` 分支用 `if` 守卫匹配，避免抢走别的定时器消息。
    - `WM_POWERBROADCAST` 对已处理的唤醒事件要返回 `TRUE(1)`，其他事件必须走 `DefWindowProcW`，否则会破坏系统电源事件链。
