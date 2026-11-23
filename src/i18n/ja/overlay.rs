// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层与贴图文案：日本語。

pub(in crate::i18n) fn overlay(k: crate::i18n::OverlayKey) -> &'static str {
    use crate::i18n::OverlayKey::*;
    match k {
        TipToolMove => "選択範囲を移動",
        TipToolRect => "矩形ツール",
        TipToolArrow => "矢印ツール",
        TipToolText => "テキストツール",
        TipToolHighlight => "マーカー",
        TipToolBlur => "ぼかし",
        TipToolGlow => "スポットライト",
        TipToolNumber => "連番マーク",
        TipToolLine => "直線ツール",
        TipToolEllipse => "楕円ツール",
        TipToolFreehand => "フリーハンド",

        TipBadgeMenu => "バッジの種類",
        TipFill => "塗りつぶし",
        TipLineStyle => "線種",
        TipUndo => "元に戻す",
        TipRedo => "やり直し",
        TipPin => "ピン留め",
        TipLongshot => "長いスクリーンショット",
        TipCopy => "コピー",
        TipSave => "名前を付けて保存",
        TipQuickSave => "クイック保存",
        TipColorSwatch => "#{hex} · ダブルクリックで色を変更",
        TipGradientSwatch => "#{from} → #{to} · ダブルクリックで切り替え",
        TipRadius => "角丸 · ドラッグで調整",

        CancelKeyRightButton => "右クリック",
        CancelHintEditing => "もう一度 {key} で注釈を破棄して編集を終了",
        CancelHintCapture => "もう一度 {key} で注釈を破棄して終了",

        ToastSaved => "{name} を保存しました · クリックで場所を開く",

        OutputPinAllocFailed => "ピン留め用のメモリを確保できません。一部のピンを閉じて再試行してください。",
        OutputPinLockFailed => "ピン留め画像のロックに失敗しました。",
        OutputCopyFailed => "コピーに失敗しました：{e}",
        OutputSaveFailed => "保存に失敗しました：{e}",
        QuickSaveFailed => "クイック保存に失敗しました：{e}",

        PinMenuEdit => "編集",
        PinMenuCopy => "コピー",
        PinMenuSave => "保存",
        PinMenuClose => "閉じる",
        PinMenuCloseAll => "すべて閉じる",
        PinCopyFailed => "コピーに失敗しました：{e}",
        PinLimitReached => "ピンの上限（{limit} 枚）に達しました。\n一部を閉じてから再試行してください。",
        PinLimitTitle => "上限",
        PinEditAllocFailed => "ピン更新用のメモリを確保できませんでした。",
        PinEditLockFailed => "更新したピン画像のロックに失敗しました。",

        ErrorOverlayWindow => "オーバーレイウィンドウの作成に失敗しました：{e}",
        ErrorPinWindowClass => "ピンウィンドウクラスの登録に失敗しました",
        ErrorPinWindowCreate => "ピンウィンドウの作成に失敗しました：{e}",
        ErrorPinCreate => "ピンの作成に失敗しました：{e}",
        ErrorPinSize => "ピン画像のサイズが不正です",
        ErrorPinBorderFailed => "ピンの枠線の合成に失敗しました",
        ErrorClipboardOpen => "クリップボードを開けません",
    }
}
