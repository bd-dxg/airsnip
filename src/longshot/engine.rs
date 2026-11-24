// SPDX-License-Identifier: GPL-3.0-only
//! 长截图拼接引擎（LNG-1/4/5/6/10）：数据面，维护拼接画布、固定页眉/页脚、
//! 高度上限与撤销段历史。纯逻辑，不依赖窗口（滚动由 `scroll` 模块负责）。
//!
//! 帧流时序：`seed(首帧)` 仅缓冲；此后每帧 `push(frame, h)` 内部用上一帧做
//! 固定区域检测与条带定位。首帧直接入画布，页眉/页脚留到**第一次真实滚动**时
//! 再检测（第二帧通常还没滚，两帧完全相同、检测不出东西，会把页眉高判成 0）。
//!
//! `prev` 始终是「最后一次成功拼入画布的帧」：定位失败 / 无位移都不更新它，
//! 否则中间那段内容会永久丢掉（issue #157 / #196）。

use super::rows::RowBuffer;
use super::stitch::{self, Params, Signs};

/// 检测粘滞侧栏所需的最小滚动位移（像素）：位移太小时「静止列」与「滚动列」分不开。
const MIN_MASK_SHIFT: u32 = 16;

/// 连续多少拍「画面在变却定位失败」就认定抓帧有问题（≈0.25s，与状态条一拍 60ms 对齐）。
const FAIL_CHANGED_HINT: u32 = 4;
/// 「画面在变」的门槛：同位置行不一致的占比。
const FAIL_CHANGED_MIN: f32 = 0.3;

/// 拼接画布：宽度固定为视口宽，高度随帧增长。
pub struct Canvas {
    w: u32,
    /// 内容区画布（不含固定页眉/页脚）。
    rows: RowBuffer,
    /// 首帧缓冲（首次 `push` 时入画布，页眉/页脚稍后检测）。
    first: Option<Vec<u8>>,
    /// 上一帧（固定区域检测与定位用）：始终是最后一次成功拼入的帧。
    prev: Vec<u8>,
    /// 上一帧的行签名（每拍只算新帧，省一半签名成本）。
    prev_sigs: Signs,
    /// 页眉/页脚是否已确认（首次出现真实滚动时检测一次）。
    fixed_done: bool,
    /// 要遮掉的静态列区间与填充色（粘滞侧栏 / 滚动条）：只在结果顶部保留一次。
    masks: Vec<(u32, u32, [u8; 4])>,
    /// 静态列检测已试过几次（最多攒够 3 次，避免白底页面每拍都扫全屏）。
    mask_tries: u8,
    /// 已确认的固定页眉（首帧顶部行，最终置于画布顶部）。
    header: Vec<u8>,
    header_h: u32,
    /// 已确认的固定页脚（首帧底部行，最终置于画布底部）。
    footer: Vec<u8>,
    footer_h: u32,
    params: Params,
    max_height: u32,
    /// 连续无位移帧数（LNG-6：达到上限即到底）。
    pub still_run: u32,
    /// 连续「画面在变却定位失败」的拍数：典型是半透明窗口（背景不透明度 < 100%）
    /// 抓不到可比帧，状态条据此换成更具体的提示（见 `suspect_transparency`）。
    fail_changed_run: u32,
    /// 已拼内容行数（不含页眉/页脚）。
    pub content_rows: u32,
    /// 段历史（撤销最后一段用）：(是否在顶部插入, 行数)。
    segments: Vec<(bool, u32)>,
    /// 已处理的帧数（只用于调试抽样落盘）。
    pushes: u32,
}

/// 单步结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// 成功拼入一段。`offset` 本帧相对上一帧滚动像素（>0 向下），`rows` 新增行数。
    Appended { offset: i32, rows: u32 },
    /// 无位移（到页底或用户未滚）。
    Still,
    /// 定位失败（画面变化太快 / 内容不连续）。
    Failed,
    /// 已达高度上限。
    Limit,
}

