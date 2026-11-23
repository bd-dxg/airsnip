// SPDX-License-Identifier: GPL-3.0-only
//! 長截圖文案：繁體中文。

pub(in crate::i18n) fn longshot(k: crate::i18n::LongshotKey) -> &'static str {
    use crate::i18n::LongshotKey::*;
    match k {
        BarIdle => "滾輪捲動 · Enter 完成 · Ctrl+S 另存為",
        BarTipUndo => "復原",
        BarTipSaveAs => "另存為",
        BarTipDone => "完成（複製並儲存）",
        BarTipCancel => "取消",

        HintCancelled => "已取消",
        HintFinished => "已完成",
        HintFailed => "拼接定位失敗，請往回捲動一點",

        ErrorInit => "長截圖初始化失敗：{e}",
        ErrorWindow => "建立長截圖視窗失敗：{e}",
        ErrorViewport => "視窗區尺寸無效",
    }
}
