// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层启动参数（SYS-2 / CAP-5 / OUT-2 / ADR 0028）：均为配置快照，
//! 打包传入覆盖层线程，避开 `new` / `run` 的长参数列表。
//! 从 `overlay::mod` 拆出，控制单文件规模；`mod.rs` 原样转出，外部引用不变。

use tiny_skia::Pixmap;

use crate::editor::Tool;
use crate::theme::Palette;

/// 覆盖层启动参数。
pub struct OverlayOpts {
    pub palette: &'static Palette,
    pub tool_keys: crate::settings::ToolKeys,
    /// 长截图键（默认 S，设置面板可改）。
    pub longshot_key: u32,
    /// OCR 键（默认 T，设置面板可改，OCR-3）。
    pub ocr_key: u32,
    /// OCR 是否开启（OCR-1）：关闭时工具栏不出现 OCR 按钮、OCR 键不生效。
    pub ocr_enabled: bool,
    /// 有未保存标注时取消先确认（OUT-3 / ADR 0027，设置项，issue #120）。
    pub confirm_cancel: bool,
    /// 贴图键（默认 F3，设置面板可改；ADR 0019：可为鼠标键）。
    pub pin_key: u32,
    /// 再次截图键（默认 Shift+F1，issue #118）：截图期间按下，当前画面作新底图重新框选。
    pub recapture_key: u32,
    /// 再次截图键修饰键位标志。
    pub recapture_modifiers: u32,
    /// 本次会话的默认工具（记住上次使用，M3）。
    pub default_tool: Tool,
    /// 色块颜色（EDT-7，issue #73 后续）：来自配置，缺省为 `DEFAULT_COLORS`。
    pub colors: [[u8; 4]; crate::editor::SWATCH_COUNT],
    /// 保存偏好（OUT-2）：文件名模板、上次格式与上次目录。
    pub save: crate::settings::SavePrefs,
    /// 界面缩放（CAP-5）。
    pub ui_scale: crate::settings::UiScale,
    /// 贴图编辑模式（ADR 0028）：以贴图显示像素为底图、选区锁定为内容矩形，完成写回贴图。
    pub pin_edit: Option<PinEdit>,
}

/// 贴图编辑模式的启动参数（ADR 0028）。
pub struct PinEdit {
    /// 贴图内容在覆盖层坐标中的矩形（= 屏幕矩形 − 抓屏原点）。
    pub rect: (i32, i32, u32, u32),
    /// 贴图当前显示像素（straight RGBA，`PinWindow::export_content` 产出）。
    pub pixels: Pixmap,
    /// 目标贴图窗口句柄（跨线程传原生值；完成消息回带给主线程）。
    pub target: usize,
}
