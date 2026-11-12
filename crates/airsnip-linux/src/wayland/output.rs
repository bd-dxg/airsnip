// SPDX-License-Identifier: GPL-3.0-only
//! 输出（显示器）信息收集：位置、逻辑尺寸、缩放、变换、当前 mode。
//!
//! 数据来源有两套：
//! - `wl_output`：`geometry`（位置，部分合成器恒为 0）、`mode`（物理像素）、`scale`、`transform`；
//! - `zxdg_output_v1`：`logical_position` / `logical_size` / `name`（可靠，优先使用）。
//!
//! 抓屏缓冲区尺寸 = 当前 mode 尺寸，**随 transform 交换长短边**
//! （合成器按「用户看到的方向」交付像素，见 wlr-screencopy 协议说明）。

use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::ZxdgOutputV1;

use super::Backend;

/// 单个输出的已收集信息。
#[derive(Clone, Default)]
pub struct Info {
    pub output: Option<WlOutput>,
    pub xdg: Option<ZxdgOutputV1>,
    /// 输出名（`zxdg_output_v1.name`，如 `DP-1`），老合成器下为空。
    pub name: Option<String>,
    /// 当前 mode 的物理像素尺寸（未按 transform 交换）。
    pub mode_w: i32,
    pub mode_h: i32,
    /// 缓冲区缩放（通常 1 或 2）。
    pub scale: i32,
    /// `wl_output.geometry` 给出的位置（部分合成器为 0，仅作兜底）。
    pub geo_x: i32,
    pub geo_y: i32,
    /// `wl_output.transform`：0/1/2/3 = 0°/90°/180°/270°，4–7 为对应镜像。
    pub transform: u32,
    /// `zxdg_output_v1` 的逻辑位置与逻辑尺寸（可能尚未收到）。
    pub logical: Option<(i32, i32, i32, i32)>,
}

impl Info {
    /// 缩放系数（退化值 0 视为 1）。
    pub fn scale(&self) -> i32 {
        if self.scale <= 0 { 1 } else { self.scale }
    }

    /// 该输出在合成画布上的左上角（物理像素）。
    pub fn canvas_pos(&self) -> (i32, i32) {
        match self.logical {
            Some((x, y, _, _)) => (x * self.scale(), y * self.scale()),
            None => (self.geo_x, self.geo_y),
        }
    }

    /// 抓屏缓冲区尺寸：mode 尺寸按 transform 交换长短边。
    pub fn buffer_size(&self) -> (i32, i32) {
        let w = if self.mode_w > 0 { self.mode_w } else { 1 };
        let h = if self.mode_h > 0 { self.mode_h } else { 1 };
        match self.transform {
            1 | 3 | 5 | 7 => (h, w),
            _ => (w, h),
        }
    }

    /// 展示用名称。
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| "(未命名输出)".to_string())
    }
}

impl Dispatch<WlOutput, usize> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &WlOutput,
        event: wl_output::Event,
        idx: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(info) = state.outputs.get_mut(*idx) else {
            return;
        };
        match event {
            wl_output::Event::Geometry { x, y, .. } => {
                info.geo_x = x;
                info.geo_y = y;
            }
            wl_output::Event::Mode {
                flags, width, height, ..
            } => {
                // 只按「当前」mode 记录尺寸；其他 mode 是可选分辨率列表。
                let current = flags
                    .into_result()
                    .map(|f| f.contains(wl_output::Mode::Current))
                    .unwrap_or(false);
                if current || info.mode_w == 0 {
                    info.mode_w = width;
                    info.mode_h = height;
                }
            }
            wl_output::Event::Scale { factor } => {
                info.scale = if factor > 0 { factor } else { 1 };
            }
            wl_output::Event::Name { name } => info.name = Some(name),
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputV1, usize> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &ZxdgOutputV1,
        event: wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::Event,
        idx: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::Event;
        let Some(info) = state.outputs.get_mut(*idx) else {
            return;
        };
        match event {
            Event::LogicalPosition { x, y } => {
                let (_, _, w, h) = info.logical.unwrap_or((0, 0, 0, 0));
                info.logical = Some((x, y, w, h));
            }
            Event::LogicalSize { width, height } => {
                let (x, y, _, _) = info.logical.unwrap_or((0, 0, 0, 0));
                info.logical = Some((x, y, width, height));
            }
            // v3 已废弃 `done`，但 v1/v2 合成器仍然发，收到也无害。
            Event::Name { name } => info.name = Some(name),
            _ => {}
        }
    }
}
