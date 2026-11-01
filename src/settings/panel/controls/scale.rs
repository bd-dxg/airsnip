// SPDX-License-Identifier: GPL-3.0-only
//! 界面缩放设置项的行 ID 映射（CAP-5）：`settings::UiScale` ↔ `RowId`。
//! 设置项定义在 `settings`（配置持久化），这里只做面板行与选项的对应，
//! 避免 `controls` 的模型枚举与交互/绘制各自维护一份 match。

use crate::settings::UiScale;

use super::RowId;

/// 界面缩放选项 → 行控件 ID。
pub fn ui_scale_row(m: UiScale) -> RowId {
    match m {
        UiScale::Auto => RowId::UiScaleAuto,
        UiScale::P100 => RowId::UiScale100,
        UiScale::P125 => RowId::UiScale125,
        UiScale::P150 => RowId::UiScale150,
        UiScale::P175 => RowId::UiScale175,
        UiScale::P200 => RowId::UiScale200,
    }
}

/// 行控件 ID → 界面缩放选项；非缩放行返回 None。
pub fn ui_scale_of(id: RowId) -> Option<UiScale> {
    Some(match id {
        RowId::UiScaleAuto => UiScale::Auto,
        RowId::UiScale100 => UiScale::P100,
        RowId::UiScale125 => UiScale::P125,
        RowId::UiScale150 => UiScale::P150,
        RowId::UiScale175 => UiScale::P175,
        RowId::UiScale200 => UiScale::P200,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 界面缩放行与 `settings::UiScale` 一一对应（面板改值才能落到配置）。
    #[test]
    fn ui_scale_rows_map_both_ways() {
        for m in UiScale::ALL {
            assert_eq!(ui_scale_of(ui_scale_row(m)), Some(m), "{m:?} 映射丢失");
            assert!(!m.title().is_empty(), "{m:?} 缺少面板标签");
        }
        assert_eq!(ui_scale_of(RowId::Autostart), None, "非缩放行不应映射");
    }
}
