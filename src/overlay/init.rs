// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层初始化（CAP-2/3）：原图 / 暗图 / 上屏图三份像素缓冲与 DIB 上屏面，
//! 以及运行时字段的初始值。从 `mod.rs` 拆出，控制单文件规模。

use tiny_skia::Pixmap;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::Ime::HIMC;

use crate::capture::CapturedScreen;
use crate::editor::{self, Document};

use super::Overlay;
use super::OverlayOpts;
use super::interaction::Phase;
use super::surface;
use super::toolbar;

impl Overlay {
    pub(super) fn new(mut capture: CapturedScreen, opts: OverlayOpts) -> Result<Self, String> {
        let OverlayOpts {
            palette,
            tool_keys,
            longshot_key,
            ocr_key,
            ocr_enabled,
            pin_key,
            default_tool,
            custom_color,
            save,
            ui_scale,
            pin_edit,
        } = opts;
        let (w, h) = (capture.width, capture.height);
        let Some(size) = tiny_skia::IntSize::from_wh(w, h) else {
            return Err("虚拟屏幕尺寸非法".into());
        };
        let rgba = std::mem::take(&mut capture.rgba);
        let mut original = Pixmap::from_vec(rgba, size).ok_or("创建原图像素图失败")?;
        let timing = crate::perf::enabled();
        let t_init = std::time::Instant::now();
        let mut dimmed = original.clone();
        let t_clone1 = t_init.elapsed();
        // 暗化（CAP-3：预生成暗图）：覆盖黑色 alpha=120，多线程分块整数乘法。
        // 贴图编辑模式（ADR 0028）不暗化：窗口外桌面不受影响，窗口内只多出一点余量。
        if pin_edit.is_none() {
            surface::dim_pixmap(&mut dimmed, 255 - 120);
        }
        let t_dim = t_init.elapsed();
        let mut display = dimmed.clone();
        let t_clone2 = t_init.elapsed();

        // 贴图编辑模式（ADR 0028）：把贴图显示像素盖印到三份底图，选区锁为内容矩形。
        let (selection, locked_selection, pin_edit_target) = match &pin_edit {
            Some(pe) => {
                let rect = super::geometry::SelRect {
                    x: pe.rect.0,
                    y: pe.rect.1,
                    w: pe.rect.2 as i32,
                    h: pe.rect.3 as i32,
                };
                for dst in [&mut original, &mut dimmed, &mut display] {
                    surface::stamp(dst, &pe.pixels, rect.x, rect.y);
                }
                (Some(rect), true, Some(pe.target))
            }
            None => (None, false, None),
        };

        // 创建上屏用 DIB section（BGRA、top-down），像素直接写入其内存。
        let (mem_dc, dib_bmp, dib_bits) = surface::create_dib_surface(w, h)?;
        if timing {
            eprintln!(
                "[airsnip] 初始化：原图 clone {t_clone1:?}，暗化 {:?}，暗图 clone {:?}，DIB {:?}",
                t_dim - t_clone1,
                t_clone2 - t_dim,
                t_init.elapsed() - t_clone2
            );
        }

        Ok(Self {
            capture,
            original,
            dimmed,
            display,
            palette,
            // 界面缩放：手动倍率或按光标所在显示器的 DPI + 分辨率自动（CAP-5）
            ui: super::ui::Ui::resolve(ui_scale),
            selection,
            first_paint: true,
            phase: Phase::Adjusting,
            drag: None,
            doc: Document::new(),
            tool: default_tool, // 记住上次工具（M3），首次为矩形
            tool_keys,
            longshot_key,
            ocr_key,
            ocr_enabled,
            pin_key,
            save,
            color_index: 1, // 默认红色
            custom_color,
            line_width: editor::DEFAULT_LINE_WIDTH,
            blur_radius: editor::DEFAULT_BLUR_RADIUS,
            font_size: editor::DEFAULT_FONT_SIZE,
            number_font_size: editor::DEFAULT_NUMBER_FONT_SIZE,
            font_family: std::sync::Arc::from(editor::DEFAULT_FONT_FAMILY),
            bold: false,
            italic: false,
            font_list: None,
            color_panel: None,
            color_edit: false,
            color_edit_before: None,
            filled: false,
            draft: None,
            pending_click: None,
            down_started_draw: false,
            selected: None,
            ctrl_drag: None,
            obj_drag: None,
            locked_selection,
            pin_edit_target,
            text_edit: None,
            bar: None,
            tip: toolbar::tip::Tip::default(),
            hwnd: HWND::default(),
            ime_old: HIMC(std::ptr::null_mut()),
            main_hwnd: HWND::default(),
            cancelled: false,
            viewport: None,
            mem_dc,
            dib_bmp,
            dib_bits,
        })
    }
}
