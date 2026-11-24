// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板鼠标交互（M3）：悬停/点击、窗口自适应。
//! 键盘捕获见 `keys`，窗口过程与绘制见 `wnd` 与 `draw`，消息循环见 `super::run`。

mod keys;

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK};

use crate::app::message_box;
use crate::overlay::geometry::in_rect;
use crate::settings::{GroupState, ThemeSetting};
use crate::theme::{self};

use super::Panel;
use super::controls::{Item, NAV_H, RowId, RowKind, Section};
use super::keycap::Zone;

impl Panel {
    /// 命中：导航区 → (分类, None)；内容区 → (None, 行下标)。
    /// 子页签不算行（它有自己的悬停状态 `tab_hover`）。
    pub(super) fn hover_at(&self, x: i32, y: i32) -> (Option<Section>, Option<usize>) {
        // 通知覆盖区域不报行悬停：否则通知下面的行会跟着亮（SYS-8）
        if super::copyinfo::is_over(self, x, y) {
            return (None, None);
        }
        if x < self.nav_w() {
            let nh = (NAV_H as f32 * self.scale) as i32;
            return (Section::ALL.get((y / nh.max(1)) as usize).copied(), None);
        }
        (None, self.hit_line(x, y).map(|(i, _, _)| i))
    }

    /// 命中的子页签（ADR 0041）：快捷键页顶部那一排。
    pub(super) fn hit_tab(&self, x: i32, y: i32) -> Option<super::controls::KeyGroup> {
        self.layout().items.iter().find_map(|it| match it {
            Item::Tab(t) if in_rect(t.rect, x, y) => Some(t.group),
            _ => None,
        })
    }

    /// 命中当前节的内容行：返回 (行下标, 控件 ID, 行内区域)。
    fn hit_line(&self, x: i32, y: i32) -> Option<(usize, RowId, Zone)> {
        let layout = self.layout();
        let mut idx = 0usize;
        for item in &layout.items {
            if let Item::Row(r) = item {
                if in_rect(r.rect, x, y) {
                    // 按钮行只有按钮本身可点：点在「恢复默认 / 版本更新」按钮外的同一行算点空白
                    // （绘制上该行也不高亮），否则按钮外一大片区域都会触发它。
                    if r.kind == RowKind::Button && !in_rect(super::link::button_rect(self, r), x, y) {
                        return None;
                    }
                    // 「关于」行同理：整行只有「我要反馈」按钮可点（SYS-8）；命中它时按
                    // `RowId::CopyInfo` 返回，让点击逻辑落到反馈动作上（行的 ID 仍是 About）
                    let id = if r.kind == RowKind::About {
                        if !in_rect(super::link::about_button_rect(self, r), x, y) {
                            return None;
                        }
                        RowId::CopyInfo
                    } else {
                        r.id
                    };
                    // 清除按钮在控件区内部，必须先判，否则会被当成键帽进捕获（SYS-2）
                    let zone = if in_rect(r.clear, x, y) {
                        Zone::Clear
                    } else if in_rect(r.control, x, y) {
                        Zone::Control
                    } else {
                        Zone::Label
                    };
                    return Some((idx, id, zone));
                }
                idx += 1;
            }
        }
        None
    }

