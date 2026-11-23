// SPDX-License-Identifier: GPL-3.0-only
//! 应用壳文案：繁體中文（台灣用語，未经母语者校对）。

pub(in crate::i18n) fn app(k: crate::i18n::AppKey) -> &'static str {
    use crate::i18n::AppKey::*;
    match k {
        TrayTooltip => "AirSnip 輕截",
        TrayCapture => "截圖",
        TrayPinFromClipboard => "從剪貼簿貼圖",
        TraySettings => "設定...",
        TrayQuit => "結束",

        DialogSaveImage => "儲存截圖",
        DialogPng => "PNG 圖片",
        DialogJpeg => "JPEG 圖片",
        DialogWebp => "WebP 圖片（無損）",
        DialogPickFolder => "選擇儲存資料夾",

        HotkeyNameCapture => "截圖",
        HotkeyNamePin => "貼圖",
        HotkeyTaken => "快速鍵 {name} 註冊失敗：該鍵已被其他程式佔用。\n請在設定中更換快速鍵。",
        HotkeyFailed => "快速鍵 {name} 註冊失敗，錯誤碼 {code}。",
        HotkeyNoModifier => {
            "截圖({shot})與貼圖({pin})快速鍵未使用修飾鍵，會覆蓋其他程式的按鍵。\n如影響其他軟體使用，請在設定中更換快速鍵。"
        }

        MouseRawFailed => "無法註冊滑鼠輸入，滑鼠快速鍵不會生效。\n請在設定中改用鍵盤快速鍵。",

        CaptureFailed => "抓取畫面失敗：{e}",
        NotImplemented => "「{feature}」尚未實作（{milestone}）。",

        PinEditTooLarge => "此貼圖過大，編輯視窗在目前螢幕上放不下。\n請先縮小貼圖後再編輯。",

        WindowClassFailed => "註冊視窗類別失敗，錯誤碼 {code}",
        WindowCreateFailed => "建立視窗失敗：{e}",
        MenuWindowClassFailed => "選單視窗類別註冊失敗",
        MenuWindowCreateFailed => "建立選單視窗失敗：{e}",

        UpdateBalloonTitle => "AirSnip 有新版本",
        UpdateBalloonBody => "v{version} 已發佈，點系統匣圖示開啟設定更新",

        SaveWriteFailed => "寫入 {path} 失敗：{e}",
        SaveCreateFailed => "建立 {path} 失敗：{e}",
        EncodeJpegFailed => "JPEG 編碼失敗：{e}",
        EncodeWebpFailed => "WebP 編碼失敗：{e}",
        UnsupportedFormat => "不支援的圖片格式：{ext}（請用 .png / .jpg / .webp）",
        NoExtension => "（無副檔名）",
    }
}
