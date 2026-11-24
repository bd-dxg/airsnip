// SPDX-License-Identifier: GPL-3.0-only
//! 長いスクリーンショットの文案：日本語。

pub(in crate::i18n) fn longshot(k: crate::i18n::LongshotKey) -> &'static str {
    use crate::i18n::LongshotKey::*;
    match k {
        // 最窄の状態バー（470px）は文字に 330px しか使えないので、短い言い回しにする。
        BarIdle => "スクロール · Enter で完了 · Ctrl+S で保存",
        BarTipUndo => "元に戻す",
        BarTipSaveAs => "名前を付けて保存",
        BarTipDone => "完了（コピーして保存）",
        BarTipCancel => "キャンセル",

        HintCancelled => "キャンセルしました",
        HintFinished => "完了しました",
        HintFailed => "つなぎ合わせに失敗しました。少し戻ってください",
        HintTransparent => "画面を取得できません · 不透明度を 100% に",

        ErrorInit => "長いスクリーンショットの開始に失敗しました：{e}",
        ErrorWindow => "長いスクリーンショットのウィンドウ作成に失敗しました：{e}",
        ErrorViewport => "表示領域サイズが不正です",
    }
}