impl Canvas {
    pub fn new(w: u32, params: Params, max_height: u32) -> Self {
        Self {
            w,
            rows: RowBuffer::new(w),
            first: None,
            prev: Vec::new(),
            prev_sigs: Signs::default(),
            fixed_done: false,
            masks: Vec::new(),
            mask_tries: 0,
            header: Vec::new(),
            header_h: 0,
            footer: Vec::new(),
            footer_h: 0,
            params,
            max_height,
            still_run: 0,
            fail_changed_run: 0,
            content_rows: 0,
            segments: Vec::new(),
            pushes: 0,
        }
    }

    pub fn height(&self) -> u32 {
        self.header_h + self.content_rows + self.footer_h
    }

    /// 是否像「半透明窗口」导致抓不到可用帧：连续多拍画面在变却定位不上。
    ///
    /// 只用来换状态条的一句提示，不影响拼接行为（失败仍由调用方按 `Step::Failed` 处理）。
    /// 实测：`BitBlt` 读不到半透明窗口的内容，DDA 能读到但帧与帧时像素不可比（alpha 混合）。
    /// 机理与数据见 `docs/longshot-known-issues.md`。
    pub fn suspect_transparency(&self) -> bool {
        self.fail_changed_run >= FAIL_CHANGED_HINT
    }

    /// 缓冲首帧。画布实际在第二次推帧时建立。
    pub fn seed(&mut self, frame: &[u8]) {
        self.first = Some(frame.to_vec());
    }

    /// 处理一帧。`frame` 与上一帧同尺寸（宽 `w`、高 `h`）。
    pub fn push(&mut self, frame: &[u8], h: u32) -> Step {
        // 首帧推入：整帧先入画布。页眉/页脚留到第一次真实滚动时再检测
        //（此刻用户通常还没滚动，两帧完全相同，检测不出来）。
        if let Some(first) = self.first.take() {
            self.rows.append(&first, 0, h);
            self.content_rows = self.rows.height();
            self.prev_sigs = Signs::new(&first, self.w, h);
            self.prev = first;
        }
        self.push_inner(frame, h)
    }

    /// 首次出现真实滚动时：先按「检测出的页眉/页脚」定位，成功才采纳；
    /// 失败则退回不排除固定区域的定位。
    fn locate_first(&mut self, ns: &Signs, h: u32) -> stitch::Match {
        let p = self.params;
        let (t, b) = stitch::fixed_regions(&self.prev_sigs, ns, h, &p);
        // 单边不超过 1/3 屏高：内容大面积同色时不至于把整屏吞成页眉。
        let cap = h / 3;
        if (t > 0 || b > 0) && t <= cap && b <= cap && t + b < h {
            let m = stitch::locate(&self.prev_sigs, ns, h, t, b, &p);
            // 必须是真实位移才采纳；offset==0 多半是动画 / 同色区域导致的误检。
            if m.confident && m.offset != 0 {
                self.adopt_fixed(t, b);
                self.fixed_done = true;
                return m;
            }
        }
        let m = stitch::locate(&self.prev_sigs, ns, h, 0, 0, &p);
        if m.confident && m.offset != 0 {
            self.fixed_done = true;
        }
        m
    }

    /// 把画布（此时只有首帧）顶部 `t` 行、底部 `b` 行划为页眉/页脚。
    fn adopt_fixed(&mut self, t: u32, b: u32) {
        let stride = self.w as usize * 4;
        if t > 0 {
            self.header = self.prev[..t as usize * stride].to_vec();
            self.header_h = t;
            self.rows.remove_top(t);
        }
        if b > 0 {
            self.footer = self.prev[self.prev.len() - b as usize * stride..].to_vec();
            self.footer_h = b;
            self.rows.remove_bottom(b);
        }
        self.content_rows = self.rows.height();
    }

