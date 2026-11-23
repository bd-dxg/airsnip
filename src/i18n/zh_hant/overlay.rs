// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层与贴图文案：繁體中文。

pub(in crate::i18n) fn overlay(k: crate::i18n::OverlayKey) -> &'static str {
    use crate::i18n::OverlayKey::*;
    match k {
        TipToolMove => "移動選取區",
        TipToolRect => "矩形工具",
        TipToolArrow => "箭頭工具",
        TipToolText => "文字工具",
        TipToolHighlight => "螢光筆",
        TipToolBlur => "高斯模糊",
        TipToolGlow => "聚光燈",
        TipToolNumber => "編號標記",
        TipToolLine => "直線工具",
        TipToolEllipse => "橢圓工具",
        TipToolFreehand => "手繪",

        TipBadgeMenu => "徽章樣式",
        TipFill => "填色",
        TipLineStyle => "線型",
        TipUndo => "復原",
        TipRedo => "重做",
        TipPin => "貼圖",
        TipLongshot => "長截圖",
        TipCopy => "複製",
        TipSave => "另存新檔",
        TipQuickSave => "快速儲存",
        TipColorSwatch => "#{hex} · 雙擊改色",
        TipGradientSwatch => "#{from} → #{to} · 雙擊換一組",
        TipRadius => "圓角 · 拖曳調整",

        CancelKeyRightButton => "右鍵",
        CancelHintEditing => "再按一次 {key} 放棄標註並離開編輯",
        CancelHintCapture => "再按一次 {key} 放棄標註並離開截圖",

        ToastSaved => "已儲存 {name} · 點擊定位檔案",

        OutputPinAllocFailed => "貼圖記憶體配置失敗，請關閉部分貼圖後重試。",
        OutputPinLockFailed => "貼圖記憶體鎖定失敗。",
        OutputCopyFailed => "複製失敗：{e}",
        OutputSaveFailed => "儲存失敗：{e}",
        QuickSaveFailed => "快速儲存失敗：{e}",

        PinMenuEdit => "編輯",
        PinMenuCopy => "複製",
        PinMenuSave => "儲存",
        PinMenuClose => "關閉",
        PinMenuCloseAll => "全部關閉",
        PinCopyFailed => "複製失敗：{e}",
        PinLimitReached => "貼圖數量已達上限（{limit} 張）。\n請關閉部分貼圖後再試。",
        PinLimitTitle => "數量上限",
        PinEditAllocFailed => "貼圖更新記憶體配置失敗。",
        PinEditLockFailed => "貼圖更新記憶體鎖定失敗。",

        ErrorOverlayWindow => "建立覆蓋層視窗失敗：{e}",
        ErrorPinWindowClass => "貼圖視窗類別註冊失敗",
        ErrorPinWindowCreate => "建立貼圖視窗失敗：{e}",
        ErrorPinCreate => "建立貼圖失敗：{e}",
        ErrorPinSize => "貼圖點陣圖尺寸無效",
        ErrorPinBorderFailed => "貼圖邊框合成失敗",
        ErrorClipboardOpen => "無法開啟剪貼簿",
    }
}
