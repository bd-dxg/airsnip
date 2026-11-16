// SPDX-License-Identifier: GPL-3.0-only
//! 对象样式（EDT-7：颜色 / 线宽；EDT-18：线型；EDT-19：渐变）。
//! 从 `editor::mod` 拆出，控制单文件规模。

use super::{Gradient, LineStyle};

/// 对象样式。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// 描边/填充色（RGBA，alpha 支持矩形透明度，EDT-1）。
    /// 渐变时它是**代表色**（渐变中点，见 `with_gradient`）：文本、荧光笔、徽标反白数字等
    /// 只认单色的地方都用它。
    pub color: [u8; 4],
    /// 线性渐变（EDT-19）：`Some` 时描边 / 填充 / 序号徽标走三色标渐变。
    pub gradient: Option<Gradient>,
    /// 线宽（物理像素）。
    pub width: f32,
    /// 线型（EDT-18）：只对有描边的对象有意义。
    pub line: LineStyle,
}

impl Style {
    pub fn new(color: [u8; 4], width: f32) -> Self {
        Self {
            color,
            gradient: None,
            width,
            line: LineStyle::Solid,
        }
    }

    /// 换线型（构造当前样式时用，保留颜色与线宽）。
    pub fn with_line(self, line: LineStyle) -> Self {
        Self { line, ..self }
    }

    /// 换成渐变涂色：代表色同步成渐变中点（文本 / 对比度 / 纯色回退用它）。
    /// alpha 跟对象原有透明度走（渐变不额外做透明度）。
    pub fn with_gradient(self, g: Gradient) -> Self {
        let g = g.with_alpha(self.color[3]);
        Self {
            color: g.representative(),
            gradient: Some(g),
            ..self
        }
    }

    /// 换回纯色（取消渐变）。
    pub fn with_solid(self, color: [u8; 4]) -> Self {
        Self {
            color,
            gradient: None,
            ..self
        }
    }

    /// 是否在用渐变涂色。
    pub fn is_gradient(&self) -> bool {
        self.gradient.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 渐变与 alpha：换成渐变后代表色变成中点色，alpha 跟原色走；换回纯色即清空渐变。
    #[test]
    fn gradient_and_solid_switch_keep_alpha() {
        let base = Style::new([255, 0, 0, 120], 3.0);
        assert!(!base.is_gradient());

        let g = Gradient::seed([255, 0, 0, 255]);
        let grad = base.with_gradient(g);
        assert!(grad.is_gradient());
        assert_eq!(grad.color[..3], g.representative()[..3], "代表色应为渐变中点");
        assert_eq!(grad.color[3], 120, "代表色 alpha 应跟原色");
        assert!(
            grad.gradient.unwrap().stops.iter().all(|s| s[3] == 120),
            "alpha 应跟原色"
        );

        let back = grad.with_solid([0, 0, 255, 120]);
        assert!(!back.is_gradient());
        assert_eq!(back.color, [0, 0, 255, 120]);
    }
}
