// SPDX-License-Identifier: GPL-3.0-only
//! 光标（CAP-2 / EDT-7）：按鼠标位置实时选择形状——
//! 手柄 → 对应方向缩放光标、对象本体与曲线控制柄 → 四向移动、按住空格 → 抓手、其余 → 十字。
//! 自绘形状（十字 / 抓手）见 `paint`，方向与移动光标用系统内置。

mod paint;

pub(in crate::overlay) use paint::cross_cursor;

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, HCURSOR, IDC_ARROW, IDC_SIZEALL, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, LoadCursorW,
    SetCursor,
};
use windows::core::PCWSTR;

use super::objedit::ObjHandle;
use super::pick::Handle;
use super::wndproc::space_down;

/// 光标形状：箭头（工具栏）、十字（默认）、抓手（空格移动选区）、移动与四个方向缩放。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Shape {
    /// 系统箭头：覆盖层工具栏上（按钮是点击目标，不是绘制区）。
    Arrow,
    Cross,
    Hand,
    /// 四向移动（对象本体、曲线控制柄、选区移动）。
    Move,
    NS,
    WE,
    NWSE,
    NESW,
}

impl Shape {
    fn cursor(self) -> HCURSOR {
        match self {
            Shape::Arrow => load(IDC_ARROW),
            Shape::Cross => cross_cursor(),
            Shape::Hand => paint::hand_cursor(),
            Shape::Move => load(IDC_SIZEALL),
            Shape::NS => load(IDC_SIZENS),
            Shape::WE => load(IDC_SIZEWE),
            Shape::NWSE => load(IDC_SIZENWSE),
            Shape::NESW => load(IDC_SIZENESW),
        }
    }
}

/// 系统光标句柄（进程共享，`LoadCursorW` 只是查表，无需额外缓存）。
fn load(id: PCWSTR) -> HCURSOR {
    unsafe { LoadCursorW(None, id).unwrap_or_default() }
}

/// 选区手柄 → 光标形状。
fn sel_shape(h: Handle) -> Shape {
    match h {
        Handle::N | Handle::S => Shape::NS,
        Handle::E | Handle::W => Shape::WE,
        Handle::NW | Handle::SE => Shape::NWSE,
        Handle::NE | Handle::SW => Shape::NESW,
        Handle::Move => Shape::Move,
    }
}

/// 对象手柄 → 光标形状。
fn obj_shape(h: ObjHandle) -> Shape {
    match h {
        ObjHandle::N | ObjHandle::S => Shape::NS,
        ObjHandle::E | ObjHandle::W => Shape::WE,
        ObjHandle::NW | ObjHandle::SE => Shape::NWSE,
        ObjHandle::NE | ObjHandle::SW => Shape::NESW,
        // 箭头端点：拖动即拉长 / 改向；`Move`/其它 → 四向移动
        ObjHandle::ArrowFrom | ObjHandle::ArrowTo | ObjHandle::Move => Shape::Move,
    }
}

impl super::Overlay {
    /// 按鼠标位置设置光标。
    pub(super) fn apply_cursor_at(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetCursor(Some(self.cursor_at(x, y).cursor()));
        }
    }

    /// 按当前鼠标屏幕位置刷新光标（`WM_SETCURSOR`、空格按下/松开时用）。
    pub(super) fn apply_cursor_now(&self, hwnd: HWND) {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
            let _ = ScreenToClient(hwnd, &mut p);
        }
        self.apply_cursor_at(p.x, p.y);
    }

    /// 当前位置的光标形状。判定顺序与 `interaction::on_lbutton_down` 的命中优先级一致
    /// （对象手柄 > 曲线控制柄 > 选区手柄 > 对象本体），保证「看到的光标 = 按下会发生的事」。
    pub(super) fn cursor_at(&self, x: i32, y: i32) -> Shape {
        // 拖动中：沿用按下时对应的形状，不因鼠标暂时离开手柄而变回十字
        if let Some(d) = &self.obj_drag {
            return obj_shape(d.handle);
        }
        if self.ctrl_drag.is_some() {
            return Shape::Move;
        }
        if let Some(d) = &self.drag {
            return sel_shape(d.handle);
        }
        if self.draft.is_some() {
            return Shape::Cross;
        }
        // 工具栏与字体面板是点击目标而非绘制区：鼠标在其上时用系统箭头
        if self.over_font_list(x, y) {
            return Shape::Arrow;
        }
        if let Some(bar) = &self.bar
            && super::geometry::in_rect(bar.rect, x, y)
        {
            return Shape::Arrow;
        }
        if let Some(sel) = self.selection {
            if let Some(i) = self.selected
                && let Some(h) = self
                    .doc
                    .objects()
                    .get(i)
                    .and_then(|o| super::objedit::hit_handle(o, x, y))
            {
                return obj_shape(h);
            }
            if self.hit_control(x, y).is_some() {
                // 曲线控制柄：拖动改变弯曲程度，用四向移动光标
                return Shape::Move;
            }
            if let Some(h) = super::pick::hit_handle(sel, x, y) {
                return sel_shape(h);
            }
            if self.hit_object(x, y).is_some() {
                return Shape::Move;
            }
        }
        if space_down() {
            return Shape::Hand;
        }
        Shape::Cross
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手柄 → 光标形状映射（光标必须与按下左键会发生的事一致）。
    #[test]
    fn handle_shapes_map_to_directions() {
        for (h, s) in [
            (ObjHandle::N, Shape::NS),
            (ObjHandle::S, Shape::NS),
            (ObjHandle::E, Shape::WE),
            (ObjHandle::W, Shape::WE),
            (ObjHandle::NW, Shape::NWSE),
            (ObjHandle::SE, Shape::NWSE),
            (ObjHandle::NE, Shape::NESW),
            (ObjHandle::SW, Shape::NESW),
            (ObjHandle::Move, Shape::Move),
            (ObjHandle::ArrowFrom, Shape::Move),
            (ObjHandle::ArrowTo, Shape::Move),
        ] {
            assert_eq!(obj_shape(h), s, "对象手柄 {h:?} 光标映射错误");
        }
        assert_eq!(sel_shape(Handle::N), Shape::NS);
        assert_eq!(sel_shape(Handle::W), Shape::WE);
        assert_eq!(sel_shape(Handle::NW), Shape::NWSE);
        assert_eq!(sel_shape(Handle::SE), Shape::NWSE);
        assert_eq!(sel_shape(Handle::NE), Shape::NESW);
        assert_eq!(sel_shape(Handle::SW), Shape::NESW);
        assert_eq!(sel_shape(Handle::Move), Shape::Move);
    }
}
