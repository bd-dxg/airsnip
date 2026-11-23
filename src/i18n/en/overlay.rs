// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层与贴图文案：English。

pub(in crate::i18n) fn overlay(k: crate::i18n::OverlayKey) -> &'static str {
    use crate::i18n::OverlayKey::*;
    match k {
        TipToolMove => "Move selection",
        TipToolRect => "Rectangle tool",
        TipToolArrow => "Arrow tool",
        TipToolText => "Text tool",
        TipToolHighlight => "Marker",
        TipToolBlur => "Blur",
        TipToolGlow => "Spotlight",
        TipToolNumber => "Number badge",
        TipToolLine => "Line tool",
        TipToolEllipse => "Ellipse tool",
        TipToolFreehand => "Freehand",

        TipBadgeMenu => "Badge style",
        TipFill => "Fill",
        TipLineStyle => "Stroke",
        TipUndo => "Undo",
        TipRedo => "Redo",
        TipPin => "Pin to screen",
        TipLongshot => "Long screenshot",
        TipCopy => "Copy",
        TipSave => "Save as\u{2026}",
        TipQuickSave => "Quick save",
        TipColorSwatch => "#{hex} \u{b7} double-click to change",
        TipGradientSwatch => "#{from} \u{2192} #{to} \u{b7} double-click to switch",
        TipRadius => "Corner radius \u{b7} drag to adjust",

        CancelKeyRightButton => "right-click",
        CancelHintEditing => "Press {key} again to discard annotations and leave edit mode",
        CancelHintCapture => "Press {key} again to discard annotations and exit",

        ToastSaved => "Saved {name} \u{b7} click to locate",

        OutputPinAllocFailed => "Not enough memory to pin. Close some pins and retry.",
        OutputPinLockFailed => "Could not lock the pin image.",
        OutputCopyFailed => "Copy failed: {e}",
        OutputSaveFailed => "Save failed: {e}",
        QuickSaveFailed => "Quick save failed: {e}",

        PinMenuEdit => "Edit",
        PinMenuCopy => "Copy",
        PinMenuSave => "Save",
        PinMenuClose => "Close",
        PinMenuCloseAll => "Close all",
        PinCopyFailed => "Copy failed: {e}",
        PinLimitReached => "Pin limit reached ({limit}).\nClose some pins and try again.",
        PinLimitTitle => "Pin limit",
        PinEditAllocFailed => "Not enough memory to update the pin.",
        PinEditLockFailed => "Could not lock the updated pin image.",

        ErrorOverlayWindow => "Failed to create the overlay window: {e}",
        ErrorPinWindowClass => "Failed to register the pin window class",
        ErrorPinWindowCreate => "Failed to create the pin window: {e}",
        ErrorPinCreate => "Failed to create the pin: {e}",
        ErrorPinSize => "Invalid pin image size",
        ErrorPinBorderFailed => "Failed to compose the pin border",
        ErrorClipboardOpen => "Cannot open the clipboard",
    }
}
