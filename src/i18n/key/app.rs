// SPDX-License-Identifier: GPL-3.0-only
//! 应用壳文案键（`app.*` / `output.*`，issue #151 B4）：托盘、文件对话框、热键注册提示、
//! 抓屏与保存错误。**不含**更新引擎的内部错误串与 `diagnose` 的排障报告（不翻译，见 ADR 0047）。

keys!(
    AppKey,
    App {
        // ── 托盘（app.tray.*）──
        /// 托盘 tooltip。
        TrayTooltip,
        TrayCapture,
        TrayPinFromClipboard,
        TraySettings,
        TrayQuit,

        // ── 权限提示（app.elevation.*，ADR 0048）──
        /// 普通权限启动时的托盘气泡标题。
        PermissionBalloonTitle,
        /// 普通权限启动时的托盘气泡正文。
        PermissionBalloonBody,

        // ── 文件对话框（app.dialog.*）──
        DialogSaveImage,
        DialogPng,
        DialogJpeg,
        /// WebP 筛选器（写明无损）。
        DialogWebp,
        DialogPickFolder,

        // ── 热键注册（app.hotkey.*）──
        /// 截图热键在提示里的名字。
        HotkeyNameCapture,
        /// 贴图热键在提示里的名字。
        HotkeyNamePin,
        /// 键被占用（`{name}`）。
        HotkeyTaken,
        /// 其它注册失败（`{name}` / `{code}`）。
        HotkeyFailed,
        /// 无修饰键风险提示（`{shot}` / `{pin}` 是**当前实际键名**）。
        HotkeyNoModifier,

        // ── 鼠标热键（app.mouse.*）──
        MouseRawFailed,

        // ── 抓屏与兜底（app.*）──
        /// `{e}`
        CaptureFailed,
        /// 未实现功能的占位提示（`{feature}` / `{milestone}`）。
        NotImplemented,

        // ── 贴图编辑（app.pin_edit.*）──
        PinEditTooLarge,

        // ── 主窗口与自绘菜单窗口（app.window.*）──
        /// 注册窗口类失败（`{code}`）。
        WindowClassFailed,
        /// `{e}`
        WindowCreateFailed,
        MenuWindowClassFailed,
        /// `{e}`
        MenuWindowCreateFailed,

        // ── 更新托盘气泡（app.update.*）──
        UpdateBalloonTitle,
        /// `{version}`
        UpdateBalloonBody,

        // ── 保存与编码（output.*）──
        /// `{path}` / `{e}`
        SaveWriteFailed,
        /// `{path}` / `{e}`
        SaveCreateFailed,
        /// `{e}`
        EncodeJpegFailed,
        /// `{e}`
        EncodeWebpFailed,
        /// 扩展名不受支持（`{ext}`）。
        UnsupportedFormat,
        /// 路径没有扩展名时的占位。
        NoExtension,
    }
);
