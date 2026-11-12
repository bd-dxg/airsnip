// SPDX-License-Identifier: GPL-3.0-only
//! 绘制工具枚举与新建对象的默认样式常量（EDT-1/2/4/5/6/12/13/14/15）。
//! 从 `editor::mod` 拆出，让对象模型文件守在单文件规模上限内。

/// 当前绘制工具（EDT-1/EDT-2/EDT-4/EDT-5/EDT-6/聚光）。
/// `#[repr(u8)]`：`settings::last_tool` 用 `as u8` 持久化/还原判别值，需保证布局稳定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tool {
    Rect,
    Arrow,
    /// EDT-4 文本：点击放置，输入后提交。
    Text,
    Highlight,
    Blur,
    /// 聚光高亮：选区其余暗化、该区域恢复原图亮度。
    Glow,
    /// EDT-12 序号标记：按下放徽标、拖动引线箭头。
    Number,
    /// EDT-13 直线：无箭头纯线段，Shift 吸附水平/垂直/45°。
    Line,
    /// EDT-14 椭圆：Shift 画正圆，支持填充。
    Ellipse,
    /// EDT-15 手绘自由笔画：贝塞尔平滑。
    Freehand,
}

impl Tool {
    /// 工具栏顺序。
    pub const ALL: [Tool; 10] = [
        Tool::Rect,
        Tool::Arrow,
        Tool::Text,
        Tool::Highlight,
        Tool::Blur,
        Tool::Glow,
        Tool::Number,
        Tool::Line,
        Tool::Ellipse,
        Tool::Freehand,
    ];
}

/// 新建对象默认线宽（物理像素，连续可调：滚轮 / 二级工具栏）。
pub const DEFAULT_LINE_WIDTH: f32 = 4.0;

/// 模糊默认强度（EDT-5：半径，连续可调）。
pub const DEFAULT_BLUR_RADIUS: f32 = 16.0;

/// 文本默认字号（EDT-4：物理像素，滚轮连续可调）。
pub const DEFAULT_FONT_SIZE: f32 = 24.0;

/// 序号标记默认字号（EDT-12：物理像素，徽标随字号缩放，滚轮连续可调）。
pub const DEFAULT_NUMBER_FONT_SIZE: f32 = 20.0;

/// 文本默认字体族（Win10/11 自带；字体面板可改，EDT-4）。
pub const DEFAULT_FONT_FAMILY: &str = "Microsoft YaHei UI";

/// 字号可调范围（滚轮 / 第二行 −+ 共用）。
pub const FONT_SIZE_RANGE: (f32, f32) = (10.0, 96.0);
