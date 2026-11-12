// SPDX-License-Identifier: GPL-3.0-only
//! 输入：指针（框选 / 画标注）与键盘（工具切换、撤销重做、输出）。
//!
//! **不解析 xkb keymap**：Wayland 的 `wl_keyboard.key` 是 Linux evdev 原始码
//! （比 X11 keycode 小 8），数字键与 Esc/Enter/Ctrl 这些码是稳定的，直接比常量。
//! 中文输入法（text-input-v3）在文本工具那一步再接。

use wayland_client::protocol::wl_keyboard::{self, KeyState, WlKeyboard};
use wayland_client::protocol::wl_pointer::{self, ButtonState, WlPointer};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

use airsnip_core::editor::Tool;

use super::state::{Action, Sel};
use super::{edit, paint};
use crate::wayland::Backend;

// Linux evdev 键码（input-event-codes.h）
const KEY_ESC: u32 = 1;
const KEY_ENTER: u32 = 28;
const KEY_CTRL_L: u32 = 29;
const KEY_S: u32 = 31;
const KEY_C: u32 = 46;
const KEY_Z: u32 = 44;
const KEY_Y: u32 = 21;
const KEY_KPENTER: u32 = 96;
const KEY_CTRL_R: u32 = 97;
const KEY_SHIFT_L: u32 = 42;
/// BTN_LEFT
const BTN_LEFT: u32 = 0x110;

impl Dispatch<WlSeat, ()> for Backend {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlPointer, ()> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let cursor_dev = state.cursor_shape_device.clone();
        let mut repaint = false;
        {
            let Some(ov) = state.overlay.as_mut() else {
                return;
            };
            match event {
                wl_pointer::Event::Enter {
                    serial,
                    surface,
                    surface_x,
                    surface_y,
                } => {
                    ov.serial = serial;
                    ov.active = ov.layers.iter().position(|l| l.surface == surface);
                    if let Some(i) = ov.active {
                        ov.pointer = Some(ov.layers[i].to_canvas(surface_x, surface_y));
                        let g = ov.layers[i].geom;
                        crate::wayland::debug(format!(
                            "指针进入：surface=({surface_x:.0},{surface_y:.0}) 图层{i} phys={:?} scale={} → 画布{:?}",
                            g.phys,
                            g.scale(),
                            ov.pointer
                        ));
                    }
                    if let Some(dev) = cursor_dev {
                        use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;
                        dev.set_shape(serial, Shape::Crosshair);
                    }
                }
                wl_pointer::Event::Motion {
                    surface_x, surface_y, ..
                } => {
                    if let Some(i) = ov.active {
                        let p = ov.layers[i].to_canvas(surface_x, surface_y);
                        ov.pointer = Some(p);
                        if ov.drawing {
                            edit::update(ov, p);
                            repaint = true;
                        } else if let Some(from) = ov.drag_from {
                            let (cw, ch) = (ov.bright.width() as i32, ov.bright.height() as i32);
                            ov.sel = Some(Sel::from_drag(from, p).clamped(cw, ch));
                            repaint = true;
                        } else if ov.current_sel().is_some() {
                            // 悬停反馈（工具栏按钮底色）
                            repaint = true;
                        }
                    }
                }
                wl_pointer::Event::Button {
                    serial,
                    button,
                    state: bs,
                    ..
                } => {
                    ov.serial = serial;
                    if button != BTN_LEFT {
                        return;
                    }
                    let pressed = matches!(bs, WEnum::Value(ButtonState::Pressed));
                    let Some(p) = ov.pointer else { return };
                    // 1) 工具栏优先：按在工具栏上就只处理按钮，不动选区/标注
                    let mut handled = false;
                    if pressed && let Some(sel) = ov.current_sel() {
                        let (cw, ch) = (ov.bright.width() as i32, ov.bright.height() as i32);
                        let tb = super::toolbar::layout(cw, ch, sel);
                        if let Some(item) = tb.hit(p.0, p.1) {
                            handled = true;
                            ov.toolbar_hold = true;
                            on_toolbar(ov, item);
                        }
                    }
                    if pressed {
                        // 2) 已框选且按在选区内 → 画标注；否则开始新的框选
                        if !handled {
                            let inside = ov.current_sel().map(|s| s.contains(p.0, p.1)).unwrap_or(false);
                            if inside {
                                ov.drawing = true;
                                edit::begin(ov, p);
                            } else {
                                ov.drawing = false;
                                ov.drag_from = Some(p);
                                ov.sel = None;
                                crate::wayland::debug(format!("框选起点 {p:?}"));
                                ov.doc = airsnip_core::editor::Document::new();
                            }
                        }
                    } else if ov.toolbar_hold {
                        // 工具栏上松手：不结束任何手势
                        ov.toolbar_hold = false;
                    } else if ov.drawing {
                        edit::commit(ov);
                        ov.drawing = false;
                    } else {
                        ov.drag_from = None;
                        if ov.sel.map(|s| !s.usable()).unwrap_or(true) {
                            ov.sel = None;
                        }
                        crate::wayland::debug(format!("框选结束 {:?}", ov.sel));
                    }
                    repaint = true;
                }
                wl_pointer::Event::Leave { .. } => ov.active = None,
                _ => {}
            }
        }
        if repaint {
            paint::refresh_all(state);
        }
    }
}

