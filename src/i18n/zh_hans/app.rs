// SPDX-License-Identifier: GPL-3.0-only
//! 应用壳文案（简体中文，issue #151 B4）：托盘、文件对话框、热键注册、保存错误。

pub(in crate::i18n) fn app(k: crate::i18n::AppKey) -> &'static str {
    use crate::i18n::AppKey::*;
    match k {
        TrayTooltip => "AirSnip 轻截",
        TrayCapture => "截图",
        TrayPinFromClipboard => "从剪贴板贴图",
        TraySettings => "设置...",
        TrayQuit => "退出",

        PermissionBalloonTitle => "以普通权限运行",
        PermissionBalloonBody => "管理员窗口可能截不到，需要时请以管理员身份重开。",

        DialogSaveImage => "保存截图",
        DialogPng => "PNG 图片",
        DialogJpeg => "JPEG 图片",
        DialogWebp => "WebP 图片（无损）",
        DialogPickFolder => "选择保存文件夹",

        HotkeyNameCapture => "截图",
        HotkeyNamePin => "贴图",
        HotkeyTaken => "热键 {name} 注册失败：该键已被其他程序占用。\n请在设置中更换热键。",
        HotkeyFailed => "热键 {name} 注册失败，错误码 {code}。",
        HotkeyNoModifier => {
            "截图({shot})与贴图({pin})热键未使用修饰键，会覆盖其他程序的按键。\n如影响其他软件使用，请在设置中更换热键。"
        }

        MouseRawFailed => "无法注册鼠标输入，鼠标热键不会生效。\n请在设置中改用键盘热键。",

        CaptureFailed => "抓屏失败：{e}",
        NotImplemented => "「{feature}」尚未实现（{milestone}）。",

        PinEditTooLarge => "该贴图过大，编辑窗口在当前屏幕上放不下。\n请先缩小贴图后再编辑。",

        WindowClassFailed => "注册窗口类失败，错误码 {code}",
        WindowCreateFailed => "创建窗口失败：{e}",
        MenuWindowClassFailed => "菜单窗口类注册失败",
        MenuWindowCreateFailed => "创建菜单窗口失败：{e}",

        UpdateBalloonTitle => "AirSnip 有新版本",
        UpdateBalloonBody => "v{version} 已发布，点托盘图标打开设置更新",

        SaveWriteFailed => "写入 {path} 失败：{e}",
        SaveCreateFailed => "创建 {path} 失败：{e}",
        EncodeJpegFailed => "JPEG 编码失败：{e}",
        EncodeWebpFailed => "WebP 编码失败：{e}",
        UnsupportedFormat => "不支持的图片格式：{ext}（请用 .png / .jpg / .webp）",
        NoExtension => "（无扩展名）",
    }
}
