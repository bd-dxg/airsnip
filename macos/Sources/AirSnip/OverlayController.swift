// SPDX-License-Identifier: GPL-3.0-only
// 一次截图会话（CAP-2/CAP-5）：每块显示器一个覆盖窗口 + 一个 Rust 覆盖层，坐标为该显示器的像素。
// 选区只在一块屏幕内：某屏已有选区时，其他屏不接受按下（与系统 ⌘⇧4 一致）。任一覆盖层关闭即结束整个会话。

import AppKit
import CAirsnip

/// 覆盖窗口：无边框、高于菜单栏与 Dock（屏蔽窗口层级）、出现在所有桌面空间（含全屏应用）。
final class OverlayWindow: NSWindow {
    init(frame: NSRect) {
        super.init(contentRect: frame, styleMask: .borderless, backing: .buffered, defer: false)
        level = NSWindow.Level(rawValue: Int(CGShieldingWindowLevel()))
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        isOpaque = true
        backgroundColor = .black
        hasShadow = false
        isReleasedWhenClosed = false
        setFrame(frame, display: false)
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

final class OverlayController {
    private var sessions: [(window: OverlayWindow, view: OverlayView)] = []
    private let onEnd: () -> Void
    private var ending = false

    /// 为每张抓屏创建覆盖层；全部失败时返回 nil（详情见 `airsnip_last_error`）。
    init?(shots: [DisplayShot], onEnd: @escaping () -> Void) {
        self.onEnd = onEnd
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        for shot in shots {
            let view = OverlayView(
                frame: NSRect(origin: .zero, size: shot.frame.size), scale: shot.scale, pixelHeight: shot.height)
            var host = HostBridge.make(for: view)
            let ov = shot.rgba.withUnsafeBufferPointer { buf -> OpaquePointer? in
                var img = AsImage(
                    data: buf.baseAddress, width: UInt32(shot.width), height: UInt32(shot.height),
                    stride: UInt32(shot.width * 4))
                return airsnip_overlay_new(&img, Float(shot.scale), dark ? 1 : 0, &host)
            }
            guard let ov else { continue }
            view.overlay = ov
            let window = OverlayWindow(frame: shot.frame)
            window.contentView = view
            sessions.append((window, view))
        }
        guard !sessions.isEmpty else { return nil }
        sessions.forEach { $0.view.controller = self }
    }

    /// 显示全部覆盖窗口，鼠标所在屏获得键盘焦点。
    func show() {
        let mouse = NSEvent.mouseLocation
        sessions.forEach { $0.window.orderFrontRegardless() }
        let key = sessions.first { NSMouseInRect(mouse, $0.window.frame, false) } ?? sessions[0]
        NSApp.activate(ignoringOtherApps: true)
        key.window.makeKeyAndOrderFront(nil)
        key.window.makeFirstResponder(key.view)
    }

    /// 某屏已有选区时，其他屏不接受按下。
    func mayInteract(_ view: OverlayView) -> Bool {
        !sessions.contains { $0.view !== view && airsnip_overlay_has_selection($0.view.overlay) != 0 }
    }

    /// 核心请求结束：异步收尾（此刻还在核心的调用栈里，不能释放覆盖层）。
    func requestEnd(from view: OverlayView) {
        guard !ending else { return }
        ending = true
        DispatchQueue.main.async { [self] in end(from: view) }
    }

    private func end(from view: OverlayView) {
        var viewport = [Int32](repeating: 0, count: 4)
        let result = airsnip_overlay_result(view.overlay, &viewport)
        for s in sessions {
            s.window.orderOut(nil)
            let ov = s.view.overlay
            s.view.overlay = nil
            airsnip_overlay_free(ov)
        }
        sessions.removeAll()
        // 把焦点还给截图前的应用（菜单栏应用自己没有窗口可留）
        NSApp.hide(nil)
        if result == 2 {
            Alerts.show("长截图将在 M6 提供，当前版本请先用普通截图。")
        }
        onEnd()
    }
}
