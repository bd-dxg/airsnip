// SPDX-License-Identifier: GPL-3.0-only
//! 抓屏（`zwlr_screencopy_manager_v1`）：逐输出取画面，再合成整屏底图。
//!
//! 与 Windows 版 `capture::capture_virtual_screen` 的位置对应，但机制完全不同：
//! Wayland 没有「虚拟桌面位图」，只能按输出抓，再按逻辑位置拼（ADR 0034 §3）。
//!
//! **已知边界**：多输出**缩放不一致**时，按「逻辑坐标 × 该输出缩放」放置只是近似；
//! 缩放一致（含全 1）时精确。Windows 版的多屏混合 DPI 也尚未实测，两边一致。

pub mod shm;

use wayland_client::Dispatch;
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_shm::Format;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{self as frame, ZwlrScreencopyFrameV1};

use crate::Error;
use crate::canvas::Canvas;
use crate::wayland::{Backend, Session};

/// 单帧抓取的进度状态（由 `Dispatch` 填，抓完即读）。
#[derive(Default)]
pub struct Progress {
    /// `buffer` 事件报告的 (格式, 宽, 高, 行长)。
    pub buffer: Option<(Format, u32, u32, u32)>,
    /// `flags.y_invert`：像素上下颠倒。
    pub y_invert: bool,
    pub ready: bool,
    pub failed: bool,
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrScreencopyFrameV1,
        event: frame::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            frame::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                if let Ok(f) = format.into_result() {
                    state.frame.buffer = Some((f, width, height, stride));
                }
            }
            frame::Event::Flags { flags } => {
                state.frame.y_invert = flags
                    .into_result()
                    .map(|f| f.contains(frame::Flags::YInvert))
                    .unwrap_or(false);
            }
            frame::Event::Ready { .. } => state.frame.ready = true,
            frame::Event::Failed => state.frame.failed = true,
            _ => {}
        }
    }
}

/// 一个输出的抓屏结果与它在画布上的位置（不持有像素，像素直接写进合成画布）。
pub struct Shot {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// 抓取全部输出并合成一张覆盖整个输出的画布。
///
/// 流程：先用输出信息算出包围盒（位置在抓屏前就已知）→ 分配一张画布 →
/// 逐个输出抓屏并**直接**写进画布对应位置。中间不再存每屏的临时位图，
/// 全屏只多一次像素拷贝（Windows 版抓屏后也要拷贝一次）。
pub fn capture_all(s: &mut Session) -> Result<(Canvas, Vec<Shot>), Error> {
    if s.backend.outputs.is_empty() {
        return Err("合成器没有报任何 wl_output".into());
    }
    let t0 = std::time::Instant::now();
    // 1. 包围盒（逻辑坐标 → 物理像素）
    let mut x0 = i32::MAX;
    let mut y0 = i32::MAX;
    let mut x1 = i32::MIN;
    let mut y1 = i32::MIN;
    for info in &s.backend.outputs {
        let (px, py) = info.canvas_pos();
        let (w, h) = info.buffer_size();
        x0 = x0.min(px);
        y0 = y0.min(py);
        x1 = x1.max(px + w);
        y1 = y1.max(py + h);
    }
    let mut canvas = Canvas::new((x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32);
    let t_alloc = t0.elapsed();

    // 2. 逐输出抓屏并写入画布
    let mut shots = Vec::new();
    for idx in 0..s.backend.outputs.len() {
        shots.push(capture_into(s, idx, &mut canvas, x0, y0)?);
    }
    if std::env::var("AIRSNIP_TIMING").is_ok() {
        eprintln!(
            "[timing] 分配画布 {:?}｜抓屏+合成 {:?}｜总计 {:?}",
            t_alloc,
            t0.elapsed() - t_alloc,
            t0.elapsed()
        );
    }
    Ok((canvas, shots))
}

/// 抓单个输出并直接写进合成画布。
fn capture_into(s: &mut Session, idx: usize, canvas: &mut Canvas, ox: i32, oy: i32) -> Result<Shot, Error> {
    let info = s.backend.outputs[idx].clone();
    let (bw, bh) = info.buffer_size();
    let (cx, cy) = info.canvas_pos();
    let shm = s.backend.shm.clone().ok_or("合成器没有提供 wl_shm")?;
    let mgr = s
        .backend
        .screencopy
        .clone()
        .ok_or("合成器不支持 zwlr_screencopy_manager_v1（niri / sway / Hyprland / KDE 支持）")?;
    let output = info.output.clone().ok_or("输出代理缺失")?;

    let buf = shm::create(&shm, &s.qh, bw, bh)?;
    s.backend.frame = Progress::default();
    // overlay_cursor = 0：不要把鼠标指针画进底图（与 Windows 版 BitBlt 行为一致）。
    let handle = mgr.capture_output(0, &output, &s.qh, ());
    handle.copy(&buf.buffer);
    s.flush()?;

    // 等 ready / failed。合成器正常时 1–2 个事件就到；上限只是防卡死。
    let mut spins = 0;
    while !s.backend.frame.ready && !s.backend.frame.failed {
        spins += 1;
        if spins > 4096 {
            handle.destroy();
            return Err(format!("抓取输出 {} 超时", info.label()).into());
        }
        s.blocking_dispatch()?;
    }
    handle.destroy();
    if s.backend.frame.failed {
        return Err(format!("合成器拒绝抓取输出 {}", info.label()).into());
    }

    let (format, fw, fh, fstride) = s
        .backend
        .frame
        .buffer
        .ok_or("合成器没有发 buffer 事件，无法解释像素格式")?;
    let (fw, fh, fstride) = (fw as i32, fh as i32, fstride as i32);
    if fw != bw || fh != bh {
        return Err(format!("缓冲区尺寸不一致：请求 {bw}×{bh}，合成器交付 {fw}×{fh}").into());
    }
    let xrgb = match format {
        Format::Xrgb8888 => true,
        Format::Xbgr8888 => false,
        other => return Err(format!("不支持的像素格式：{other:?}").into()),
    };
    if fstride as usize * fh as usize > buf.len() {
        return Err("交付行长超出缓冲区".into());
    }

    // 直接写进合成画布（不先复制一份每屏位图，见 `capture_all` 说明）。
    canvas.blit(
        cx - ox,
        cy - oy,
        fw as u32,
        fh as u32,
        fstride as usize,
        buf.pixels(),
        xrgb,
        s.backend.frame.y_invert,
    );
    Ok(Shot {
        name: info.label(),
        x: cx,
        y: cy,
        w: fw as u32,
        h: fh as u32,
    })
}
