// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板文案：日本語。
//!
//! **未经母语者校对**（ADR 0047）。量宽约束同英文：工具栏格只有约 83px（日文下自动 4 列），
//! 整行标签区约 288px。片假名外来语天生偏长，所以「スクリーンショット」只出现在
//! 快捷键页（有 170px），工具栏里用「縦長撮影」「マーカー」这类短词。

use crate::i18n::SettingsKey;

/// 设置面板文案（日本語）。
pub(in crate::i18n) fn settings(k: SettingsKey) -> &'static str {
    use SettingsKey::*;
    match k {
        NavGeneral => "一般",
        NavToolbar => "ツールバー",
        NavHotkeys => "ショートカット",
        NavSave => "保存",
        NavAppearance => "外観",
        NavAbout => "アプリ情報",

        GeneralAutostart => "Windows と一緒に起動",
        GeneralConfirmCancel => "Esc / 右クリックを 2 回で終了",
        GeneralLanguage => "言語",
        GeneralPinOriginal => "ピン位置·元の位置",
        GeneralPinCenter => "ピン位置·画面中央",
        GeneralDoubleClickCopy => "空白をダブルクリック·コピー",
        GeneralDoubleClickPin => "空白をダブルクリック·ピン留め",
        GeneralWindowSnap => "ウィンドウ吸着",
        GeneralControlSnap => "コントロール単位の吸着",
        GeneralOpenLogFolder => "長い撮影ログ（パスワードなし）",
        GeneralOpenLogFolderBtn => "ログフォルダーを開く",
        LanguageFollowSystem => "システムに従う",

        GroupTools => "ツール",
        GroupColors => "色",
        GroupEdit => "編集",
        GroupOutput => "出力",
        GroupFile => "ファイル",

        ToolMove => "移動",
        ToolRect => "矩形",
        ToolArrow => "矢印",
        ToolText => "テキスト",
        ToolHighlight => "マーカー",
        ToolBlur => "ぼかし",
        ToolGlow => "スポットライト",
        ToolNumber => "連番",
        ToolLine => "直線",
        ToolEllipse => "楕円",
        ToolFreehand => "手描き",

        SlotColor1 => "色 1",
        SlotColor2 => "色 2",
        SlotGradient => "グラデーション",
        SlotColorFallback => "色",
        SlotFill => "塗り",
        SlotLine => "線種",
        SlotUndo => "元に戻す",
        SlotRedo => "やり直し",
        SlotTextRow => "フォント行",
        SlotPin => "ピン留め",
        SlotLongshot => "縦長撮影",
        SlotCopy => "コピー",
        SlotSave => "保存",
        SlotQuickSave => "クイック保存",
        ToolbarFooterHint => "コピー / 保存ボタンは非表示。Enter でコピー、Ctrl+S で保存できます",

        KeyTabCapture => "撮影",
        KeyTabOutput => "出力",
        KeyTabTools => "ツール",
        KeyShot => "スクリーンショット",
        KeyRecapture => "再撮影",
        KeyFullscreen => "全画面",
        KeyLastArea => "前回の範囲",
        KeySave => "保存",
        KeyQuickSave => "クイック保存",
        KeyPinBorder => "ピン枠",
        KeyLongshot => "縦長撮影",
        KeyResetAll => "すべて初期化",
        KeyHintLabel => "ヒント",
        KeyCapturing => "キーを押す…",
        KeyUnbound => "未設定",
        KeyMouseMiddle => "ホイールクリック",
        KeyMouseSide1 => "マウスサイド1",
        KeyMouseSide2 => "マウスサイド2",
        KeyUnknown => "キー {vk}",
        KeyConflict => "キー {name} は他のショートカットで使用中です。別のキーを選んでください。",
        KeyClearConfirm => "「{name}」のショートカットを消すと、トレイメニューからのみ実行できます。\n消しますか？",
        KeyResetConfirm => {
            "初期化すると {count} 項目すべてのショートカット設定が上書きされます（消した項目も含む）。\n初期化しますか？"
        }
        KeyModifierNote => "修飾キーなしは他アプリのキーを奪います。Ctrl/Alt を推奨",

        SaveNotify => "クイック保存後に通知を表示",
        SaveAlsoCopy => "クリップボードにもコピー",
        SaveDir => "保存先（クリックで変更）",
        SaveName => "ファイル名",
        SaveFormat => "形式",
        SaveOnExist => "同名ファイルがある場合",
        SavePreview => "プレビュー",
        SaveOpenFolder => "フォルダーを開く",
        SaveVarHelp => "変数の説明",
        SaveDirMissing => {
            "フォルダーがまだありません：{dir}\n先に 1 枚保存するか、「保存先」から既存のフォルダーを選んでください。"
        }
        SaveTemplateEmpty => "保存先テンプレートが空です：設定の「保存」ページで入力してください。",
        SaveTemplateNoName => "保存先テンプレートがフォルダーだけです：ファイル名（変数可）を追加してください。",
        SaveNameInvalidChars => "ファイル名「{name}」に Windows で使えない文字が含まれています（< > : \" | ? *）。",
        SaveNameControlChars => "ファイル名に制御文字が含まれています。",
        SaveNameAllTaken => {
            "同名ファイルが多すぎます：「{stem}-1」から「{stem}-9999」まで埋まっています。フォルダーを整理するか、「同名ファイルがある場合」を「上書き」にしてください。"
        }
        SaveDirCreateFailed => "フォルダー {dir} を作成できませんでした：{e}",
        ExistingRename => "連番を追加",
        ExistingOverwrite => "上書き",

        VarYear => "4 桁の年（例：2026）",
        VarYearShort => "2 桁の年（例：26）",
        VarMonth => "2 桁の月 01\u{2013}12",
        VarDay => "2 桁の日 01\u{2013}31",
        VarHour => "2 桁の時（24 時間制）00\u{2013}23",
        VarMinute => "2 桁の分 00\u{2013}59",
        VarSecond => "2 桁の秒 00\u{2013}59",
        VarTimestamp => "Unix ミリ秒（同じ秒に複数保存しても重複しない）",
        VarEnv => "環境変数（例：%USERPROFILE%、%TEMP%）",

        ThemeFollowSystem => "システムに従う",
        ThemeLight => "ライト",
        ThemeDark => "ダーク",
        ScaleAuto => "表示倍率·自動",
        Scale100 => "表示倍率·100%",
        Scale125 => "表示倍率·125%",
        Scale150 => "表示倍率·150%",
        Scale175 => "表示倍率·175%",
        Scale200 => "表示倍率·200%",
        ScaleAutoValue => "自動",

        AboutAppName => "AirSnip",
        AboutRepo => "プロジェクトページ",
        AboutLicenses => "サードパーティライセンス",
        AboutCheckUpdate => "バージョン更新",
        AboutAutoCheck => "起動時に自動チェック",
        AboutReward => "プロジェクトを支援",
        AboutRewardLine1 => "趣味の時間で開発・維持しています：",
        AboutRewardLine2 => "OS の変更への追従、バグ修正、機能追加。",
        AboutRewardLine3 => "WeChat で応援していただけると、",
        AboutRewardLine4 => "それが更新の原動力になります。",
        AboutFeedback => "フィードバック",
        AboutLicensesLink => "一覧を見る \u{203a}",
        AboutToastCopied => "システム情報をコピーしました",
        AboutToastFailed => "システム情報をコピーできませんでした",
        AboutToastOpen => "フィードバックページを開く",

        UpdateCheck => "更新を確認",
        UpdateChecking => "確認中\u{2026}",
        UpdateUpToDate => "最新です",
        UpdateToVersion => "v{version} に更新",
        UpdateDownloading => "ダウンロード中 {percent}%",
        UpdateInstalling => "インストール中\u{2026}",
        UpdateRetry => "再試行",

        WindowTitle => "AirSnip 設定",
        WindowClassFailed => "設定ウィンドウクラスの登録に失敗しました",
        WindowCreateFailed => "設定ウィンドウの作成に失敗しました：{e}",
        AutostartFailed => {
            "自動起動の設定に失敗しました：タスクスケジューラのタスクを作成または削除できません。\n管理者として実行し直してから再試行してください。"
        }
    }
}