/// 工具栏点击。
fn on_toolbar(ov: &mut super::state::Overlay, item: super::toolbar::Item) {
    use super::toolbar::Item;
    use airsnip_core::editor::DEFAULT_COLORS;
    match item {
        Item::Tool(t) => {
            ov.tool = t;
            ov.draft = None;
        }
        Item::Color(i) => {
            if i < DEFAULT_COLORS.len() {
                ov.color_index = i;
                ov.style.color = DEFAULT_COLORS[i];
            }
        }
        Item::Fill => ov.filled = !ov.filled,
        Item::Undo => {
            edit::undo(ov);
        }
        Item::Redo => {
            edit::redo(ov);
        }
        Item::Copy => ov.finish(Action::Copy),
        Item::Save => ov.finish(Action::Save),
        Item::Sep => {}
    }
}

impl Dispatch<WlKeyboard, ()> for Backend {
    fn event(
        state: &mut Self,
        _proxy: &WlKeyboard,
        event: wl_keyboard::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let mut repaint = false;
        {
            let Some(ov) = state.overlay.as_mut() else {
                return;
            };
            match event {
                wl_keyboard::Event::Enter { serial, .. } => ov.serial = serial,
                wl_keyboard::Event::Modifiers { mods_depressed, .. } => {
                    ov.ctrl = mods_depressed & 4 != 0; // XKB Control
                }
                wl_keyboard::Event::Key {
                    serial, key, state: ks, ..
                } => {
                    ov.serial = serial;
                    if !matches!(ks, WEnum::Value(KeyState::Pressed)) {
                        return;
                    }
                    match key {
                        KEY_CTRL_L | KEY_CTRL_R => ov.ctrl = true,
                        KEY_ESC => ov.finish(Action::Cancel),
                        KEY_ENTER | KEY_KPENTER => ov.finish(Action::Copy),
                        KEY_C if ov.ctrl => ov.finish(Action::Copy),
                        KEY_S if ov.ctrl => ov.finish(Action::Save),
                        KEY_Z if ov.ctrl => repaint = edit::undo(ov),
                        KEY_Y if ov.ctrl => repaint = edit::redo(ov),
                        // 工具切换：1–9、0（与 README 的 10 个工具顺序一致）
                        k if (2..=11).contains(&k) => {
                            let idx = (k - 2) as usize; // evdev 1..=10 对应键帽 1..0
                            if let Some(t) = Tool::ALL.get(idx).copied() {
                                ov.tool = t;
                                ov.draft = None;
                                repaint = true;
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if repaint {
            paint::refresh_all(state);
        }
    }
}

// `KEY_SHIFT_L` 预留给 S3 的 Shift 约束（正方形 / 方向吸附）
#[allow(dead_code)]
const _SHIFT: u32 = KEY_SHIFT_L;

impl Dispatch<wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, ()>
    for Backend
{
    fn event(
        _state: &mut Self,
        _proxy: &wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
        _event: <wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1, ()>
    for Backend
{
    fn event(
        _state: &mut Self,
        _proxy: &wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1,
        _event: <wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::WpCursorShapeDeviceV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}
