// SPDX-License-Identifier: GPL-3.0-only
//! App-shell text: English (not yet reviewed by a native speaker).

pub(in crate::i18n) fn app(k: crate::i18n::AppKey) -> &'static str {
    use crate::i18n::AppKey::*;
    match k {
        TrayTooltip => "AirSnip",
        TrayCapture => "Screenshot",
        TrayPinFromClipboard => "Pin from clipboard",
        TraySettings => "Settings...",
        TrayQuit => "Exit",

        DialogSaveImage => "Save screenshot",
        DialogPng => "PNG image",
        DialogJpeg => "JPEG image",
        DialogWebp => "WebP image (lossless)",
        DialogPickFolder => "Choose save folder",

        HotkeyNameCapture => "Screenshot",
        HotkeyNamePin => "Pin",
        HotkeyTaken => {
            "Could not register {name}: the key is already taken by another app.\nPick a different shortcut in Settings."
        }
        HotkeyFailed => "Could not register {name} (error {code}).",
        HotkeyNoModifier => {
            "The screenshot ({shot}) and pin ({pin}) hotkeys have no modifier and will override other apps.\nIf that gets in the way, change them in Settings."
        }

        MouseRawFailed => {
            "Could not register raw mouse input, so mouse hotkeys will not work.\nUse keyboard shortcuts instead."
        }

        CaptureFailed => "Screen capture failed: {e}",
        NotImplemented => "\u{201c}{feature}\u{201d} is not implemented yet ({milestone}).",

        PinEditTooLarge => "This pin is too large for an edit window on the current screen.\nShrink the pin first.",

        WindowClassFailed => "Failed to register the window class (error {code})",
        WindowCreateFailed => "Failed to create the window: {e}",
        MenuWindowClassFailed => "Failed to register the menu window class",
        MenuWindowCreateFailed => "Failed to create the menu window: {e}",

        UpdateBalloonTitle => "AirSnip update available",
        UpdateBalloonBody => "v{version} is out. Click the tray icon to open Settings and update.",

        SaveWriteFailed => "Failed to write {path}: {e}",
        SaveCreateFailed => "Failed to create {path}: {e}",
        EncodeJpegFailed => "JPEG encoding failed: {e}",
        EncodeWebpFailed => "WebP encoding failed: {e}",
        UnsupportedFormat => "Unsupported image format: {ext} (use .png / .jpg / .webp)",
        NoExtension => "(no extension)",
    }
}
