// SPDX-License-Identifier: GPL-3.0-only
// AirSnip macOS 入口：菜单栏常驻应用（无 Dock 图标）。`--selftest` 只做 Swift ↔ Rust 桥接自检（CI 用）。

import AppKit

if CommandLine.arguments.contains("--selftest") {
    exit(SelfTest.run() ? 0 : 1)
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
