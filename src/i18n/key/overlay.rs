// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层与贴图文案键（`tip.*` / `overlay.*` / `pin.*` / `error.*`，49 条）。

keys!(
    OverlayKey,
    Overlay {
        // ── 工具栏悬浮提示：工具（tip.tool.*，EDT-10）──
        TipToolMove,
        TipToolRect,
        TipToolArrow,
        TipToolText,
        TipToolHighlight,
        TipToolBlur,
        TipToolGlow,
        TipToolNumber,
        TipToolLine,
        TipToolEllipse,
        TipToolFreehand,

        // ── 工具栏悬浮提示：控件（tip.*）──
        TipBadgeMenu,
        TipFill,
        TipLineStyle,
        TipUndo,
        TipRedo,
        TipPin,
        TipLongshot,
        TipCopy,
        /// 「另存为」——与快捷键页的 `KeySave`（保存）区分。
        TipSave,
        TipQuickSave,
        /// 色块气泡（`{hex}`）。
        TipColorSwatch,
        /// 渐变色块气泡（`{from}` / `{to}`）。
        TipGradientSwatch,
        /// 圆角控制柄气泡。
        TipRadius,

        // ── 取消截图提示（overlay.cancel.*）──
        /// 鼠标右键的键名（提示里与 `Esc` 并列）。
        CancelKeyRightButton,
        /// 贴图编辑模式下的取消提示（`{key}`）。
        CancelHintEditing,
        /// 普通截图下的取消提示（`{key}`）。
        CancelHintCapture,

        // ── 覆盖层轻提示（overlay.toast.*）──
        /// 快速保存成功（`{name}`）。
        ToastSaved,

        // ── 输出错误（overlay.output.*）──
        OutputPinAllocFailed,
        OutputPinLockFailed,
        /// `{e}`
        OutputCopyFailed,
        /// `{e}`
        OutputSaveFailed,
        /// `{e}`
        QuickSaveFailed,

        // ── 贴图右键菜单与提示（pin.*）──
        PinMenuEdit,
        PinMenuCopy,
        PinMenuSave,
        PinMenuClose,
        PinMenuCloseAll,
        /// `{e}`
        PinCopyFailed,
        /// 贴图数量上限（`{limit}`）。
        PinLimitReached,
        /// 上限提示框标题。
        PinLimitTitle,
        /// 贴图编辑写回时内存分配失败。
        PinEditAllocFailed,
        /// 贴图编辑写回时内存锁失败。
        PinEditLockFailed,

        // ── 窗口与初始化错误（error.*，用户可见 MessageBox）──
        /// `{e}`
        ErrorOverlayWindow,
        ErrorPinWindowClass,
        /// `{e}`
        ErrorPinWindowCreate,
        /// `{e}`
        ErrorPinCreate,
        ErrorPinSize,
        /// 贴图边框（1px 蓝边）合成失败。
        ErrorPinBorderFailed,
        ErrorClipboardOpen,
    }
);
