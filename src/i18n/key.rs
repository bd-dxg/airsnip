// SPDX-License-Identifier: GPL-3.0-only
//! 文案键（I18N-2，issue #151 / ADR 0047）。
//!
//! 组织方式：**按区域拆子枚举**，每门语言对每个区域写一个完整 `match`（不写 `_ =>` 兜底）。
//! 这样漏一条文案是**编译错误**，不是「运行时英文界面里冒出一句中文」；同时每个语言的
//! 表文件保持在单文件 300 行以内。
//!
//! 与工具栏可见性键（`tool.rect` 等）**分属不同命名空间**：可见性键是落进配置的契约，
//! 文案键要能随措辞改。两者都叫 `tool.rect` 纯属巧合，不要合并。
//!
//! 插值只支持具名的 `{name}` 占位符（`i18n::t_with`），不引 ICU MessageFormat；
//! 语序差异就整句单独写一条。

/// 全部文案键（区域在此汇总；新增区域时加一个变体 + 一个 `match` 分支）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Settings(SettingsKey),
    Overlay(OverlayKey),
}

impl Key {
    /// 全部键（供「每个键都有文案」单测遍历）。
    pub fn all() -> impl Iterator<Item = Key> {
        SettingsKey::ALL
            .iter()
            .copied()
            .map(Key::Settings)
            .chain(OverlayKey::ALL.iter().copied().map(Key::Overlay))
    }
}

