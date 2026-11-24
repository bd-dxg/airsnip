// SPDX-License-Identifier: GPL-3.0-only
//! 长截图文案键（`longshot.*`，issue #151 B3，11 条）：状态条默认提示与按钮气泡、
//! 状态条上的终态 / 失败提示、初始化与建窗错误。
//!
//! 不含：`Session::status`（只给调试与单测读，字段注释已标明）、窗口标题与窗口类名
//! （`airsnip.longshot` 等技术标识）、拼接日志（`i18n:keep`），理由见 ADR 0047。

keys!(
    LongshotKey,
    Longshot {
        /// 状态条默认提示（无异常时的常驻一行）。
        BarIdle,
        /// 「撤销」按钮气泡。
        BarTipUndo,
        /// 「另存为」按钮气泡。
        BarTipSaveAs,
        /// 「完成」按钮气泡（复制 + 落盘 + 结束，别被读成只复制）。
        BarTipDone,
        /// 「取消」按钮气泡。
        BarTipCancel,

        /// 状态条：用户取消了。
        HintCancelled,
        /// 状态条：拼接完成。
        HintFinished,
        /// 状态条：连续定位失败，提示向回滚动。
        HintFailed,
        /// 状态条：连续失败且画面一直在变（典型是目标窗口半透明、背景不透明度 < 100%），
        /// 提示把不透明度调到 100%。
        HintTransparent,

        /// `{e}`
        ErrorInit,
        /// `{e}`
        ErrorWindow,
        /// 视口尺寸为 0（会经 `ErrorInit` 显示）。
        ErrorViewport,
    }
);