    fn push_inner(&mut self, frame: &[u8], h: u32) -> Step {
        let params = self.params;
        self.pushes = self.pushes.wrapping_add(1);
        let ns = Signs::new(frame, self.w, h);
        let m = if self.fixed_done {
            stitch::locate(&self.prev_sigs, &ns, h, self.header_h, self.footer_h, &params)
        } else {
            self.locate_first(&ns, h)
        };
        // 调试：每拍的定位结果、分数分母（有信息量行）与画面变化占比。
        // 文本日志常开（每拍一行、几 KB、不含画面，ADR 0046）；`chg` 同时是
        // 「画面变了却判没动」的判据，所以总得算。
        let chg = self.prev_sigs.changed_ratio(&ns, h);
        super::debug::log(&format!(
            "push: offset={} start={} end={} confident={} fixed_top={} fixed_bottom={} info={} changed={:.0}%",
            m.offset,
            m.start,
            m.end,
            m.confident,
            self.header_h,
            self.footer_h,
            self.prev_sigs.info_rows(h),
            chg * 100.0
        ));
        // 每 25 拍的抽样落盘成本高（全屏 PNG 编码几十 ms/张），只在开发者的
        // `AIRSNIP_DEBUG` 下做；异常帧落盘不受影响。
        if super::debug::verbose() && self.pushes.is_multiple_of(25) {
            super::debug::dump_pair("seq", &self.prev, frame, self.w, h);
        }
        // 画面明显变了却判「没动」：把两帧落盘，区分「页面真没滚」与「抓到的是旧帧」。
        if m.confident && m.offset == 0 && chg > 0.5 {
            super::debug::log("still-but-changed: 画面在变但判无位移"); // i18n:keep（日志，ADR 0047）
            super::debug::dump_pair("still", &self.prev, frame, self.w, h);
        }
        if !m.confident {
            super::debug::dump_pair("failed", &self.prev, frame, self.w, h);
        }
        // 不确定：保持 prev 不变（它必须始终是「最后一次成功拼入的帧」），
        // 用户下一拍继续滚动或动画停在某个状态时还能接上。
        if !m.confident {
            // 画面一直在变却一直定位不上：这不是「用户没滚」，多半是抓到的帧根本不可比
            // （半透明窗口的内容由系统合成，见 docs/longshot-known-issues.md）。
            if chg > FAIL_CHANGED_MIN {
                self.fail_changed_run += 1;
            } else {
                self.fail_changed_run = 0;
            }
            return Step::Failed;
        }
        // 无位移：同样不更新 prev，避免动画把参考帧一点点带偏。
        if m.offset == 0 {
            self.still_run += 1;
            self.fail_changed_run = 0;
            return Step::Still;
        }
        self.still_run = 0;
        self.fail_changed_run = 0;

        // 追加/前置新增内容（m.start/m.end 为全天坐标，已剔除固定区域）。
        let rows = (m.end - m.start) as u32;
        if rows == 0 {
            self.prev = frame.to_vec();
            self.prev_sigs = ns;
            return Step::Still;
        }
        if self.content_rows + rows > self.max_height {
            self.prev = frame.to_vec();
            self.prev_sigs = ns;
            return Step::Limit;
        }
        let prepend = m.offset < 0;
        // 粘滞侧栏（论坛的话题侧栏、时间轴、聊天窗左右栏）：真实滚过一大段后检测一次，
        // 之后拼入的行把那几列用相邻背景填平，否则侧栏会随每一段重复一遍。
        if self.masks.is_empty() && self.mask_tries < 3 && m.offset.unsigned_abs() >= MIN_MASK_SHIFT {
            self.mask_tries += 1;
            self.masks = stitch::static_columns(&self.prev, frame, self.w, h, self.header_h, self.footer_h);
        }
        if prepend {
            self.rows.prepend_masked(frame, m.start, m.end, &self.masks);
        } else {
            self.rows.append_masked(frame, m.start, m.end, &self.masks);
        }
        self.content_rows = self.rows.height();
        self.segments.push((prepend, rows));
        self.prev = frame.to_vec();
        self.prev_sigs = ns;
        Step::Appended { offset: m.offset, rows }
    }

    /// 是否有可撤销的段。
    pub fn can_undo(&self) -> bool {
        !self.segments.is_empty()
    }

    /// 撤销最后一段内容（LNG-10 失败修正入口）。
    pub fn undo_last(&mut self) -> bool {
        let Some((prepend, rows)) = self.segments.pop() else {
            return false;
        };
        if prepend {
            self.rows.remove_top(rows);
        } else {
            self.rows.remove_bottom(rows);
        }
        self.content_rows = self.rows.height();
        true
    }

