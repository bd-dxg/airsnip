// SPDX-License-Identifier: GPL-3.0-only
//! アプリ本体の文言：日本語（ネイティブ校正前）。

pub(in crate::i18n) fn app(k: crate::i18n::AppKey) -> &'static str {
    use crate::i18n::AppKey::*;
    match k {
        TrayTooltip => "AirSnip",
        TrayCapture => "スクリーンショット",
        TrayPinFromClipboard => "クリップボードからピン留め",
        TraySettings => "設定...",
        TrayQuit => "終了",

        PermissionBalloonTitle => "標準権限で実行中",
        PermissionBalloonBody => {
            "管理者として実行中のウインドウは撮影できない場合があります。必要なら管理者として起動し直してください。"
        }

        DialogSaveImage => "スクリーンショットを保存",
        DialogPng => "PNG 画像",
        DialogJpeg => "JPEG 画像",
        DialogWebp => "WebP 画像（可逆）",
        DialogPickFolder => "保存フォルダーを選択",

        HotkeyNameCapture => "スクリーンショット",
        HotkeyNamePin => "ピン留め",
        HotkeyTaken => {
            "{name} を登録できませんでした：他のアプリがこのキーを使用しています。\n設定で別のショートカットに変更してください。"
        }
        HotkeyFailed => "{name} を登録できませんでした（エラー {code}）。",
        HotkeyNoModifier => {
            "スクリーンショット（{shot}）とピン留め（{pin}）に修飾キーがないため、他のアプリのキー入力を奪います。\n支障がある場合は設定で変更してください。"
        }

        MouseRawFailed => {
            "マウス入力を登録できませんでした。マウスのショートカットは動作しません。\n代わりにキーボードのショートカットを使用してください。"
        }

        CaptureFailed => "画面の取り込みに失敗しました：{e}",
        NotImplemented => "「{feature}」は未実装です（{milestone}）。",

        PinEditTooLarge => {
            "このピンは大きすぎて、現在の画面に編集ウィンドウを収められません。\n先にピンを縮小してください。"
        }

        WindowClassFailed => "ウィンドウクラスの登録に失敗しました（エラー {code}）",
        WindowCreateFailed => "ウィンドウの作成に失敗しました：{e}",
        MenuWindowClassFailed => "メニューウィンドウクラスの登録に失敗しました",
        MenuWindowCreateFailed => "メニューウィンドウの作成に失敗しました：{e}",

        UpdateBalloonTitle => "AirSnip の新しいバージョンがあります",
        UpdateBalloonBody => "v{version} が公開されました。トレイアイコンから設定を開いて更新してください。",

        SaveWriteFailed => "{path} への書き込みに失敗しました：{e}",
        SaveCreateFailed => "{path} の作成に失敗しました：{e}",
        EncodeJpegFailed => "JPEG エンコードに失敗しました：{e}",
        EncodeWebpFailed => "WebP エンコードに失敗しました：{e}",
        UnsupportedFormat => "未対応の画像形式です：{ext}（.png / .jpg / .webp を使用してください）",
        NoExtension => "（拡張子なし）",
    }
}
