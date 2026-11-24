// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板文案键（`settings.*`，130 条）。

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
        AboutHomepage,
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
        /// 关于页底部退款提示（第三方购买请向卖方退款）。
        AboutRefundNote,
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