    /// 取整图第 `y` 行（含页眉与页脚）。
    fn row_at(&self, y: u32) -> Option<&[u8]> {
        let stride = self.w as usize * 4;
        if y < self.header_h {
            let o = y as usize * stride;
            return self.header.get(o..o + stride);
        }
        let y = y - self.header_h;
        if y < self.content_rows {
            return Some(self.rows.row(y));
        }
        let y = y - self.content_rows;
        if y < self.footer_h {
            let o = y as usize * stride;
            return self.footer.get(o..o + stride);
        }
        None
    }

    /// 生成整图缩略图（LNG-3 预览）：最近邻抽样，等比缩放到 `max_w × max_h` 内。
    /// 成本只与目标尺寸相关，与拼接图高度无关（不先合成整张大图）。
    pub fn thumbnail(&self, max_w: u32, max_h: u32) -> Option<tiny_skia::Pixmap> {
        let (sw, sh) = (self.w, self.height());
        if sw == 0 || sh == 0 || max_w == 0 || max_h == 0 {
            return None;
        }
        // 等比缩放：宽高都不超过上限
        let scale = (max_w as f32 / sw as f32).min(max_h as f32 / sh as f32);
        let tw = ((sw as f32 * scale) as u32).clamp(1, max_w);
        let th = ((sh as f32 * scale) as u32).clamp(1, max_h);
        let mut out = vec![0u8; (tw * th * 4) as usize];
        let dst_stride = tw as usize * 4;
        for y in 0..th {
            let sy = (y as u64 * sh as u64 / th as u64) as u32;
            let Some(row) = self.row_at(sy) else { continue };
            let dst = y as usize * dst_stride;
            for x in 0..tw {
                let sx = (x as u64 * sw as u64 / tw as u64) as usize * 4;
                let d = dst + x as usize * 4;
                if let Some(px) = row.get(sx..sx + 4) {
                    out[d..d + 4].copy_from_slice(px);
                }
            }
        }
        tiny_skia::Pixmap::from_vec(out, tiny_skia::IntSize::from_wh(tw, th)?)
    }

    /// 导出画布像素（页眉 + 内容 + 页脚）。
    pub fn to_pixmap(&self) -> Option<tiny_skia::Pixmap> {
        let mut out = RowBuffer::new(self.w);
        if self.header_h > 0 {
            let mut hdr = RowBuffer::new(self.w);
            hdr.append(&self.header, 0, self.header_h);
            out.append_buffer(&hdr);
        }
        out.append_buffer(&self.rows);
        if self.footer_h > 0 {
            let mut ftr = RowBuffer::new(self.w);
            ftr.append(&self.footer, 0, self.footer_h);
            out.append_buffer(&ftr);
        }
        out.into_pixmap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params {
            min_ratio: 0.6,
            min_fixed: 8,
        }
    }

