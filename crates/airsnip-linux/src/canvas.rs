// SPDX-License-Identifier: GPL-3.0-only
//! 画布：一张 RGBA8888 的位图，本前端所有位图操作都落在它上面。
//!
//! 与 Windows 版的 `Pixmap` 对应，但只保留 Linux 侧需要的最小接口
//! （ADR 0034 阶段 1 只做「把各输出的画面合成为整屏底图」）。

/// 画布：行优先、RGBA8888、无行尾填充。
#[derive(Clone, Debug)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Canvas {
    /// 全透明画布。
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0; w as usize * h as usize * 4],
        }
    }

    /// 把一块 `wl_shm` 抓屏结果贴到画布上的 `(dx, dy)`。
    ///
    /// - `src` 为源位图原始字节，行长 `stride`（可有行尾填充）；
    /// - `xrgb` 为 true 表示源是 `Xrgb8888`（内存序 B,G,R,X），false 表示 `Xbgr8888`（R,G,B,X）；
    /// - `flip_y` 对应 `zwlr_screencopy_frame_v1.flags::y_invert`（OpenGL 合成器可能上下颠倒）。
    ///
    /// 越界部分裁剪丢弃（输出布局出现负坐标或部分重叠时不 panic）。
    pub fn blit(&mut self, dx: i32, dy: i32, sw: u32, sh: u32, stride: usize, src: &[u8], xrgb: bool, flip_y: bool) {
        let row_bytes = sw as usize * 4;
        for y in 0..sh {
            let ty = dy as i64 + y as i64;
            if ty < 0 || ty >= self.h as i64 {
                continue;
            }
            let sy = if flip_y { sh - 1 - y } else { y } as usize;
            let s_off = sy * stride;
            if s_off + row_bytes > src.len() {
                continue;
            }
            let src_row = &src[s_off..s_off + row_bytes];
            for x in 0..sw {
                let tx = dx as i64 + x as i64;
                if tx < 0 || tx >= self.w as i64 {
                    continue;
                }
                let si = (x as usize) * 4;
                let di = (ty as usize * self.w as usize + tx as usize) * 4;
                let (r, g, b) = if xrgb {
                    (src_row[si + 2], src_row[si + 1], src_row[si])
                } else {
                    (src_row[si], src_row[si + 1], src_row[si + 2])
                };
                self.px[di] = r;
                self.px[di + 1] = g;
                self.px[di + 2] = b;
                self.px[di + 3] = 255;
            }
        }
    }

    /// 取一块子画布（阶段 2 起：按选区裁剪出图）。
    pub fn crop(&self, x: i32, y: i32, w: u32, h: u32) -> Canvas {
        let mut out = Canvas::new(w, h);
        for row in 0..h as i32 {
            let sy = y + row;
            if sy < 0 || sy >= self.h as i32 {
                continue;
            }
            for col in 0..w as i32 {
                let sx = x + col;
                if sx < 0 || sx >= self.w as i32 {
                    continue;
                }
                let si = (sy as usize * self.w as usize + sx as usize) * 4;
                let di = (row as usize * w as usize + col as usize) * 4;
                out.px[di..di + 4].copy_from_slice(&self.px[si..si + 4]);
            }
        }
        out
    }
}
