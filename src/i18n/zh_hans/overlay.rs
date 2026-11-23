// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层与贴图文案：简体中文。

pub(in crate::i18n) fn overlay(k: crate::i18n::OverlayKey) -> &'static str {
    use crate::i18n::OverlayKey::*;
    match k {
        TipToolMove => "移动选区",
        TipToolRect => "矩形工具",
        TipToolArrow => "箭头工具",
        TipToolText => "文本工具",
        TipToolHighlight => "荧光笔",
        TipToolBlur => "高斯模糊",
        TipToolGlow => "聚光灯",
        TipToolNumber => "序号标记",
        TipToolLine => "直线工具",
        TipToolEllipse => "椭圆工具",
        TipToolFreehand => "手绘",

        TipBadgeMenu => "徽标形态",
        TipFill => "填充",
        TipLineStyle => "线型",
        TipUndo => "撤销",
        TipRedo => "重做",
        TipPin => "贴图",
        TipLongshot => "长截图",
        TipCopy => "复制",
        TipSave => "保存（另存为）",
        TipQuickSave => "快速保存",
        TipColorSwatch => "#{hex} · 双击改色",
        TipGradientSwatch => "#{from} → #{to} · 双击换一套",
        TipRadius => "圆角 · 拖动调节",

        CancelKeyRightButton => "右键",
        CancelHintEditing => "再按一次 {key} 放弃标注并退出编辑",
        CancelHintCapture => "再按一次 {key} 放弃标注并退出截图",

        ToastSaved => "已保存 {name} · 点击定位文件",

        OutputPinAllocFailed => "贴图内存分配失败，请关闭部分贴图后重试。",
        OutputPinLockFailed => "贴图内存锁失败。",
        OutputCopyFailed => "复制失败：{e}",
        OutputSaveFailed => "保存失败：{e}",
        QuickSaveFailed => "快速保存失败：{e}",

        PinMenuEdit => "编辑",
        PinMenuCopy => "复制",
        PinMenuSave => "保存",
        PinMenuClose => "关闭",
        PinMenuCloseAll => "关闭全部",
        PinCopyFailed => "复制失败：{e}",
        PinLimitReached => "贴图数量已达上限（{limit} 张）。\n请关闭部分贴图后再试。",
        PinLimitTitle => "数量上限",
        PinEditAllocFailed => "贴图更新内存分配失败。",
        PinEditLockFailed => "贴图更新内存锁失败。",

        ErrorOverlayWindow => "创建覆盖层窗口失败：{e}",
        ErrorPinWindowClass => "贴图窗口类注册失败",
        ErrorPinWindowCreate => "创建贴图窗口失败：{e}",
        ErrorPinCreate => "贴图创建失败：{e}",
        ErrorPinSize => "贴图像素图尺寸非法",
        ErrorPinBorderFailed => "贴图边框合成失败",
        ErrorClipboardOpen => "无法打开剪贴板",
    }
}