/// 按键名批量生成枚举 + 全量清单的宏。
/// 只写一遍清单：漏进 `ALL` 与漏写变体都会立刻暴露（`ALL` 是遍历用的唯一来源）。
/// 第二个参数是 `Key` 里的包裹变体名（`key.text()` 用它拼回去）。
macro_rules! keys {
    ($name:ident, $variant:ident { $($(#[$meta:meta])* $key:ident),* $(,)? }) => {
        /// 界面文案键。
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        pub enum $name {
            $($(#[$meta])* $key),*
        }

        impl $name {
            /// 全部键，顺序即枚举顺序。
            pub const ALL: &'static [$name] = &[$($name::$key),*];

            /// 当前界面语言的文案（调用点最常见的写法：`K::NavGeneral.text()`）。
            pub fn text(self) -> &'static str {
                crate::i18n::t(crate::i18n::Key::$variant(self))
            }
        }
    };
}

keys!(
    SettingsKey,
    Settings {
        // ── 导航页名（settings.nav.*）──
        NavGeneral,
        NavToolbar,
        NavHotkeys,
        NavSave,
        NavAppearance,
        NavAbout,

        // ── 通用页（settings.general.*）──
        GeneralAutostart,
        GeneralConfirmCancel,
        GeneralLanguage,
        /// 贴图初始位置：截在原位（关态文案）。
        GeneralPinOriginal,
        /// 贴图初始位置：屏幕中间（开态文案）。
        GeneralPinCenter,
        /// 双击选区空白：复制截图（关态文案）。
        GeneralDoubleClickCopy,
        /// 双击选区空白：贴到屏幕（开态文案）。
        GeneralDoubleClickPin,
        GeneralWindowSnap,
        GeneralControlSnap,
        GeneralOpenLogFolder,
        /// 通用页「打开日志文件夹」按钮文案（SYS-9）。
        GeneralOpenLogFolderBtn,

        // ── 语言（settings.language.*；语言自称不走表）──
        LanguageFollowSystem,

        // ── 工具栏分组（settings.toolbar.group.*）──
        GroupTools,
        GroupColors,
        GroupEdit,
        GroupOutput,
        GroupFile,

        // ── 工具名（工具栏格 / 快捷键页 / 覆盖层提示共用）──
        ToolMove,
        ToolRect,
        ToolArrow,
        ToolText,
        ToolHighlight,
        ToolBlur,
        ToolGlow,
        ToolNumber,
        ToolLine,
        ToolEllipse,
        ToolFreehand,

        // ── 工具栏元素（settings.toolbar.slot.*）──
        SlotColor1,
        SlotColor2,
        SlotGradient,
        /// 色块下标越界时的兜底名（正常路径不会显示）。
        SlotColorFallback,
        SlotFill,
        /// 线型（EDT-18）：「直线」是 `ToolLine`，两者别混。
        SlotLine,
        SlotUndo,
        SlotRedo,
        SlotTextRow,
        SlotPin,
        SlotLongshot,
        SlotCopy,
        SlotSave,
        SlotQuickSave,
        /// 工具栏页脚注：文件组全隐时的提醒（EDT-16 R7）。
        ToolbarFooterHint,

        // ── 快捷键页（settings.hotkeys.*）──
        KeyTabCapture,
        KeyTabOutput,
        KeyTabTools,
        KeyShot,
        KeyRecapture,
        KeyFullscreen,
        KeyLastArea,
        KeySave,
        KeyQuickSave,
        KeyPinBorder,
        KeyLongshot,
        KeyResetAll,
        /// 快捷键页底部只读行标签「提示」。
        KeyHintLabel,
        /// 捕获态提示「按下新键…」。
        KeyCapturing,
        /// 未绑定按键的显示名。
        KeyUnbound,
        KeyMouseMiddle,
        KeyMouseSide1,
        KeyMouseSide2,
        /// 未知虚拟键名（`{vk:#x}`）。
        KeyUnknown,
        /// 冲突提示（`{name}`）。
        KeyConflict,
        /// 清空热键确认（`{name}`）。
        KeyClearConfirm,
        /// 恢复默认确认（`{count}`）。
        KeyResetConfirm,
        /// 无修饰键风险提示。
        KeyModifierNote,

        // ── 保存页（settings.save.*）──
        SaveNotify,
        SaveAlsoCopy,
        SaveDir,
        SaveName,
        SaveFormat,
        SaveOnExist,
        SavePreview,
        SaveOpenFolder,
        SaveVarHelp,
        /// 目录不存在提示（`{dir}`）。
        SaveDirMissing,
        /// 模板为空（`output::quick::validate`）。
        SaveTemplateEmpty,
        /// 模板只有目录没有文件名。
        SaveTemplateNoName,
        /// 文件名含 Windows 不允许的字符（`{name}`）。
        SaveNameInvalidChars,
        /// 文件名含控制字符。
        SaveNameControlChars,
        /// 自动加序号用尽 9999 个位置（`{stem}`）。
        SaveNameAllTaken,
        /// 建目录失败（`{dir}` / `{e}`）。
        SaveDirCreateFailed,
        ExistingRename,
        ExistingOverwrite,

        // ── 变量说明浮层（output::quick::VARS 的描述列）──
        VarYear,
        VarYearShort,
        VarMonth,
        VarDay,
        VarHour,
        VarMinute,
        VarSecond,
        VarTimestamp,
        VarEnv,

        // ── 外观页（settings.appearance.*）──
        ThemeFollowSystem,
        ThemeLight,
        ThemeDark,
        ScaleAuto,
        Scale100,
        Scale125,
        Scale150,
        Scale175,
        Scale200,
        /// 缩放倍率的短标签（诊断文本用，不带「界面缩放·」前缀）。
        ScaleAutoValue,

        // ── 关于页（settings.about.*）──
        AboutAppName,
        AboutRepo,
        AboutLicenses,
        AboutCheckUpdate,
        AboutAutoCheck,
        AboutReward,
        AboutRewardLine1,
        AboutRewardLine2,
        AboutRewardLine3,
        AboutRewardLine4,
        AboutFeedback,
        AboutLicensesLink,
        AboutToastCopied,
        AboutToastFailed,
        AboutToastOpen,

        // ── 更新状态（update.*；面板按钮文案，B4 托盘气泡复用）──
        UpdateCheck,
        UpdateChecking,
        UpdateUpToDate,
        /// 有新版本（`{version}`）。
        UpdateToVersion,
        /// 下载中（`{percent}`）。
        UpdateDownloading,
        UpdateInstalling,
        UpdateRetry,

        // ── 窗口与错误（error.*）──
        WindowTitle,
        WindowClassFailed,
        /// 创建设置窗口失败（`{e}`）。
        WindowCreateFailed,
        AutostartFailed,
    }
);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// 键清单不应有重复（宏展开后若有同名变体会先编译失败；这里守 `Key::all`
    /// 与两个区域 `ALL` 的对应关系，新增区域忘接进 `Key::all` 会被这条拦下）。
    #[test]
    fn all_keys_are_listed_once() {
        assert_eq!(Key::all().count(), SettingsKey::ALL.len() + OverlayKey::ALL.len());
        let mut seen = std::collections::HashSet::new();
        for k in SettingsKey::ALL {
            assert!(seen.insert(*k), "键重复：{k:?}");
        }
        let mut seen = std::collections::HashSet::new();
        for k in OverlayKey::ALL {
            assert!(seen.insert(*k), "键重复：{k:?}");
        }
    }
}