    /// 带 GIF 头像的论坛帖子页（issue #196 场景）：每个帖子左上角一块头像，
    /// 头像区域每帧都在变（GIF 动画），帖子沿页面向下重复出现。
    /// 返回（页面像素, 按帧号生成视口帧的闭包参数）。
    fn forum_page(w: u32, total: u32) -> Vec<u8> {
        let mut data = vec![0u8; (w * total * 4) as usize];
        for y in 0..total {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                // 正文：逐行可区分（文字行）
                data[o] = (y & 0xff) as u8;
                data[o + 1] = ((y >> 8) & 0xff) as u8;
                data[o + 2] = ((x * 3 + y * 7) & 0xff) as u8;
                data[o + 3] = 255;
            }
        }
        data
    }

    /// 取视口帧并把「头像块」按帧号改写（模拟 GIF 每一帧都不同）。
    fn forum_frame(page: &[u8], w: u32, y0: u32, h: u32, frame_no: u32) -> Vec<u8> {
        let stride = w as usize * 4;
        let mut out = page[y0 as usize * stride..(y0 + h) as usize * stride].to_vec();
        // 每 50 行一个帖子，帖子顶部 16 行 x [4, 40) 是头像；头像按帧号翻色。
        for y in 0..h {
            let global = y0 + y;
            if global % 50 >= 16 {
                continue;
            }
            for x in 4..40 {
                let o = (y as usize * w as usize + x as usize) * 4;
                out[o] = ((x * 11 + frame_no * 37) & 0xff) as u8;
                out[o + 1] = ((y * 13 + frame_no * 53) & 0xff) as u8;
            }
        }
        out
    }

    /// 滚动穿过一堆动画头像（issue #196）：只要头像只占行内少数分段，就不该妨碍定位。
    #[test]
    fn canvas_scrolls_through_animated_avatars() {
        let w = 200u32;
        let view_h = 100u32;
        let step = 70u32; // 重叠 30 行（≥ 内容区 1/5）
        let page = forum_page(w, 700);
        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&forum_frame(&page, w, 0, view_h, 0));
        for k in 1..=6u32 {
            let f = forum_frame(&page, w, k * step, view_h, k);
            assert_eq!(
                c.push(&f, view_h),
                Step::Appended {
                    offset: step as i32,
                    rows: step
                },
                "第 {k} 次滚动应拼入 {step} 行（头像动画不应妨碍定位）"
            );
        }
        assert_eq!(c.height(), view_h + 6 * step);
    }

    /// 造完整页面：固定页眉 hdr 行 + 内容 total 行 + 固定页脚 ftr 行。
    /// 行内容按全局行号着色，页眉/页脚用特殊色便于断言。
    fn make_page(w: u32, hdr: u32, content: u32, ftr: u32) -> Vec<u8> {
        let h = hdr + content + ftr;
        let mut data = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let is_hdr = y < hdr;
            let is_ftr = y >= hdr + content;
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                if is_hdr {
                    data[o] = 200;
                    data[o + 1] = 201;
                    data[o + 2] = (x * 5) as u8;
                } else if is_ftr {
                    data[o] = 202;
                    data[o + 1] = 203;
                    data[o + 2] = (x * 7) as u8;
                } else {
                    data[o] = (y & 0xff) as u8;
                    data[o + 1] = ((y >> 8) & 0xff) as u8;
                    data[o + 2] = (x * 3) as u8;
                }
                data[o + 3] = 255;
            }
        }
        data
    }

    /// 简化：把「视口内、页眉页脚固定的完整页面窗口」抽成页眉+内容窗口+页脚。
    fn frame_simple(page: &[u8], w: u32, hdr: u32, ftr: u32, content_y0: u32, view_h: u32) -> Vec<u8> {
        let stride = w as usize * 4;
        let content_win = (view_h - hdr - ftr) as usize;
        let mut out = vec![0u8; (w * view_h) as usize * 4];
        // 页眉
        out[..hdr as usize * stride].copy_from_slice(&page[..hdr as usize * stride]);
        // 内容窗口
        let c_start = (hdr + content_y0) as usize * stride;
        let c_len = content_win * stride;
        out[hdr as usize * stride..(hdr as usize + content_win) * stride]
            .copy_from_slice(&page[c_start..c_start + c_len]);
        // 页脚
        let f_start = (page.len() / stride - ftr as usize) * stride;
        out[(view_h - ftr) as usize * stride..].copy_from_slice(&page[f_start..]);
        out
    }

    #[test]
    fn canvas_stitches_down_scroll_with_fixed_header_footer() {
        let w = 64u32;
        let hdr = 10u32;
        let ftr = 8u32;
        let content_total = 120u32;
        let view_h = hdr + 60 + ftr; // 内容视口 60 行
        let page = make_page(w, hdr, content_total, ftr);

        let mut c = Canvas::new(w, params(), 30000);
        // 首帧：内容窗口从 0 开始
        let f0 = frame_simple(&page, w, hdr, ftr, 0, view_h);
        c.seed(&f0);
        // 第二帧：内容下滚 10 行
        let f1 = frame_simple(&page, w, hdr, ftr, 10, view_h);
        let s1 = c.push(&f1, view_h);
        assert_eq!(s1, Step::Appended { offset: 10, rows: 10 });
        // 第三帧：再多滚 20 行
        let f2 = frame_simple(&page, w, hdr, ftr, 30, view_h);
        let s2 = c.push(&f2, view_h);
        assert_eq!(s2, Step::Appended { offset: 20, rows: 20 });

        // 最终高度 = 页眉 10 + 内容 90 + 页脚 8
        assert_eq!(c.height(), 10 + 60 + 30 + 8);
        let img = c.to_pixmap().expect("画布可转像素图");
        assert_eq!(img.height(), 10 + 60 + 30 + 8);
        // 分段比对：页眉 = 页面 0..10，内容 = 页面 10..100，页脚 = 页面末尾 130..138
        for y in 0..10 {
            let o = y as usize * w as usize * 4;
            assert_eq!(
                &img.data()[o..o + w as usize * 4],
                &page[o..o + w as usize * 4],
                "页眉第 {y} 行"
            );
        }
        for y in 10..100 {
            let o = y as usize * w as usize * 4;
            assert_eq!(
                &img.data()[o..o + w as usize * 4],
                &page[o..o + w as usize * 4],
                "内容第 {y} 行"
            );
        }
        for y in 0..8 {
            let o = (100 + y) as usize * w as usize * 4;
            let po = (130 + y) as usize * w as usize * 4;
            assert_eq!(
                &img.data()[o..o + w as usize * 4],
                &page[po..po + w as usize * 4],
                "页脚第 {y} 行"
            );
        }
    }

    #[test]
    fn canvas_undo_removes_last_segment() {
        let w = 64u32;
        let hdr = 0u32;
        let ftr = 0u32;
        let content_total = 200u32;
        let view_h = 60u32;
        let page = make_page(w, hdr, content_total, ftr);

        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&view(&page, w, 0, view_h));
        let s1 = c.push(&view(&page, w, 10, view_h), view_h);
        assert_eq!(s1, Step::Appended { offset: 10, rows: 10 });
        assert_eq!(c.height(), 70);
        assert!(c.undo_last());
        assert_eq!(c.height(), 60);
        // 无可撤销段时返回 false
        assert!(!c.undo_last());
    }

    #[test]
    fn canvas_prepends_on_up_scroll() {
        let w = 64u32;
        let hdr = 0u32;
        let ftr = 0u32;
        let content_total = 200u32;
        let view_h = 60u32;
        let page = make_page(w, hdr, content_total, ftr);

        let mut c = Canvas::new(w, params(), 30000);
        // 起点在第 90 行；向上滚 10 行 → 内容窗口 [80,140)
        c.seed(&view(&page, w, 90, view_h));
        let s1 = c.push(&view(&page, w, 80, view_h), view_h);
        assert_eq!(s1, Step::Appended { offset: -10, rows: 10 });
        // 画布 = 上滚前移：起点 80，共 10 + 60 = 70 行
        assert_eq!(c.height(), 70);
        let img = c.to_pixmap().unwrap();
        for y in 0..img.height() {
            let o = y as usize * w as usize * 4;
            let po = (80 + y) as usize * w as usize * 4;
            assert_eq!(
                &img.data()[o..o + w as usize * 4],
                &page[po..po + w as usize * 4],
                "第 {y} 行"
            );
        }
    }

    #[test]
    fn canvas_keeps_reference_frame_on_failure() {
        // 定位失败时 prev 不能换成失败帧，否则失败帧之前的内容永久丢掉（issue #196）。
        let w = 64u32;
        let view_h = 60u32;
        let page = make_page(w, 0, 200, 0);
        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&view(&page, w, 0, view_h));
        // 第二帧与首帧相同：静止
        assert_eq!(c.push(&view(&page, w, 0, view_h), view_h), Step::Still);
        // 一帧彻底对不上的画面（模拟动画 / 模糊帧）
        let noise: Vec<u8> = (0..(w * view_h * 4) as usize).map(|i| (i * 37 % 251) as u8).collect();
        assert_eq!(c.push(&noise, view_h), Step::Failed);
        assert_eq!(c.height(), 60, "失败不应改变画布");
        // 下一帧真滚了 10 行：还能接在首帧后面
        assert_eq!(
            c.push(&view(&page, w, 10, view_h), view_h),
            Step::Appended { offset: 10, rows: 10 }
        );
        assert_eq!(c.height(), 70);
    }

    #[test]
    fn canvas_detects_fixed_regions_on_first_real_scroll() {
        // 第二帧往往还没滚动（两帧相同），页眉/页脚必须留到首次真实滚动时检测。
        let w = 64u32;
        let hdr = 10u32;
        let ftr = 8u32;
        let content_total = 120u32;
        let view_h = hdr + 60 + ftr;
        let page = make_page(w, hdr, content_total, ftr);
        let mut c = Canvas::new(w, params(), 30000);
        let f0 = frame_simple(&page, w, hdr, ftr, 0, view_h);
        c.seed(&f0);
        assert_eq!(c.push(&f0, view_h), Step::Still, "未滚动时应静止");
        assert_eq!(
            c.push(&frame_simple(&page, w, hdr, ftr, 10, view_h), view_h),
            Step::Appended { offset: 10, rows: 10 }
        );
        // 页眉/页脚已拆出画布：10 + 60 + 10 + 8
        assert_eq!(c.height(), 10 + 60 + 10 + 8);
        let img = c.to_pixmap().expect("画布可转像素图");
        let stride = w as usize * 4;
        for y in 0..10 {
            let o = y * stride;
            assert_eq!(&img.data()[o..o + stride], &page[o..o + stride], "页眉第 {y} 行");
        }
    }

    /// 左侧粘滞侧栏（论坛话题页、聊天窗左右栏）：只应出现在结果顶部一次，
    /// 后面拼入的段落里要填平，不能每拼一段就重复一遍侧栏。
    #[test]
    fn canvas_blanks_sticky_sidebar_in_later_segments() {
        let w = 200u32;
        let rail = 40u32;
        let view_h = 100u32;
        let step = 70u32;
        let page = make_page(w, 0, 500, 0);
        let frame = |off: u32| -> Vec<u8> {
            let mut f = view(&page, w, off, view_h);
            // 侧栏：背景 [90,91,92]，每 8 行有一块「文字」
            for y in 0..view_h {
                for x in 0..rail {
                    let o = ((y * w + x) * 4) as usize;
                    let text = y % 8 == 0 && (10..25).contains(&x);
                    let v = if text { 20 } else { 90 };
                    f[o] = v;
                    f[o + 1] = v + 1;
                    f[o + 2] = v + 2;
                    f[o + 3] = 255;
                }
            }
            f
        };
        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&frame(0));
        for k in 1..=3u32 {
            assert_eq!(
                c.push(&frame(k * step), view_h),
                Step::Appended {
                    offset: step as i32,
                    rows: step
                },
                "第 {k} 次滚动"
            );
        }
        let img = c.to_pixmap().expect("画布可转像素图");
        let stride = w as usize * 4;
        // 顶部一屏保留侧栏（含文字：x=15 处是文字色）
        assert_eq!(&img.data()[15 * 4..15 * 4 + 3], &[20, 21, 22], "顶部应保留侧栏文字");
        // 拼入的行：侧栏列被填成侧栏自己的背景色，不再出现文字
        for y in view_h..(view_h + 3 * step) {
            let row = &img.data()[y as usize * stride..(y as usize + 1) * stride];
            for x in 0..rail as usize {
                assert_eq!(
                    &row[x * 4..x * 4 + 3],
                    &[90, 91, 92],
                    "第 {y} 行第 {x} 列应是侧栏背景色"
                );
            }
        }
    }

    /// 页面的主内容区拼完后（左侧无粘滞侧栏）不受遮罩影响。
    #[test]
    fn canvas_keeps_full_width_when_nothing_is_sticky() {
        let w = 200u32;
        let view_h = 100u32;
        let page = make_page(w, 0, 300, 0);
        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&view(&page, w, 0, view_h));
        assert_eq!(
            c.push(&view(&page, w, 70, view_h), view_h),
            Step::Appended { offset: 70, rows: 70 }
        );
        let img = c.to_pixmap().unwrap();
        let stride = w as usize * 4;
        // 拼入的第一行（画布 y=view_h）应对应页面第 100 行，逐像素一致
        let o = view_h as usize * stride;
        let p = 100usize * stride;
        assert_eq!(&img.data()[o..o + stride], &page[p..p + stride]);
    }

    #[test]
    fn canvas_respects_height_limit() {
        let w = 64u32;
        let hdr = 0u32;
        let ftr = 0u32;
        let content_total = 200u32;
        let view_h = 60u32;
        let page = make_page(w, hdr, content_total, ftr);
        let mut c = Canvas::new(w, params(), 85); // 上限 85
        c.seed(&view(&page, w, 0, view_h));
        assert_eq!(
            c.push(&view(&page, w, 10, view_h), view_h),
            Step::Appended { offset: 10, rows: 10 }
        ); // 70
        assert_eq!(
            c.push(&view(&page, w, 20, view_h), view_h),
            Step::Appended { offset: 10, rows: 10 }
        ); // 80
        assert_eq!(c.push(&view(&page, w, 30, view_h), view_h), Step::Limit); // 90 > 85
        assert_eq!(c.height(), 80);
    }

    fn view(page: &[u8], w: u32, y0: u32, h: u32) -> Vec<u8> {
        let stride = w as usize * 4;
        page[y0 as usize * stride..(y0 + h) as usize * stride].to_vec()
    }

    /// 预览缩略图（LNG-3）：尺寸不超上限、保持长宽比、内容可区分。
    #[test]
    fn thumbnail_scales_within_bounds() {
        let w = 64u32;
        let view_h = 60u32;
        let page = make_page(w, 0, 200, 0);
        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&view(&page, w, 0, view_h));
        // 每次滚 20 行（不超过内容区一半，locate 才能定位）
        c.push(&view(&page, w, 20, view_h), view_h);
        c.push(&view(&page, w, 40, view_h), view_h);
        assert_eq!(c.height(), 100);

        // 高度上限 90 → 按高度等比缩（64×100 → 57×90）
        let t = c.thumbnail(1000, 90).expect("可生成缩略图");
        assert!(t.width() <= 1000 && t.height() <= 90, "不得超出上限");
        assert_eq!(t.height(), 90, "应缩到高度上限");
        assert!(
            (t.width() as i32 - 57).abs() <= 1,
            "宽度应保持长宽比，实际 {}",
            t.width()
        );

        // 宽度受限时按宽度缩（64×100 → 16×25）
        let t2 = c.thumbnail(16, 1000).expect("可生成缩略图");
        assert_eq!((t2.width(), t2.height()), (16, 25), "应等比缩放到宽度上限");

        // 内容不透明（页面每行 alpha=255）
        assert_eq!(t.data()[3], 255, "缩略图像素应不透明");
    }

    /// 完全不相关的帧：用于模拟「画面在变但无法定位」（半透明窗口的实际现象）。
    fn noise_frame(w: u32, h: u32, seed: u32) -> Vec<u8> {
        let mut v = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                let t = y * 31 + x * 17 + seed * 101;
                v[o] = (t & 0xff) as u8;
                v[o + 1] = ((t >> 3) & 0xff) as u8;
                v[o + 2] = (((t >> 5) ^ (y * 13)) & 0xff) as u8;
                v[o + 3] = 255;
            }
        }
        v
    }

    /// 连续「画面在变却定位失败」多拍 → 判定为抓帧问题（半透明窗口）；
    /// 一旦真的拼入一段，提示状态要立刻复位。
    #[test]
    fn suspect_transparency_needs_repeated_failed_frames() {
        let (w, view_h) = (64u32, 40u32);
        let page = make_page(w, 0, 200, 0);
        let mut c = Canvas::new(w, params(), 30000);
        c.seed(&view(&page, w, 0, view_h));
        c.push(&view(&page, w, 0, view_h), view_h);
        assert!(!c.suspect_transparency(), "静止帧不应误报");

        for i in 0..4u32 {
            assert_eq!(c.push(&noise_frame(w, view_h, i), view_h), Step::Failed);
        }
        assert!(c.suspect_transparency(), "连续失败且画面在变应判为抓帧问题");

        // 恢复：能正常拼入一段后立刻清掉
        let mut c2 = Canvas::new(w, params(), 30000);
        c2.seed(&view(&page, w, 0, view_h));
        c2.push(&view(&page, w, 0, view_h), view_h);
        for i in 0..4u32 {
            c2.push(&noise_frame(w, view_h, i), view_h);
        }
        assert!(c2.suspect_transparency());
        assert!(matches!(
            c2.push(&view(&page, w, 20, view_h), view_h),
            Step::Appended { .. }
        ));
        assert!(!c2.suspect_transparency(), "成功拼入后应复位");
    }
}
