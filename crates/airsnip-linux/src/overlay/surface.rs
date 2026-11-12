// SPDX-License-Identifier: GPL-3.0-only
//! 每个输出一块 `zwlr_layer_surface_v1`（overlay 层、全屏、独占键盘），
//! 以及它自己的双缓冲 `wl_shm` 缓冲区。
//!
//! 为什么每个输出一块：Wayland 的一个 surface 属于一个输出。Windows 版是
//! **一个** 横跨虚拟屏幕的窗口，Wayland 上只能拆成「每屏一块，各自透明贴齐」，
//! 逻辑上仍是同一个会话（选区跨屏也成立，见 `paint`）。

use std::fs::{File, OpenOptions};
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use memmap2::MmapMut;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::Layer as LayerKind;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::{
    self as layer_surface, ZwlrLayerSurfaceV1,
};

use crate::Error;
use crate::wayland::Backend;

/// 该输出在画布上的位置与缩放。
#[derive(Clone, Copy, Debug, Default)]
pub struct Geom {
    /// 画布物理坐标左上角。
    pub phys: (i32, i32),
    /// 物理像素尺寸（= 抓屏缓冲尺寸）。
    pub size: (i32, i32),
    /// 合成器缩放（逻辑 = 物理 / scale）。
    pub scale: i32,
}

impl Geom {
    pub fn scale(&self) -> i32 {
        if self.scale <= 0 { 1 } else { self.scale }
    }
}

/// 一块 shm 缓冲。
pub struct Buf {
    pub buffer: WlBuffer,
    pub map: MmapMut,
    pub released: Arc<AtomicBool>,
    pub w: i32,
    pub h: i32,
    _file: File,
}

impl Buf {
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.map[..]
    }
}

/// 一个输出上的覆盖层。
pub struct Layer {
    pub idx: usize,
    pub surface: WlSurface,
    pub layer_surface: ZwlrLayerSurfaceV1,
    pub geom: Geom,
    /// 合成器给的逻辑尺寸（configure）。
    pub logical: Option<(u32, u32)>,
    pub bufs: Vec<Buf>,
    next: usize,
}

impl Layer {
    /// 建 surface + layer surface，并提交首次 commit 等 configure。
    pub fn create(backend: &Backend, qh: &QueueHandle<Backend>, idx: usize, geom: Geom) -> Result<Self, Error> {
        let compositor = backend.compositor.clone().ok_or("合成器没有 wl_compositor")?;
        let shell = backend
            .layer_shell
            .clone()
            .ok_or("合成器不支持 zwlr_layer_shell_v1（niri / sway / Hyprland / KDE 6 支持）")?;
        let output = backend
            .outputs
            .get(idx)
            .and_then(|o| o.output.clone())
            .ok_or("输出代理缺失")?;

        let surface = compositor.create_surface(qh, idx);
        let layer_surface = shell.get_layer_surface(
            &surface,
            Some(&output),
            LayerKind::Overlay,
            "airsnip".to_string(),
            qh,
            idx,
        );
        // 四边都锚定 + 尺寸 0 → 合成器给「整块输出」的 configure。
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::Anchor;
        layer_surface.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
        layer_surface.set_size(0, 0);
        layer_surface.set_exclusive_zone(-1);
        // 独占键盘：截图期间所有按键都归覆盖层（Esc / Enter / Ctrl+S）。
        // 输入法在 S3 做文本标注时才需要恢复，届时再改 OnDemand。
        use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::KeyboardInteractivity;
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        surface.set_buffer_scale(geom.scale());
        surface.commit();

        Ok(Self {
            idx,
            surface,
            layer_surface,
            geom,
            logical: None,
            bufs: Vec::new(),
            next: 0,
        })
    }

    /// configure 之后准备两块缓冲（双缓冲，避免每帧新建 shm）。
    pub fn ensure_buffers(&mut self, shm: &WlShm, qh: &QueueHandle<Backend>) -> Result<(), Error> {
        if !self.bufs.is_empty() {
            return Ok(());
        }
        let (w, h) = self.geom.size;
        for _ in 0..2 {
            self.bufs.push(create_buf(shm, qh, w, h)?);
        }
        Ok(())
    }

