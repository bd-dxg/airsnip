// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层（CAP-2/3）：覆盖整个虚拟屏幕的无边框置顶窗口，
//! 冻结画面 + 选区暗化（预生成暗图 + 选区回贴原图），支持框选、移动与八向缩放。
//! 选区确认后进入标注模式（EDT-1/EDT-2/EDT-7）：工具栏切换工具与样式，在选区内拖拽即绘制标注。
//! **ADR 0036**：框选完成后默认是「移动选区」态（`Mode::Move`）——拖选区内空白平移选区、拖对象移动该对象，
//! 标注跟着一起走；显式选工具（工具栏 / 数字键）后才进入绘制态（`Mode::Draw`）。
//!
//! 文件分工：`wndproc` 消息分发、`setup` 窗口创建、`geometry` 矩形运算、`surface` 像素缓冲与上屏、
//! `cursor` 光标、`selection` 脏区重绘、`interaction` 鼠标交互、`pick` 对象选中与控制柄、
//! `objedit` 对象手柄与拖动缩放、`wheel` 滚轮调属性、`font` 字体设置与字体面板、`annotate` 标注与输出、
//! `delete` 选中对象的单独删除、`toolbar` 工具栏、`ui` 按显示器 DPI 的界面缩放、`opts` 启动参数、`recapture` 再次截图（issue #118）。
//! 按下优先级（ADR 0023 / ADR 0036）见 `pick::pick_target`：手柄 > 曲线控制柄 > 选区手柄
//! > 空格移动选区 > （移动态）对象 / 选区平移 > Ctrl + 对象 > 选中对象框边 > 对象内部（绘制）> 空白绘制。

mod annotate;
mod cancel_hint;
mod control;
mod cursor;
mod delete;
mod fade;
mod font;
pub(crate) mod geometry;
mod ime;
mod init;
mod interaction;
mod linemenu;
mod number_edit;
mod objedit;
mod opts;
mod pick;
mod pin_edit;
mod quicksave;
mod quickselect;
mod recapture;
mod run;
mod selection;
mod setup;
pub(crate) mod shape;
mod snap;
pub(crate) mod surface;
mod text_edit_state;
mod textinput;
mod toast;
pub(crate) mod toolbar;
pub(crate) mod ui;
mod uia;
mod wheel;
mod wndproc;

use tiny_skia::Pixmap;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{HBITMAP, HDC};
use windows::Win32::UI::Input::Ime::HIMC;

use crate::capture::CapturedScreen;
use crate::editor::{Document, Mode, Object, Point};
use crate::theme::Palette;

use geometry::SelRect;
use interaction::{Drag, Phase};
use text_edit_state::TextEdit;

/// 覆盖层运行结果（M4 长截图视口选取复用）。#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayResult {
    /// 截图完成（已复制/保存/贴图）。
    Done,
    /// 用户取消。
    Cancelled,
    /// 长截图视口选取完成：屏幕坐标 (x, y, w, h)。
    Viewport((i32, i32, u32, u32)),
}

