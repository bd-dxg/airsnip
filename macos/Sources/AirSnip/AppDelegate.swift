// SPDX-License-Identifier: GPL-3.0-only
// 菜单栏应用壳（SYS-1/SYS-2）：菜单栏图标与菜单、全局截图热键、截图流程（权限 → 抓屏 → 覆盖层）。
// 待机时没有定时器与轮询，只等热键与菜单事件。

import AppKit
import CAirsnip

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem?
    private var shotKey: HotKey?
    private var overlay: OverlayController?
    private var capturing = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        var key: UInt32 = 0
        var mods: UInt32 = 0
        airsnip_config_shot_hotkey(&key, &mods)
        let label = hotKeyLabel(keyCode: key, modifiers: mods)
        setupStatusItem(shotLabel: label)
        do {
            shotKey = try HotKey(keyCode: key, modifiers: mods, id: 1) { [weak self] in self?.startCapture() }
            NSLog("[airsnip] 截图热键 %@ 已注册", label)
        } catch {
            let status = (error as? HotKeyError)?.status ?? 0
            Alerts.show("截图快捷键 \(label) 注册失败（错误 \(status)，可能已被其他程序占用）。\n仍可点菜单栏图标 › 截图。")
        }
    }

    private func setupStatusItem(shotLabel: String) {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        let image = NSImage(systemSymbolName: "scissors", accessibilityDescription: "AirSnip")
        image?.isTemplate = true  // 跟随菜单栏深浅色
        item.button?.image = image
        let menu = NSMenu()
        let shot = NSMenuItem(title: "截图（\(shotLabel)）", action: #selector(startCapture), keyEquivalent: "")
        shot.target = self
        menu.addItem(shot)
        menu.addItem(.separator())
        let version = String(cString: airsnip_version())
        let about = NSMenuItem(title: "AirSnip \(version)（GPL-3.0）", action: nil, keyEquivalent: "")
        about.isEnabled = false
        menu.addItem(about)
        menu.addItem(NSMenuItem(title: "退出 AirSnip", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        item.menu = menu
        statusItem = item
    }

    /// 截图：权限 → 抓所有显示器 → 显示覆盖层。会话进行中再触发直接忽略。
    @objc func startCapture() {
        NSLog("[airsnip] 收到截图请求")
        guard overlay == nil, !capturing else { return }
        guard ScreenPermission.ensure() else { return }
        capturing = true
        let displays = DisplayInfo.all()
        let t0 = CFAbsoluteTimeGetCurrent()
        Task { @MainActor in
            defer { self.capturing = false }
            do {
                let shots = try await ScreenCapture.captureAll(displays)
                let t1 = CFAbsoluteTimeGetCurrent()
                guard let ctrl = OverlayController(shots: shots, onEnd: { [weak self] in self?.overlay = nil }) else {
                    Alerts.show("截图失败：\(String(cString: airsnip_last_error()))")
                    return
                }
                self.overlay = ctrl
                ctrl.show()
                let t2 = CFAbsoluteTimeGetCurrent()
                NSLog(
                    "[airsnip] 截图：%d 块屏，抓屏 %.0f ms，建覆盖层 + 显示 %.0f ms", shots.count,
                    (t1 - t0) * 1000, (t2 - t1) * 1000)
            } catch {
                Alerts.show("截图失败：\(error.localizedDescription)")
            }
        }
    }
}