    /// 取一块已释放的缓冲的下标；全都还在合成器手上时返回 None（跳过这一帧）。
    pub fn take_buffer_index(&mut self) -> Option<usize> {
        for i in 0..self.bufs.len() {
            let idx = (self.next + i) % self.bufs.len();
            if self.bufs[idx].released.load(Ordering::Acquire) {
                self.next = (idx + 1) % self.bufs.len();
                self.bufs[idx].released.store(false, Ordering::Release);
                return Some(idx);
            }
        }
        None
    }

    /// 把画好的缓冲贴上去。
    pub fn present(&self, buf: &Buf) {
        let (w, h) = (buf.w, buf.h);
        self.surface.attach(Some(&buf.buffer), 0, 0);
        self.surface.damage_buffer(0, 0, w, h);
        self.surface.commit();
    }

    /// 表面局部（逻辑坐标）→ 画布物理坐标。
    pub fn to_canvas(&self, x: f64, y: f64) -> (i32, i32) {
        let s = self.geom.scale() as f64;
        (self.geom.phys.0 + (x * s) as i32, self.geom.phys.1 + (y * s) as i32)
    }
}

impl Drop for Layer {
    fn drop(&mut self) {
        for b in &self.bufs {
            b.buffer.destroy();
        }
        self.layer_surface.destroy();
        self.surface.destroy();
    }
}

fn create_buf(shm: &WlShm, qh: &QueueHandle<Backend>, w: i32, h: i32) -> Result<Buf, Error> {
    if w <= 0 || h <= 0 {
        return Err(format!("输出尺寸非法：{w}×{h}").into());
    }
    let stride = w * 4;
    let len = stride as usize * h as usize;
    let path = format!(
        "/dev/shm/airsnip-ov-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    );
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("建 shm 临时文件 {path} 失败：{e}"))?;
    std::fs::remove_file(&path).ok();
    file.set_len(len as u64)?;
    // SAFETY：映射长度取自刚 set_len 的文件尺寸，文件句柄由 `_file` 持有。
    let map = unsafe { MmapMut::map_mut(&file)? };
    let released = Arc::new(AtomicBool::new(true));
    let pool = shm.create_pool(file.as_fd(), len as i32, qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, Format::Argb8888, qh, released.clone());
    pool.destroy();
    Ok(Buf {
        buffer,
        map,
        released,
        w,
        h,
        _file: file,
    })
}

impl Dispatch<WlSurface, usize> for Backend {
    fn event(
        _state: &mut Self,
        _proxy: &WlSurface,
        _event: <WlSurface as Proxy>::Event,
        _data: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, Arc<AtomicBool>> for Backend {
    fn event(
        _state: &mut Self,
        _proxy: &WlBuffer,
        event: wayland_client::protocol::wl_buffer::Event,
        data: &Arc<AtomicBool>,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wayland_client::protocol::wl_buffer::Event::Release = event {
            data.store(true, Ordering::Release);
        }
    }
}

impl Dispatch<ZwlrLayerSurfaceV1, usize> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrLayerSurfaceV1,
        event: layer_surface::Event,
        idx: &usize,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let shm = state.shm.clone();
        let mut pos_for_paint = None;
        {
            let Some(overlay) = state.overlay.as_mut() else {
                return;
            };
            let Some(pos) = overlay.layers.iter().position(|l| l.idx == *idx) else {
                return;
            };
            match event {
                layer_surface::Event::Configure { serial, width, height } => {
                    let layer = &mut overlay.layers[pos];
                    layer.layer_surface.ack_configure(serial);
                    crate::wayland::debug(format!("输出 {} configure {width}×{height}（逻辑）", idx));
                    layer.logical = Some((width, height));
                    // 首帧 configure 之后才知道尺寸，此时才建缓冲。
                    if let Some(shm) = shm {
                        let _ = layer.ensure_buffers(&shm, qh);
                    }
                    // 立刻画一帧，否则用户看到的是「黑一下」。
                    pos_for_paint = Some(pos);
                }
                layer_surface::Event::Closed => overlay.finish(Action::Cancel),
                _ => {}
            }
        }
        if let Some(pos) = pos_for_paint {
            crate::overlay::paint::paint_layer(state, pos);
        }
    }
}

use crate::overlay::state::Action;
