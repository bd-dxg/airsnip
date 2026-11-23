// SPDX-License-Identifier: GPL-3.0-only
//! English 文案（区域分文件：设置面板 / 覆盖层与贴图）。

mod app;
mod overlay;
mod settings;

pub(in crate::i18n) use app::app;
pub(super) use overlay::overlay;
pub(super) use settings::settings;
