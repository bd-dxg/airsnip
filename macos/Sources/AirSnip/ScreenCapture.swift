// SPDX-License-Identifier: GPL-3.0-only
// 抓屏（CAP-1/CAP-5）：ScreenCaptureKit 单帧截图，每块显示器一张，按显示器原生像素（points × backingScaleFactor）。
// 输出 RGBA8 预乘、sRGB：冻结画面以 sRGB 上屏可还原原貌，导出文件为标准 sRGB（P3 广色域外的颜色会被裁到 sRGB）。

import AppKit
import ScreenCaptureKit

/// 一块显示器的抓屏结果。
struct DisplayShot {
    let displayID: CGDirectDisplayID
    /// 显示器在全局坐标中的位置（points，左下原点）。
    let frame: NSRect
    let scale: CGFloat
    let width: Int
    let height: Int
    /// RGBA8 预乘，长度 width * height * 4。
    let rgba: [UInt8]
}

/// 抓屏前在主线程取好的显示器信息（NSScreen 不能跨并发任务传递）。
struct DisplayInfo {
    let displayID: CGDirectDisplayID
    let frame: NSRect
    let scale: CGFloat

    static func all() -> [DisplayInfo] {
        NSScreen.screens.compactMap { s in
            guard let id = s.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID
            else { return nil }
            return DisplayInfo(displayID: id, frame: s.frame, scale: s.backingScaleFactor)
        }
    }
}

enum ScreenCapture {
    /// 并行抓取所有显示器；排除本应用窗口（菜单栏菜单收起前的残影等）。
    static func captureAll(_ displays: [DisplayInfo]) async throws -> [DisplayShot] {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        let pid = ProcessInfo.processInfo.processIdentifier
        let me = content.applications.filter { $0.processID == pid }
        return try await withThrowingTaskGroup(of: DisplayShot?.self) { group in
            for info in displays {
                guard let display = content.displays.first(where: { $0.displayID == info.displayID }) else { continue }
                let filter = SCContentFilter(display: display, excludingApplications: me, exceptingWindows: [])
                group.addTask { try await capture(filter: filter, info: info) }
            }
            var shots: [DisplayShot] = []
            for try await shot in group {
                if let shot { shots.append(shot) }
            }
            return shots
        }
    }

    private static func capture(filter: SCContentFilter, info: DisplayInfo) async throws -> DisplayShot? {
        let width = Int((info.frame.width * info.scale).rounded())
        let height = Int((info.frame.height * info.scale).rounded())
        let cfg = SCStreamConfiguration()
        cfg.width = width
        cfg.height = height
        cfg.showsCursor = false
        cfg.captureResolution = .best
        let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: cfg)
        guard let rgba = rgbaPixels(image, width: width, height: height) else { return nil }
        return DisplayShot(
            displayID: info.displayID, frame: info.frame, scale: info.scale,
            width: width, height: height, rgba: rgba)
    }

    /// CGImage → RGBA8 预乘 sRGB（尺寸不符时按目标尺寸绘制）。
    static func rgbaPixels(_ image: CGImage, width: Int, height: Int) -> [UInt8]? {
        var buf = [UInt8](repeating: 0, count: width * height * 4)
        let ok = buf.withUnsafeMutableBytes { raw -> Bool in
            guard
                let ctx = CGContext(
                    data: raw.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: sRGB,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return false }
            ctx.interpolationQuality = .none
            ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        return ok ? buf : nil
    }
}

/// 全程统一的 sRGB 色彩空间（抓屏转换、冻结画面上屏共用）。
let sRGB = CGColorSpace(name: CGColorSpace.sRGB)!
