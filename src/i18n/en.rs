// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板文案：English。
//!
//! **未经母语者校对**（ADR 0047）。刻意用短词控制量宽（issue #151 R6）：
//! 工具栏格子只有约 83px（英文下自动降到 4 列），整行标签区约 288px。
//! 所以 `Highlight → Marker`、`Long screenshot → Long shot`、`Reset all` 这类缩短是
//! **布局约束**，不是文风偏好；要改长名前先量宽。

use super::SettingsKey;

/// 设置面板文案（English）。
pub(super) fn settings(k: SettingsKey) -> &'static str {
    use SettingsKey::*;
    match k {
        NavGeneral => "General",
        NavToolbar => "Toolbar",
        NavHotkeys => "Shortcuts",
        NavSave => "Save",
        NavAppearance => "Appearance",
        NavAbout => "About",

        GeneralAutostart => "Start with Windows",
        GeneralConfirmCancel => "Press Esc / right-click twice to exit",
        GeneralLanguage => "Language",
        GeneralPinOriginal => "Pin at original position",
        GeneralPinCenter => "Pin at screen center",
        GeneralDoubleClickCopy => "Double-click selection: copy",
        GeneralDoubleClickPin => "Double-click selection: pin",
        GeneralWindowSnap => "Window snap",
        GeneralControlSnap => "Control-level snap",
        GeneralOpenLogFolder => "Long shot log (no passwords)",
        GeneralOpenLogFolderBtn => "Open log folder",
        LanguageFollowSystem => "Follow system",

        GroupTools => "Tools",
        GroupColors => "Colors",
        GroupEdit => "Edit",
        GroupOutput => "Output",
        GroupFile => "Files",

        ToolMove => "Move",
        ToolRect => "Rect",
        ToolArrow => "Arrow",
        ToolText => "Text",
        ToolHighlight => "Marker",
        ToolBlur => "Blur",
        ToolGlow => "Spotlight",
        ToolNumber => "Number",
        ToolLine => "Line",
        ToolEllipse => "Ellipse",
        ToolFreehand => "Freehand",

        SlotColor1 => "Color 1",
        SlotColor2 => "Color 2",
        SlotGradient => "Gradient",
        SlotColorFallback => "Color",
        SlotFill => "Fill",
        SlotLine => "Stroke",
        SlotUndo => "Undo",
        SlotRedo => "Redo",
        SlotTextRow => "Font row",
        SlotPin => "Pin",
        SlotLongshot => "Long shot",
        SlotCopy => "Copy",
        SlotSave => "Save",
        SlotQuickSave => "Quick save",
        ToolbarFooterHint => "Copy / Save hidden; Enter copies, Ctrl+S saves",

        KeyTabCapture => "Capture",
        KeyTabOutput => "Output",
        KeyTabTools => "Tools",
        KeyShot => "Screenshot",
        KeyRecapture => "Recapture",
        KeyFullscreen => "Fullscreen",
        KeyLastArea => "Last area",
        KeySave => "Save",
        KeyQuickSave => "Quick save",
        KeyPinBorder => "Pin border",
        KeyLongshot => "Long shot",
        KeyResetAll => "Reset all",
        KeyHintLabel => "Note",
        KeyCapturing => "Press a key…",
        KeyUnbound => "Unset",
        KeyMouseMiddle => "Middle click",
        KeyMouseSide1 => "Mouse side 1",
        KeyMouseSide2 => "Mouse side 2",
        KeyUnknown => "Key {vk}",
        KeyConflict => "Key {name} is already used by another shortcut.",
        KeyClearConfirm => {
            "After clearing \u{201c}{name}\u{201d}, {name} is only available from the tray menu.\nClear it?"
        }
        KeyResetConfirm => "Resetting overwrites all {count} shortcut settings (including cleared ones).\nReset?",
        KeyModifierNote => "Unmodified keys override other apps; prefer Ctrl/Alt",

        SaveNotify => "Show a toast after quick save",
        SaveAlsoCopy => "Also copy to clipboard",
        SaveDir => "Save location (click to change)",
        SaveName => "File name",
        SaveFormat => "Format",
        SaveOnExist => "If the file exists",
        SavePreview => "Preview",
        SaveOpenFolder => "Open folder",
        SaveVarHelp => "Variables",
        SaveDirMissing => {
            "Folder does not exist yet: {dir}\nSave an image first, or click \u{201c}Save location\u{201d} to pick an existing folder."
        }
        SaveTemplateEmpty => "Save path template is empty: fill it in on the Save page.",
        SaveTemplateNoName => "Save path template has no file name: add one (variables allowed).",
        SaveNameInvalidChars => {
            "File name \u{201c}{name}\u{201d} contains characters Windows does not allow (< > : \" | ? *)."
        }
        SaveNameControlChars => "File name contains control characters.",
        SaveNameAllTaken => {
            "Too many files with the same name: \u{201c}{stem}-1\u{201d} to \u{201c}{stem}-9999\u{201d} are all taken. Clean up the folder or set \u{201c}If the file exists\u{201d} to \u{201c}Overwrite\u{201d}."
        }
        SaveDirCreateFailed => "Could not create folder {dir}: {e}",
        ExistingRename => "Auto-number",
        ExistingOverwrite => "Overwrite",

        VarYear => "Four-digit year, e.g. 2026",
        VarYearShort => "Two-digit year, e.g. 26",
        VarMonth => "Two-digit month 01\u{2013}12",
        VarDay => "Two-digit day 01\u{2013}31",
        VarHour => "Two-digit hour (24h) 00\u{2013}23",
        VarMinute => "Two-digit minute 00\u{2013}59",
        VarSecond => "Two-digit second 00\u{2013}59",
        VarTimestamp => "Unix milliseconds (never collides in the same second)",
        VarEnv => "Environment variable, e.g. %USERPROFILE%, %TEMP%",

        ThemeFollowSystem => "Follow system",
        ThemeLight => "Light",
        ThemeDark => "Dark",
        ScaleAuto => "UI scale \u{b7} Auto",
        Scale100 => "UI scale \u{b7} 100%",
        Scale125 => "UI scale \u{b7} 125%",
        Scale150 => "UI scale \u{b7} 150%",
        Scale175 => "UI scale \u{b7} 175%",
        Scale200 => "UI scale \u{b7} 200%",
        ScaleAutoValue => "Auto",

        AboutAppName => "AirSnip",
        AboutRepo => "Project page",
        AboutLicenses => "Licenses",
        AboutCheckUpdate => "Version",
        AboutAutoCheck => "Check on startup",
        AboutReward => "Support this project",
        AboutRewardLine1 => "Built and maintained in spare time:",
        AboutRewardLine2 => "chasing OS changes, fixing bugs, adding features.",
        AboutRewardLine3 => "Scan with WeChat to support it \u{2014}",
        AboutRewardLine4 => "every bit keeps it going.",
        AboutFeedback => "Feedback",
        AboutLicensesLink => "View list \u{203a}",
        AboutToastCopied => "System info copied",
        AboutToastFailed => "Could not copy system info",
        AboutToastOpen => "Open feedback page",

        UpdateCheck => "Check for updates",
        UpdateChecking => "Checking\u{2026}",
        UpdateUpToDate => "Up to date",
        UpdateToVersion => "Update to v{version}",
        UpdateDownloading => "Downloading {percent}%",
        UpdateInstalling => "Installing\u{2026}",
        UpdateRetry => "Retry",

        WindowTitle => "AirSnip Settings",
        WindowClassFailed => "Failed to register the settings window class",
        WindowCreateFailed => "Failed to create the settings window: {e}",
        AutostartFailed => {
            "Could not set up auto-start: the scheduled task could not be created or removed.\nRun AirSnip as administrator and try again."
        }
    }
}
