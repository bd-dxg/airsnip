// SPDX-License-Identifier: GPL-3.0-only
// 宿主回调表（AsHost）：Rust 覆盖层需要的平台能力 → Swift。
// C 函数指针不能捕获上下文，覆盖层视图经 ctx（未持有引用）传回；视图寿命长于覆盖层。

import AppKit
import CAirsnip
import UniformTypeIdentifiers

enum HostBridge {
    static func make(for view: OverlayView) -> AsHost {
        AsHost(
            ctx: Unmanaged.passUnretained(view).toOpaque(),
            present: { ctx, x, y, w, h in HostBridge.view(ctx)?.present(x: x, y: y, w: w, h: h) },
            set_cursor: { ctx, shape in HostBridge.view(ctx)?.setShape(shape) },
            close: { ctx in HostBridge.view(ctx)?.requestClose() },
            alert: { _, msg in
                if let msg { Alerts.show(String(cString: msg)) }
            },
            pin: { _, _, _, _ in
                Alerts.show("贴图功能将在 M5 提供，当前版本请先用复制（回车 / ⌘C）。")
                return 0
            },
            pick_save_file: { ctx, dir, name, ext, out, len in
                HostBridge.view(ctx)?.pickSaveFile(dir: dir, name: name, out: out, len: len) ?? 0
            },
            set_ime: { ctx, on in HostBridge.view(ctx)?.imeEnabled = on != 0 },
            clipboard_png: { png, len, _, _ in
                guard let png else { return }
                Clipboard.writePNG(Data(bytes: png, count: len))
            },
            notify: { msg in
                guard let msg else { return }
                let text = String(cString: msg)
                DispatchQueue.main.async { Alerts.show(text) }
            })
    }

    private static func view(_ ctx: UnsafeMutableRawPointer?) -> OverlayView? {
        ctx.map { Unmanaged<OverlayView>.fromOpaque($0).takeUnretainedValue() }
    }
}

extension OverlayView {
    /// 另存为对话框（OUT-2）：盖在覆盖层之上；选定路径写入 `out`（NUL 结尾）。
    /// 格式下拉与更多选项在 M4 补齐，当前按文件扩展名决定格式（核心校验扩展名）。
    func pickSaveFile(
        dir: UnsafePointer<CChar>?, name: UnsafePointer<CChar>?, out: UnsafeMutablePointer<CChar>?, len: Int
    ) -> Int32 {
        guard let out, let name, len > 0 else { return 0 }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = String(cString: name)
        if let dir { panel.directoryURL = URL(fileURLWithPath: String(cString: dir), isDirectory: true) }
        panel.allowedContentTypes = [.png, .jpeg, .webP]
        guard Alerts.runAbove(panel, { panel.runModal() }) == .OK, let url = panel.url else { return 0 }
        let bytes = Array(url.path.utf8)
        guard bytes.count < len else {
            Alerts.show("保存路径过长：\(url.path)")
            return 0
        }
        bytes.withUnsafeBufferPointer { src in
            out.withMemoryRebound(to: UInt8.self, capacity: len) { dst in
                dst.update(from: src.baseAddress!, count: bytes.count)
                dst[bytes.count] = 0
            }
        }
        return 1
    }
}

enum Clipboard {
    /// 写剪贴板（OUT-1）：PNG + TIFF 两种表示（部分应用只认 TIFF）。数据已复制，可从任意线程调用。
    static func writePNG(_ png: Data) {
        DispatchQueue.main.async {
            let item = NSPasteboardItem()
            item.setData(png, forType: .png)
            if let tiff = NSBitmapImageRep(data: png)?.tiffRepresentation {
                item.setData(tiff, forType: .tiff)
            }
            let pb = NSPasteboard.general
            pb.clearContents()
            pb.writeObjects([item])
        }
    }
}

enum Alerts {
    /// 高于覆盖层（屏蔽窗口层级）一级，提示框与对话框不会被覆盖层挡住。
    static let level = NSWindow.Level(rawValue: Int(CGShieldingWindowLevel()) + 1)

    static func show(_ message: String) {
        let alert = NSAlert()
        alert.messageText = "AirSnip"
        alert.informativeText = message
        _ = runAbove(alert.window) { alert.runModal() }
    }

    /// 以模态方式显示 `window`，并保证它在覆盖层之上。
    /// runModal 会把窗口层级重置为模态面板级（8），事先设的 level 无效：窗口被压在覆盖层下，
    /// 看不见也点不到，模态循环又挡住覆盖层的事件，表现为卡死。所以进入模态循环后再抬层级。
    static func runAbove<T>(_ window: NSWindow, _ run: () -> T) -> T {
        RunLoop.main.perform(inModes: [.modalPanel]) { window.level = level }
        NSApp.activate(ignoringOtherApps: true)
        return run()
    }
}
