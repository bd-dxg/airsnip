// SPDX-License-Identifier: GPL-3.0-only
//! 长截图文案：English.

pub(in crate::i18n) fn longshot(k: crate::i18n::LongshotKey) -> &'static str {
    use crate::i18n::LongshotKey::*;
    match k {
        // 最窄状态条（470px）只有 330px 放文字，这里刻意用短句而不是
        // 「Scroll the page · Press Enter to finish · Ctrl+S to save as…」（ADR 0047）。
        BarIdle => "Scroll \u{b7} Enter to finish \u{b7} Ctrl+S to save",
        BarTipUndo => "Undo",
        BarTipSaveAs => "Save as",
        BarTipDone => "Done (copy & save)",
        BarTipCancel => "Cancel",

        HintCancelled => "Cancelled",
        HintFinished => "Finished",
        HintFailed => "Stitching failed: scroll back a little",
        HintTransparent => "No frames captured · set window opacity to 100%",

        ErrorInit => "Failed to start the long screenshot: {e}",
        ErrorWindow => "Failed to create the long screenshot window: {e}",
        ErrorViewport => "Invalid viewport size",
    }
}