    pub(super) fn on_click(&mut self, x: i32, y: i32) {
        // 「我要反馈」通知先命中：点其中按钮打开反馈页，点其它位置只吞掉（SYS-8）
        if super::copyinfo::on_click(self, x, y) {
            return;
        }
        // 下拉列表展开时：点击**只**用于选项或关闭，不穿透到其它控件（OUT-2 / ADR 0041）
        if self.open_select.is_some() {
            super::savepage::on_list_click(self, x, y);
            return;
        }
        // 变量说明浮层展开时：任意点击先收起它，并吞掉这次点击（防止误触后面的控件）
        if self.var_help {
            self.var_help = false;
            return;
        }
        // 文本编辑中：任何点击先提交（输入框外点击 = 结束编辑）
        super::textbox::commit(self);
        // 左侧导航：切换分类
        if x < self.nav_w() {
            let nh = (NAV_H as f32 * self.scale) as i32;
            if let Some(sec) = Section::ALL.get((y / nh.max(1)) as usize).copied() {
                self.section = sec;
                self.capturing = None;
                self.hover = None;
            }
            return;
        }
        // 快捷键页的子页签（ADR 0041）：切换当前组；切组先结束捕获，否则捕获态会挂在
        // 一个即将消失的键帽行上。
        if let Some(group) = self.hit_tab(x, y) {
            if self.hotkey_group != group {
                self.end_capture();
                self.hotkey_group = group;
                self.hover = None;
                self.tab_hover = Some(group);
            }
            return;
        }
        let Some((_, id, zone)) = self.hit_line(x, y) else {
            // 点空白处退出捕获态
            self.capturing = None;
            return;
        };
        // 快捷键行（SYS-2）：点键帽进捕获、点右侧清除按钮直接清空
        if super::keycap::is_key_row(id) {
            match zone {
                Zone::Clear => self.clear_key(id),
                Zone::Control => {
                    self.end_capture();
                    self.begin_capture(id);
                }
                Zone::Label => self.end_capture(),
            }
            return;
        }
        // 一键恢复默认（SYS-2，快捷键页底部）
        if id == RowId::ResetKeys {
            self.reset_keys();
            return;
        }
        // 「保存」页的新控件（OUT-2 / ADR 0041）：开关 / 下拉 / 预览 / 按钮 / 变量说明
        if super::savepage::on_click(self, id) {
            return;
        }
        match id {
            // 连按两次退出（OUT-3，issue #120）：下次截图生效
            RowId::ConfirmCancel => {
                self.end_capture();
                self.config.confirm_cancel = !self.config.confirm_cancel;
                self.config_dirty = true;
            }
            RowId::Autostart => {
                self.end_capture();
                let next = !self.autostart_on;
                if crate::app::autostart::set_autostart(next) {
                    self.autostart_on = next;
                    // 同步配置并标记立即应用（config.auto_start 与任务计划一致）
                    self.config.auto_start = next;
                    self.config_dirty = true;
                } else {
                    // 任务计划创建/删除失败（通常是权限不足）：弹窗反馈并维持现状
                    message_box(
                        None,
                        crate::i18n::SettingsKey::AutostartFailed.text(),
                        MB_OK | MB_ICONWARNING,
                    );
                }
            }
            RowId::ThemeFollow => {
                self.end_capture();
                self.set_theme(ThemeSetting::Follow);
            }
            RowId::ThemeLight => {
                self.end_capture();
                self.set_theme(ThemeSetting::Light);
            }
            RowId::ThemeDark => {
                self.end_capture();
                self.set_theme(ThemeSetting::Dark);
            }
            // 界面缩放（CAP-5）：改工作副本 + 立即按新倍率重排面板（ADR 0016），
            // 选完当场就能看到效果（调用方随后 fit_window + 重绘）
            id if super::controls::ui_scale_of(id).is_some() => {
                self.end_capture();
                if let Some(m) = super::controls::ui_scale_of(id)
                    && self.config.ui_scale != m
                {
                    self.config.ui_scale = m;
                    self.scale = crate::overlay::ui::Ui::resolve(m).scale();
                    // 重排后行位置全变：旧悬停行失效（鼠标不动时不会收到新的 WM_MOUSEMOVE）
                    self.hover = None;
                    self.nav_hover = None;
                    self.config_dirty = true;
                }
            }
            // 贴图位置（M2b）：一行开关，开 = 居中、关 = 原位置。
            // 两者都立即写回（不等关面板），这样面板开着时按 F1 截图就是新行为。
            RowId::PinPosCenter => {
                self.end_capture();
                self.config.pin_position = match self.config.pin_position {
                    crate::settings::PinPosition::Center => crate::settings::PinPosition::Original,
                    crate::settings::PinPosition::Original => crate::settings::PinPosition::Center,
                };
                self.config_dirty = true;
            }
            // 双击选区空白的动作（ADR 0036）：开 = 贴图，关 = 复制；同样立即写回
            RowId::DoubleClick => {
                self.end_capture();
                self.config.double_click_action = match self.config.double_click_action {
                    crate::settings::DoubleClickAction::Copy => crate::settings::DoubleClickAction::Pin,
                    crate::settings::DoubleClickAction::Pin => crate::settings::DoubleClickAction::Copy,
                };
                self.config_dirty = true;
            }
            // 窗口吸附（CAP-6 / ADR 0037，issue #62）：下次截图生效（与双击动作、贴图位置同一写回路径）
            RowId::WindowSnap => {
                self.end_capture();
                self.config.window_snap = !self.config.window_snap;
                self.config_dirty = true;
            }
            // 控件级捕捉（ADR 0039）：同样立即写回
            RowId::ControlSnap => {
                self.end_capture();
                self.config.control_snap = !self.config.control_snap;
                self.config_dirty = true;
            }
            // 打开日志文件夹（SYS-9 / ADR 0046，issue #200）：日志常开，这里只负责
            // 把文件取出来。`log_dir` 保证目录存在，不必像「保存」页那样先判存在再提示。
            RowId::OpenLogFolder => {
                self.end_capture();
                crate::output::quick::open_folder(&crate::longshot::debug::log_dir());
            }
            // 官网（关于页）：用浏览器打开，落实「唯一官方下载渠道」
            RowId::Homepage => {
                self.end_capture();
                crate::app::open::url(crate::app::open::HOMEPAGE_URL);
            }
            // 项目主页（关于页）：用浏览器打开
            RowId::Repo => {
                self.end_capture();
                crate::app::open::url(crate::app::open::REPO_URL);
            }
            // 第三方组件声明（M5）：用浏览器打开仓库中的 THIRD-PARTY.md
            RowId::Licenses => {
                self.end_capture();
                crate::app::open::url(crate::app::open::THIRD_PARTY_URL);
            }
            // 我要反馈（SYS-8）：复制系统信息 + 轻通知，2 秒后打开新建 issue 页
            RowId::CopyInfo => {
                self.end_capture();
                self.start_feedback();
            }
            // 检查更新（SYS-7）：无待装更新则检查，已有则下载并安装
            RowId::CheckUpdate => {
                self.end_capture();
                self.start_update();
            }
            // 启动时自动检查开关（SYS-7）
            RowId::AutoCheckUpdate => {
                self.end_capture();
                self.config.auto_check_update = !self.config.auto_check_update;
                self.config_dirty = true;
            }
            // 工具栏元素显隐（EDT-16）：纯视觉偏好，只影响绘制，不动快捷键与输出
            RowId::Toolbar(slot) => {
                self.end_capture();
                let on = !self.config.toolbar.visible(slot);
                self.config.toolbar.set_visible(slot, on);
                self.config_dirty = true;
            }
            // 组头（EDT-16）：部分可见或全隐 → 全显；已全显 → 全隐
            RowId::ToolbarGroup(group) => {
                self.end_capture();
                let on = self.config.toolbar.group_state(group) != GroupState::All;
                self.config.toolbar.set_group_visible(group, on);
                self.config_dirty = true;
            }
            _ => self.end_capture(),
        }
    }

    /// 修改主题并即时刷新面板配色（含系统标题栏）。
    fn set_theme(&mut self, t: ThemeSetting) {
        if self.config.theme == t {
            return;
        }
        self.config.theme = t;
        self.config_dirty = true;
        let resolved = theme::resolve(t);
        self.palette = theme::palette(resolved);
        super::window::apply_dark_titlebar(self.hwnd, resolved == theme::Theme::Dark);
    }
}
