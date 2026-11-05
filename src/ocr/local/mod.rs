// SPDX-License-Identifier: GPL-3.0-only
//! WinRT 本地 OCR 实现（`Windows.Media.Ocr`）：COM 初始化、图像转换、引擎创建与结果收集。
//!
//! 这里同步等待识别完成（`SetCompleted` + Win32 事件），所以**必须**在后台线程调用；
//! 覆盖层与主窗口的消息循环不能阻塞（OCR-12）。

mod bitmap;
mod com;
mod lang;

use core::ffi::c_void;

use tiny_skia::Pixmap;
use windows::Media::Ocr::OcrResult;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::core::PCWSTR;
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

use super::{Error, Recognized};

/// 单次识别的最长等待（毫秒）：超时只影响后台线程，不卡界面。
const TIMEOUT_MS: u32 = 30_000;

pub(super) fn recognize(pixmap: &Pixmap, language: Option<&str>) -> Result<Recognized, Error> {
    com::ensure()?;
    let engine = lang::create_engine(language)?;
    let max = windows::Media::Ocr::OcrEngine::MaxImageDimension()
        .map_err(|e| Error::Failed(format!("读取引擎上限失败：{e}")))?;
    let bitmap = bitmap::to_software_bitmap(pixmap, max)?;
    let op = engine
        .RecognizeAsync(&bitmap)
        .map_err(|e| Error::Failed(format!("发起识别失败：{e}")))?;
    wait(&op)?;
    let result = op.GetResults().map_err(|e| Error::Failed(format!("识别失败：{e}")))?;
    let language = engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|t| t.to_string_lossy())
        .unwrap_or_default();
    Ok(Recognized {
        text: super::join_segments(collect_segments(&result)?),
        language,
    })
}

pub(super) fn available_languages() -> Result<Vec<String>, Error> {
    com::ensure()?;
    lang::list()
}

/// 阻塞等待异步识别完成：完成回调里 `SetEvent`，这里等事件（不依赖调用线程的消息循环）。
fn wait(op: &IAsyncOperation<OcrResult>) -> Result<(), Error> {
    let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
        .map_err(|e| Error::Failed(format!("创建等待事件失败：{e}")))?;
    let _guard = EventGuard(event);
    // 回调可能在任意线程触发，只把句柄值传进去（裸指针不满足 Send）
    let raw = event.0 as usize;
    let handler = AsyncOperationCompletedHandler::new(move |_, _| {
        unsafe {
            let _ = SetEvent(HANDLE(raw as *mut c_void));
        }
        Ok(())
    });
    op.SetCompleted(&handler)
        .map_err(|e| Error::Failed(format!("注册完成回调失败：{e}")))?;
    // 注册后再查状态：若已完成，回调可能不再触发，此时不必等待
    if op
        .Status()
        .map_err(|e| Error::Failed(format!("读取识别状态失败：{e}")))?
        == AsyncStatus::Started
    {
        let r = unsafe { WaitForSingleObject(event, TIMEOUT_MS) };
        if r != WAIT_OBJECT_0 {
            return Err(Error::Failed("识别超时".to_string()));
        }
    }
    Ok(())
}

/// 事件句柄 RAII：出错路径上也要关闭。
struct EventGuard(HANDLE);

