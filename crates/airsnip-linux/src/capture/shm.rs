// SPDX-License-Identifier: GPL-3.0-only
//! `wl_shm` 缓冲区：抓屏的像素落地在这里。
//!
//! Wayland 传文件描述符而不传共享内存名字，所以这里做一个 `/dev/shm` 临时文件、
//! 打开后立刻 `unlink`（句柄仍然有效，进程退出也不会留垃圾），再 `mmap` 到本进程。
//! 不用 `memfd_create` 是为了少引一个 unsafe 层（glibc 2.27+ 才有该符号）。

use std::fs::{File, OpenOptions};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicU32, Ordering};

use memmap2::MmapMut;
use wayland_client::QueueHandle;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;

use crate::Error;
use crate::wayland::Backend;

/// 序号只用于给临时文件起不重名的名字。
static SEQ: AtomicU32 = AtomicU32::new(0);

/// 一块可被合成器直接写入的 shm 缓冲区。
pub struct Buffer {
    pub buffer: WlBuffer,
    pub h: i32,
    pub stride: i32,
    pool: WlShmPool,
    map: MmapMut,
    // 文件句柄在 pool 创建后已被 Wayland 复制，但保留着更省心（也便于排障）。
    _file: File,
}

impl Buffer {
    /// 抓屏完成后读取像素（长度 = `stride * h`）。
    pub fn pixels(&self) -> &[u8] {
        &self.map[..]
    }

    /// 像素总字节数。
    pub fn len(&self) -> usize {
        self.stride as usize * self.h as usize
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
    }
}

/// 创建一个 `w × h` 的 `Xrgb8888` 缓冲区。
pub fn create(shm: &WlShm, qh: &QueueHandle<Backend>, w: i32, h: i32) -> Result<Buffer, Error> {
    if w <= 0 || h <= 0 {
        return Err(format!("输出尺寸非法：{w}×{h}").into());
    }
    let stride = w * 4;
    let len = stride as usize * h as usize;

    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = format!("/dev/shm/airsnip-{}-{seq}", std::process::id());
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| format!("建 shm 临时文件 {path} 失败：{e}"))?;
    std::fs::remove_file(&path).ok();
    file.set_len(len as u64)?;

    // SAFETY：映射长度取自刚 set_len 的文件尺寸，映射期间文件句柄由 `_file` 持有。
    let map = unsafe { MmapMut::map_mut(&file)? };

    let pool = shm.create_pool(file.as_fd(), len as i32, qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, Format::Xrgb8888, qh, ());
    Ok(Buffer {
        buffer,
        h,
        stride,
        pool,
        map,
        _file: file,
    })
}