/// 覆盖层运行时状态（完整实现分散在各子模块的 `impl Overlay` 中）。
pub struct Overlay {
    pub capture: CapturedScreen,
    pub original: Pixmap,
    pub dimmed: Pixmap,
    pub display: Pixmap,
    /// 首帧标记：首帧 `display` 已是暗图，跳过回贴（M5 性能）。
    pub first_paint: bool,
    /// 当前主题调色板（截图开始时由设置解析，M3）。
    pub palette: &'static Palette,
    /// 界面缩放（按光标所在显示器 DPI，CAP-5）：工具栏、手柄、尺寸提示与光标随之缩放。
    pub ui: ui::Ui,
    pub selection: Option<SelRect>,
    phase: Phase,
    drag: Option<Drag>,
    /// 标注文档（对象列表 + 撤销/重做，EDT-1/EDT-2/EDT-7）。
    doc: Document,
    /// 当前交互模式（ADR 0036）：`Move` = 平移选区 / 移动对象，`Draw(t)` = 用 t 画新标注。
    /// 开局为 `Move`（贴图编辑模式例外，选区锁定，直接进绘制态），
    /// 不再记忆上次工具（没有“上次工具”这件事）。
    mode: Mode,
    /// 双击选区空白的动作（ADR 0036 / 设置面板「通用」页）：复制或贴图。
    double_click: crate::settings::DoubleClickAction,
    /// 窗口吸附（CAP-6 / ADR 0037，issue #62）：悬停高亮光标下的窗口矩形，单击即选中。
    window_snap: bool,
    /// 吸附运行态：候选矩形 / 候选窗口句柄 / 按下待定。
    snap: snap::Snap,
    /// 控件级捕捉（CAP-6 二级 / ADR 0039）：在整窗吸附上再下钻到 UIA 子控件。
    control_snap: bool,
    /// 控件级捕捉的运行态（worker 线程 / 请求代次 / 熔断）。
    uia: uia::Uia,
    /// 淡入进度（帧数，`fade::STEPS` 即完成）：一次性定时器链的状态（`fade`）。
    fade_step: u32,
    /// 工具切换键配置（M3：数字键 1-5 切换标注工具）。
    tool_keys: crate::settings::ToolKeys,
    /// 长截图键（覆盖层内按下，默认 S；设置面板可改）。
    longshot_key: u32,
    /// 全屏键（ADR 0040，默认 F）：光标所在显示器的整屏作为选区，仅无选区时生效。
    fullscreen_key: u32,
    /// 上次区域键（ADR 0040，默认 R）：复用本次运行的上次框选区域，仅无选区时生效。
    last_area_key: u32,
    /// 保存 / 快速保存的运行时状态（OUT-2 / ADR 0041）：键位、配置、在途与轻提示。
    quick: quicksave::QuickSave,
    /// 工具栏可见性（EDT-16）：逐元素显隐；隐藏的工具 / 按钮不绘制，但快捷键与输出行为照旧。
    visibility: crate::settings::ToolbarConfig,
    /// 有未保存标注时取消先确认（OUT-3 / ADR 0027，设置项，issue #120）。
    confirm_cancel: bool,
    /// 二次确认「待命」状态（OUT-3 / issue #120）：上次触发退出的键与时刻。
    cancel_arm: Option<(u32, std::time::Instant)>,
    /// 已弹出的二次确认提示（OUT-3 / issue #120）。
    cancel_hint: Option<cancel_hint::CancelHint>,
    /// 贴图键（覆盖层内按下贴选区，默认 F3；设置面板可改，ADR 0019）。
    pin_key: u32,
    /// 保存偏好（OUT-2）：文件名模板、上次格式与上次目录。
    save: crate::settings::SavePrefs,
    /// 当前色块下标（0..SWATCH_COUNT，EDT-7）：`SOLID_COUNT` 及以上是渐变色块（EDT-19）。
    color_index: usize,
    /// 纯色块颜色（EDT-7 / EDT-19）：初值来自配置，双击色块取色改写。
    colors: [[u8; 4]; crate::editor::SOLID_COUNT],
    /// 下一笔将用的渐变（EDT-19）：每落笔 / 换工具推一套，只存运行时（不写配置）。
    gradient: crate::editor::Gradient,
    /// 渐变的基准色（首次进入渐变模式时的纯色块）与已推进的套数。
    /// 用「基准 + 计数」生成而不是拿上一套终点当起点：后者经 u8 往返会逐套漂，步长越走越小。
    gradient_base: [u8; 4],
    gradient_count: u32,
    /// 新建对象的默认线宽（滚轮 / 二级工具栏可连续调整）。
    line_width: f32,
    /// 当前线型（EDT-18）：新建对象用它，切换时同时作用于选中对象。
    line_style: crate::editor::LineStyle,
    /// 展开中的线型下拉（EDT-18）。
    line_menu: Option<toolbar::linemenu::LineMenu>,
    /// 新建模糊对象的默认半径（EDT-5）。
    blur_radius: f32,
    /// 新建文本对象的默认字号（EDT-4，滚轮 / 第二行可调）。
    font_size: f32,
    /// 新建序号标记的默认字号（EDT-12，滚轮可调；徽标圆随字号缩放）。
    number_font_size: f32,
    /// 新建文本对象的默认字体族、粗体、斜体（EDT-4，第二行工具栏可改）。
    font_family: std::sync::Arc<str>,
    bold: bool,
    italic: bool,
    /// 展开中的字体列表面板（EDT-4）。
    font_list: Option<toolbar::fontlist::FontPanel>,
    /// 展开中的取色面板（EDT-7，issue #73；双击色块打开）。
    color_panel: Option<toolbar::colorpanel::ColorPanel>,
    /// 取色拖动中是否已为选中对象开启撤销快照（按下时 `doc.begin`，松手归档，issue #73）。
    color_edit: bool,
    /// 取色拖动开始时选中对象的颜色（判断松手时是否真的变化）。
    color_edit_before: Option<[u8; 4]>,
    filled: bool,
    /// 正在拖拽、尚未提交的标注对象。
    draft: Option<Object>,
    /// 按下但还没决定是「画」还是「选中」的点击（ADR 0023）：对象内部与选区内空白都先记一笔。
    pending_click: Option<interaction::PendingClick>,
    /// 上次左键按下是否启动了选区内空白绘制（双击完成截图判据，M3）。
    /// 不用 `draft.is_some()`：双击第一击的草稿会在弹起时被提交/丢弃。
    pub down_started_draw: bool,
    /// 当前选中的对象索引（EDT-7）；控制柄仅在选中时显示（EDT-3）。
    selected: Option<usize>,
    /// 正在拖动的曲线控制柄：(控制点索引, 按下时的坐标)（EDT-3）。
    ctrl_drag: Option<(usize, Point)>,
    /// 正在拖动的选中对象（EDT-7 移动/缩放）。
    obj_drag: Option<objedit::ObjDrag>,
    /// 选区是否锁定（贴图编辑模式，ADR 0028）：禁止移动 / 缩放选区。
    locked_selection: bool,
    /// 贴图编辑目标句柄（ADR 0028）：完成时随结果回带给主线程写回贴图。
    pin_edit_target: Option<usize>,
    /// 文本输入中状态（EDT-4）。
    text_edit: Option<TextEdit>,
    /// 当前工具栏布局（选区存在时才有）。
    bar: Option<toolbar::Toolbar>,
    /// 悬浮提示状态（EDT-10）：悬停目标、延迟定时器与已弹气泡。
    tip: toolbar::tip::Tip,
    pub hwnd: HWND,
    /// 禁用输入法前的 IME 上下文（截图期间禁用，文本输入时恢复，EDT-4）。
    ime_old: HIMC,
    /// 主消息窗口句柄（贴图载荷/exel 完成后通知用，PIN-1）。
    pub main_hwnd: HWND,
    pub cancelled: bool,
    /// 长截图触发（M4）：工具栏按钮 / 长截图键设置屏幕坐标选区，随后覆盖层关闭。
    pub viewport: Option<(i32, i32, u32, u32)>,
    /// 上屏用内存 DC（持有 DIB section，BGRA 像素直接写入 dib_bits）。
    mem_dc: HDC,
    /// DIB section 位图句柄。
    dib_bmp: HBITMAP,
    /// DIB section 像素指针（BGRA，top-down）。
    dib_bits: *mut u8,
}

/// 覆盖层启动参数（SYS-2 / CAP-5 / OUT-2）：见 `opts`。
pub use opts::{OverlayOpts, PinEdit};
/// 主循环（`run`）拆到 `run` 子模块，保持单文件规模。
pub use run::run;
