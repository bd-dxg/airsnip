#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# 构建 macOS 版：Rust 核心静态库 → Swift 外壳 → dist/AirSnip.app → 签名 → dist/airsnip-v{版本}-macos-arm64.zip
#
# 签名身份：环境变量 AIRSNIP_SIGN_ID（钥匙串里的证书名，如 Developer ID）；未设置时用临时签名（ad-hoc）。
# 临时签名的默认指定要求含 cdhash，每次构建都变，「屏幕录制」授权会随之失效；
# 因此开发构建把指定要求固定为 bundle id，授权可跨构建保留（仅限开发，正式分发用 Developer ID）。
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
SIGN_ID="${AIRSNIP_SIGN_ID:--}"
APP=dist/AirSnip.app

cargo build -p airsnip-macos --profile release-macos
swift build --package-path macos -c release --arch arm64
BIN=$(swift build --package-path macos -c release --arch arm64 --show-bin-path)

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN/AirSnip" "$APP/Contents/MacOS/AirSnip"
strip -x "$APP/Contents/MacOS/AirSnip"
sed "s/__VERSION__/$VERSION/g" macos/Info.plist > "$APP/Contents/Info.plist"
if [ "$SIGN_ID" = "-" ]; then
    BUNDLE_ID=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")
    codesign --force --sign - --timestamp=none \
        --requirements "=designated => identifier \"$BUNDLE_ID\"" "$APP"
else
    codesign --force --sign "$SIGN_ID" --timestamp "$APP"
fi
codesign --verify --strict "$APP"

ZIP="dist/airsnip-v$VERSION-macos-arm64.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo "AirSnip.app：$(du -sh "$APP" | cut -f1)，zip：$(du -h "$ZIP" | cut -f1)（签名：$SIGN_ID）"
