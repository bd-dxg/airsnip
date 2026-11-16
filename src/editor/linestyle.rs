// SPDX-License-Identifier: GPL-3.0-only
//! 标注线型（EDT-18，issue #168）：实线 / 虚线 / 点划线。
//! 线型是**对象样式**（存在 `Style` 上，与颜色 / 线宽同一套语义：改当前样式会同时作用于选中对象），
//! 也是跨截图记忆的「当前样式」；记忆走静态槽（`line_store` / `line_take`，同 `palette` 的模式）。
//! 段长按线宽缩放（ADR 0042）：相邻线宽的观感比例恒定，导出到任意分辨率都不走样。

use serde::{Deserialize, Serialize};

/// 线型（EDT-18）：只对有**描边**的对象有意义——填充矩形 / 椭圆、高亮、模糊、聚光、文本都没有描边。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    /// 实线（默认）：不设 dash，走原有的整段描边路径。
    #[default]
    Solid,
    /// 虚线：段 3×线宽、隙 2×线宽。
    Dashed,
    /// 点划线：长段 6×线宽、隙 2×线宽、点 1×线宽、隙 2×线宽。
    DashDot,
}

impl LineStyle {
    /// 下拉列表顺序（实线 → 虚线 → 点划线）。
    pub const ALL: [LineStyle; 3] = [LineStyle::Solid, LineStyle::Dashed, LineStyle::DashDot];

    /// 是否实线。
    pub fn is_solid(self) -> bool {
        self == LineStyle::Solid
    }

    /// tiny-skia 的 dash 数组（`Stroke.dash`，实 / 虚交替的绝对像素长度）。
    /// 返回 `None` 表示实线（不设 dash）。
    pub fn dash_array(self, width: f32) -> Option<Vec<f32>> {
        let w = width.max(0.5);
        let arr = match self {
            LineStyle::Solid => return None,
            LineStyle::Dashed => vec![w * 3.0, w * 2.0],
            LineStyle::DashDot => vec![w * 6.0, w * 2.0, w * 1.0, w * 2.0],
        };
        // 极细线兜底：段长 / 间隙不小于 1px，否则 dash 会退化成噪点
        Some(arr.into_iter().map(|v| v.max(1.0)).collect())
    }
}

/// 覆盖层线程改过的线型：主线程在截图结束时取出并写回配置（同 `palette::colors_store`）。
static LINE: std::sync::Mutex<Option<LineStyle>> = std::sync::Mutex::new(None);

/// 记录本次改过的线型（覆盖层线程调用）。
pub fn line_store(s: LineStyle) {
    if let Ok(mut g) = LINE.lock() {
        *g = Some(s);
    }
}

/// 取出并清空记录的线型（主线程写回配置用）。
pub fn line_take() -> Option<LineStyle> {
    LINE.lock().ok().and_then(|mut g| g.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 段长随线宽缩放：同一种线型在不同线宽下保持相同比例（ADR 0042）。
    #[test]
    fn dash_scales_with_width() {
        assert_eq!(LineStyle::Solid.dash_array(4.0), None, "实线不设 dash");
        assert_eq!(LineStyle::Dashed.dash_array(4.0), Some(vec![12.0, 8.0]));
        assert_eq!(LineStyle::Dashed.dash_array(8.0), Some(vec![24.0, 16.0]));
        assert_eq!(
            LineStyle::DashDot.dash_array(4.0),
            Some(vec![24.0, 8.0, 4.0, 8.0]),
            "点划线 = 长段 / 隙 / 点 / 隙"
        );
    }

    /// 极细线的兜底：段长与间隙都不小于 1px（否则 0.5px 线宽会算出 1.5 / 1.0 以下的碎段）。
    #[test]
    fn dash_never_below_one_pixel() {
        for style in [LineStyle::Dashed, LineStyle::DashDot] {
            let arr = style.dash_array(0.2).expect("非实线应有 dash 数组");
            assert!(arr.iter().all(|v| *v >= 1.0), "{style:?} 出现小于 1px 的段：{arr:?}");
        }
    }

    /// 运行时记忆：写入后能取出，取出即清空。
    #[test]
    fn line_memory_roundtrip() {
        assert_eq!(line_take(), None, "初始应为空");
        line_store(LineStyle::DashDot);
        assert_eq!(line_take(), Some(LineStyle::DashDot));
        assert_eq!(line_take(), None, "取出后应清空");
    }
}
