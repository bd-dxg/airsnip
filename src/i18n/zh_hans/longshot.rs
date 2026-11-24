// SPDX-License-Identifier: GPL-3.0-only
//! 长截图文案：简体中文。

pub(in crate::i18n) fn longshot(k: crate::i18n::LongshotKey) -> &'static str {
    use crate::i18n::LongshotKey::*;
    match k {
        BarIdle => "滚轮滚动 · Enter 完成 · Ctrl+S 另存为",
        BarTipUndo => "撤销",
        BarTipSaveAs => "另存为",
        BarTipDone => "完成（复制并保存）",
        BarTipCancel => "取消",

        HintCancelled => "已取消",
        HintFinished => "已完成",
        HintFailed => "拼接定位失败，请向回滚动一点",
        HintTransparent => "抓不到画面 · 窗口半透明时请调到 100%",

        ErrorInit => "长截图初始化失败：{e}",
        ErrorWindow => "创建长截图窗口失败：{e}",
        ErrorViewport => "视口尺寸非法",
    }
}
