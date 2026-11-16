// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键行元数据表（issue #140 / ADR 0041）：布局 / 键帽显示 / 冲突检测 / 捕获写回 /
//! 清空都由这一张表驱动。类型与访问器在 `super`，这里只有数据。
//! 新增一行 = `RowId` 加变体 + 本表加一项（**并填上它的 `group`**，
//! 漏填会被 `every_key_row_belongs_to_exactly_one_group` 拦下）。
//! **表顺序即界面顺序**：同组内相邻排列，组与组之间由子页签分隔，同时也决定冲突提示先报哪一项。

use super::super::RowId;
use super::{KeyGroup, KeyRowSpec};
/// 快捷键行元数据（顺序即界面顺序：**同组内相邻**，组与组之间由子页签分隔，ADR 0041）。
pub const KEY_ROWS: [KeyRowSpec; 20] = [
    // ── 子页签「截图」─────────────────────────────────────────────
    KeyRowSpec::hotkey(
        RowId::HotkeyShot,
        "截图",
        KeyGroup::Capture,
        true,
        |hk, _| (hk.screenshot_key, hk.screenshot_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.screenshot_key = vk;
            cfg.hotkey.screenshot_modifiers = mods;
        },
    ),
    KeyRowSpec::hotkey(
        RowId::HotkeyRecapture,
        "再次截图",
        KeyGroup::Capture,
        false,
        |hk, _| (hk.recapture_key, hk.recapture_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.recapture_key = vk;
            cfg.hotkey.recapture_modifiers = mods;
        },
    ),
    // 快速选区（ADR 0040）：全屏 / 上次区域，覆盖层内、无修饰键
    KeyRowSpec::plain(
        RowId::HotkeyFullscreen,
        "全屏",
        KeyGroup::Capture,
        |hk, _| (hk.fullscreen_key, 0),
        |cfg, vk, _| cfg.hotkey.fullscreen_key = vk,
    ),
    KeyRowSpec::plain(
        RowId::HotkeyLastArea,
        "上次区域",
        KeyGroup::Capture,
        |hk, _| (hk.last_area_key, 0),
        |cfg, vk, _| cfg.hotkey.last_area_key = vk,
    ),
    // ── 子页签「输出」─────────────────────────────────────────────
    // 保存 / 快速保存（OUT-2 / ADR 0041）：覆盖层内生效的带修饰键组合，
    // 与无修饰的「长截图 S」共存（不同修饰键 = 不同动作）。
    KeyRowSpec::combo(
        RowId::HotkeySave,
        "保存",
        KeyGroup::Output,
        |hk, _| (hk.save_key, hk.save_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.save_key = vk;
            cfg.hotkey.save_modifiers = mods;
        },
    ),
    KeyRowSpec::combo(
        RowId::HotkeyQuickSave,
        "快速保存",
        KeyGroup::Output,
        |hk, _| (hk.quick_save_key, hk.quick_save_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.quick_save_key = vk;
            cfg.hotkey.quick_save_modifiers = mods;
        },
    ),
    KeyRowSpec::hotkey(
        RowId::HotkeyPin,
        "贴图",
        KeyGroup::Output,
        true,
        |hk, _| (hk.pin_key, hk.pin_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.pin_key = vk;
            cfg.hotkey.pin_modifiers = mods;
        },
    ),
    KeyRowSpec::hotkey(
        RowId::HotkeyBorder,
        "贴图边框",
        KeyGroup::Output,
        false,
        |hk, _| (hk.border_key, hk.border_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.border_key = vk;
            cfg.hotkey.border_modifiers = mods;
        },
    ),
    // 长截图键在覆盖层内按下即触发（无修饰键）
    KeyRowSpec::plain(
        RowId::HotkeyLongshot,
        "长截图",
        KeyGroup::Output,
        |hk, _| (hk.longshot_key, 0),
        |cfg, vk, _| cfg.hotkey.longshot_key = vk,
    ),
    // ── 子页签「工具」─────────────────────────────────────────────
    // ADR 0036：移动（指针）键排在工具键之前——它才是框选完成后的缺省态
    KeyRowSpec::plain(
        RowId::ToolMove,
        "移动",
        KeyGroup::Tools,
        |_, tk| (tk.move_key, 0),
        |cfg, vk, _| cfg.tool_keys.move_key = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolRect,
        "矩形",
        KeyGroup::Tools,
        |_, tk| (tk.rect, 0),
        |cfg, vk, _| cfg.tool_keys.rect = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolArrow,
        "箭头",
        KeyGroup::Tools,
        |_, tk| (tk.arrow, 0),
        |cfg, vk, _| cfg.tool_keys.arrow = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolText,
        "文本",
        KeyGroup::Tools,
        |_, tk| (tk.text, 0),
        |cfg, vk, _| cfg.tool_keys.text = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolBlur,
        "模糊",
        KeyGroup::Tools,
        |_, tk| (tk.blur, 0),
        |cfg, vk, _| cfg.tool_keys.blur = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolHighlight,
        "荧光笔",
        KeyGroup::Tools,
        |_, tk| (tk.highlight, 0),
        |cfg, vk, _| cfg.tool_keys.highlight = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolGlow,
        "聚光灯",
        KeyGroup::Tools,
        |_, tk| (tk.glow, 0),
        |cfg, vk, _| cfg.tool_keys.glow = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolNumber,
        "序号",
        KeyGroup::Tools,
        |_, tk| (tk.number, 0),
        |cfg, vk, _| cfg.tool_keys.number = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolLine,
        "直线",
        KeyGroup::Tools,
        |_, tk| (tk.line, 0),
        |cfg, vk, _| cfg.tool_keys.line = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolEllipse,
        "椭圆",
        KeyGroup::Tools,
        |_, tk| (tk.ellipse, 0),
        |cfg, vk, _| cfg.tool_keys.ellipse = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolFreehand,
        "手绘",
        KeyGroup::Tools,
        |_, tk| (tk.freehand, 0),
        |cfg, vk, _| cfg.tool_keys.freehand = vk,
    ),
];