impl Drop for EventGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// WinRT 的 `Lines` / `Words` → 带包围盒的片段（排序与拼接规则见 `ocr::lines`）。
///
/// 必须取包围盒：`Lines()` 的返回顺序不是阅读顺序，同一视觉行也会被切成多段。
fn collect_segments(result: &OcrResult) -> Result<Vec<super::Segment>, Error> {
    let lines = result
        .Lines()
        .map_err(|e| Error::Failed(format!("读取识别结果失败：{e}")))?;
    let n = lines.Size().map_err(|e| Error::Failed(e.to_string()))?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let line = lines.GetAt(i).map_err(|e| Error::Failed(e.to_string()))?;
        let words = line.Words().map_err(|e| Error::Failed(e.to_string()))?;
        let wn = words.Size().map_err(|e| Error::Failed(e.to_string()))?;
        let (mut left, mut top) = (f32::MAX, f32::MAX);
        let mut bottom = f32::MIN;
        let mut ws = Vec::with_capacity(wn as usize);
        for j in 0..wn {
            let word = words.GetAt(j).map_err(|e| Error::Failed(e.to_string()))?;
            ws.push(word.Text().map_err(|e| Error::Failed(e.to_string()))?.to_string_lossy());
            let r = word
                .BoundingRect()
                .map_err(|e| Error::Failed(format!("读取文字位置失败：{e}")))?;
            left = left.min(r.X);
            top = top.min(r.Y);
            bottom = bottom.max(r.Y + r.Height);
        }
        // 无词的碎片行直接丢掉（否则会在结果里留空行）
        if ws.is_empty() {
            continue;
        }
        out.push(super::Segment {
            left,
            top,
            bottom,
            words: ws,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tiny_skia::{Color, Pixmap, Transform};

    use super::bitmap;
    use crate::editor::Point;
    use crate::ocr::{Error, available_languages, recognize};
    use crate::render::text::{self, FontSpec};

    /// 把文字画进 pixmap（与覆盖层同一套 DirectWrite 栅格化），模拟屏幕渲染文本。
    fn render_text(text_value: &str, size: f32) -> Pixmap {
        let mut p = Pixmap::new(480, 160).expect("pixmap");
        p.fill(Color::WHITE);
        let spec = FontSpec::from_parts(&Arc::from("Microsoft YaHei UI"), size, false, false);
        text::draw(
            &mut p,
            Point::new(16.0, 24.0),
            text_value,
            &spec,
            [0, 0, 0, 255],
            Transform::identity(),
        );
        p
    }

    /// 密集多行文本不能漏行（「识别不完整、内容丢失」的回归守卫）。
    ///
    /// 三组尺寸覆盖不同缩放分支：小图与宽图都走 2x，1920×1080 也走 2x（在像素预算内）。
    /// 手动跑：`cargo test ocr::local -- --ignored`
    #[test]
    #[ignore = "需要系统 OCR 语言包（中文）"]
    fn dense_block_keeps_every_line() {
        for (w, h, gap, n) in [
            (700u32, 400u32, 40.0f32, 8usize),
            (1400, 1000, 22.0, 40),
            (1920, 1080, 22.0, 45),
        ] {
            let mut p = Pixmap::new(w, h).expect("pixmap");
            p.fill(Color::WHITE);
            let spec = FontSpec::from_parts(&Arc::from("Microsoft YaHei UI"), 15.0, false, false);
            for i in 0..n {
                text::draw(
                    &mut p,
                    Point::new(20.0, 30.0 + i as f32 * gap),
                    &format!("行{i} 中文测试文本内容"),
                    &spec,
                    [0, 0, 0, 255],
                    Transform::identity(),
                );
            }
            let out = recognize(&p, Some("zh-Hans-CN")).expect("应能识别");
            let got = out.text.lines().filter(|l| !l.trim().is_empty()).count();
            assert_eq!(got, n, "{w}x{h} 漏行：期望 {n}，得到 {got}\n{}", out.text);
        }
    }

    /// 端到端：真实渲染 → 真实 WinRT 识别（需系统 OCR 语言包）。
    /// 手动跑：`cargo test ocr::local -- --ignored`
    #[test]
    #[ignore = "需要系统 OCR 语言包"]
    fn recognizes_rendered_latin_and_chinese() {
        let langs = available_languages().expect("应能枚举识别语言");
        assert!(!langs.is_empty(), "系统未安装任何 OCR 语言包");

        let pixmap = render_text("Hello AirSnip", 40.0);
        let out = recognize(&pixmap, None).expect("应能识别英文");
        // 不比对逐字大小写（小字号下 l/I 容易混），只验证「识别出两个词且中间有空格」
        assert_eq!(
            out.text.split_whitespace().count(),
            2,
            "英文词间应保留空格：{:?}",
            out.text
        );
        assert!(
            out.text.to_lowercase().contains("airsnip"),
            "应能识别出 AirSnip：{:?}",
            out.text
        );
    }

    /// 中文行内不应出现逐字空格（WinRT `OcrLine::Text()` 的老毛病）。
    #[test]
    #[ignore = "需要系统 OCR 语言包（中文）"]
    fn recognizes_chinese_without_inner_spaces() {
        let pixmap = render_text("你好世界", 40.0);
        let out = recognize(&pixmap, Some("zh-Hans-CN")).expect("应能识别中文");
        assert!(out.text.contains("你好"), "识别结果：{:?}", out.text);
        assert!(!out.text.contains("你 好"), "中文之间不应补空格：{:?}", out.text);
    }

    /// 屏幕字号（13px）的文字也要能认对：小图靠整数倍放大提升识别率（偏旁被拆开的主因）。
    #[test]
    #[ignore = "需要系统 OCR 语言包（中文）"]
    fn recognizes_small_screen_text() {
        for size in [13.0f32, 15.0] {
            let pixmap = render_text("设置与关于", size);
            let out = recognize(&pixmap, Some("zh-Hans-CN")).expect("应能识别");
            let text: String = out.text.chars().filter(|c| !c.is_whitespace()).collect();
            assert!(
                text.contains("设置") || text.contains("关于"),
                "{size}px 识别失败：{:?}",
                out.text
            );
        }
    }

    /// 超大选区应给出明确错误，而不是让引擎自己失败。
    #[test]
    fn rejects_bitmap_over_engine_limit() {
        let pixmap = Pixmap::new(8, 8).expect("pixmap");
        let err = bitmap::to_software_bitmap(&pixmap, 4).expect_err("超过上限应报错");
        assert_eq!(err, Error::TooLarge { max: 4 });
    }

    /// **宽图也要放大**：早期只对长边 ≤1200 的图放大，宽截图上的小字照样被拆偏旁
    /// （实测 2003×160：1x 拆 4 处，2x 全对）。
    #[test]
    fn wide_image_is_still_scaled_up() {
        super::com::ensure().expect("COM 初始化");
        let pixmap = Pixmap::new(2003, 160).expect("pixmap");
        let bmp = bitmap::to_software_bitmap(&pixmap, 10000).expect("应能转位图");
        assert_eq!(bmp.PixelWidth().expect("w"), 4006, "宽图应放大 2 倍");
        assert_eq!(bmp.PixelHeight().expect("h"), 320);
    }

    /// 大选区不放大（像素预算）：4K 全屏选区 2 倍会吃掉 130MB+。
    #[test]
    fn huge_selection_is_not_scaled() {
        super::com::ensure().expect("COM 初始化");
        let pixmap = Pixmap::new(3840, 2160).expect("pixmap");
        let bmp = bitmap::to_software_bitmap(&pixmap, 10000).expect("4K 选区不放大后应能转位图");
        assert_eq!(bmp.PixelWidth().expect("w"), 3840, "4K 选区不应放大");
    }

    /// 空图直接拒绝（不调用引擎）。
    #[test]
    fn rejects_empty_bitmap() {
        let pixmap = Pixmap::new(1, 1).expect("pixmap");
        assert!(matches!(
            bitmap::to_software_bitmap(&pixmap, 0),
            Err(Error::TooLarge { .. }) | Err(Error::Failed(_))
        ));
    }
}
