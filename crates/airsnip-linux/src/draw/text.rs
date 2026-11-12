// SPDX-License-Identifier: GPL-3.0-only
//! Linux 侧文本引擎：cosmic-text（Harfbuzz 级整形 + Swash 栅格化）。
//!
//! 接进 core 的 `TextSink` 接口（ADR 0034），于是 `draw_objects` 这套绘制代码
//! 两个前端共用一份。字体查找走 fontconfig（`FontSystem::new()`），中文由
//! 系统回退链解决（思源黑体 / Noto CJK）。
//!
//! `TextSink` 的方法签名是 `&self`，而 cosmic-text 需要 `&mut FontSystem`，
//! 所以内部用 `RefCell` 包一层；UI 是单线程的，不存在跨线程争用。

use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style as TextStyle, SwashCache, Weight};
use tiny_skia::{Pixmap, PremultipliedColorU8, Transform};

use airsnip_core::draw::text::{TextSink, TextSpec};
use airsnip_core::editor::Point;

struct Inner {
    font_system: FontSystem,
    cache: SwashCache,
}

/// cosmic-text 文本引擎。
pub struct CosmicText {
    inner: RefCell<Inner>,
}

impl CosmicText {
    fn new() -> Self {
        Self {
            inner: RefCell::new(Inner {
                font_system: FontSystem::new(),
                cache: SwashCache::new(),
            }),
        }
    }

    fn attrs(spec: &TextSpec) -> Attrs<'_> {
        let family = if spec.family.is_empty() {
            Family::SansSerif
        } else {
            Family::Name(&spec.family)
        };
        Attrs::new()
            .family(family)
            .weight(if spec.bold { Weight::BOLD } else { Weight::NORMAL })
            .style(if spec.italic {
                TextStyle::Italic
            } else {
                TextStyle::Normal
            })
    }

    /// 整形一版文本；`width` 为 Some 时按该宽度软换行。
    fn shape(text: &str, spec: &TextSpec, inner: &mut Inner, width: Option<f32>) -> Buffer {
        let metrics = Metrics::new(spec.size, spec.size * 1.25);
        let mut buffer = Buffer::new(&mut inner.font_system, metrics);
        buffer.set_size(width, None);
        buffer.set_text(text, &Self::attrs(spec), Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut inner.font_system, false);
        buffer
    }
}

/// 全局单例：`TextSink` 的方法签名是 `&self`，绘制是单线程 UI 流程，
/// 用 `Mutex` 包一层即可满足 `Sync`（不会嵌套加锁：只在最外层调用处取一次）。
pub fn with_engine<R>(f: impl FnOnce(&CosmicText) -> R) -> R {
    static ENGINE: OnceLock<Mutex<CosmicText>> = OnceLock::new();
    let m = ENGINE.get_or_init(|| Mutex::new(CosmicText::new()));
    let guard = m.lock().unwrap_or_else(|e| e.into_inner());
    f(&guard)
}

impl TextSink for CosmicText {
    fn measure(&self, text: &str, spec: &TextSpec) -> (f32, f32) {
        let mut inner = self.inner.borrow_mut();
        let buffer = CosmicText::shape(text, spec, &mut inner, None);
        let mut w: f32 = 0.0;
        let mut h: f32 = 0.0;
        for run in buffer.layout_runs() {
            w = w.max(run.line_w);
            h = h.max(run.line_y + run.line_height);
        }
        (w, h)
    }

    fn draw(&self, pixmap: &mut Pixmap, pos: Point, text: &str, spec: &TextSpec, color: [u8; 4], tr: Transform) {
        let mut inner = self.inner.borrow_mut();
        let buffer = CosmicText::shape(text, spec, &mut inner, None);
        let Inner { font_system, cache } = &mut *inner;
        let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
        let base = cosmic_text::Color::rgba(color[0], color[1], color[2], color[3]);

        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let physical = glyph.physical((pos.x, pos.y + run.line_y), 1.0);
                // tiny-skia 的 map_point 是「传入可变点就地变换」
                let mut origin = tiny_skia::Point { x: 0.0, y: 0.0 };
                tr.map_point(&mut origin);
                let (dx, dy) = (origin.x.round() as i32, origin.y.round() as i32);
                let pixels = pixmap.pixels_mut();
                cache.with_pixels(font_system, physical.cache_key, base, |x, y, c| {
                    let gx = physical.x + x + dx;
                    let gy = physical.y + y + dy;
                    if gx < 0 || gy < 0 || gx >= pw || gy >= ph {
                        return;
                    }
                    let idx = (gy * pw + gx) as usize;
                    let dst = pixels[idx];
                    let a = c.a() as u32;
                    if a == 0 {
                        return;
                    }
                    let mix = |s: u8, d: u8| -> u8 { ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8 };
                    if let Some(px) = PremultipliedColorU8::from_rgba(
                        mix(c.r(), dst.red()),
                        mix(c.g(), dst.green()),
                        mix(c.b(), dst.blue()),
                        255,
                    ) {
                        pixels[idx] = px;
                    }
                });
            }
        }
    }
}
