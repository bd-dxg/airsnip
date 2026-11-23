// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板控件模型与布局（M3）：左侧分类导航 + 右侧内容区。

use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::SelRect;
use crate::settings::{ToolbarGroup, ToolbarSlot};

mod keyrows;
mod keys;
mod layout;
mod scale;
mod sections;
mod toolbar_layout;
pub use keyrows::*;
pub use keys::*;
pub use layout::*;
pub use scale::*;

/// 窗口内容总宽（横向桌面窗口，宽高比约 1.4）。
pub const W: i32 = 720;
/// 窗口内容总高：固定值，避免切换分类时窗口上下跳动。
/// 最高的分类「工具栏」内容高 482；快捷键页在 ADR 0041 拆成三个子页签后回到 388，
/// 因此面板高回到 520（ADR 0040 为 20 项铺开时的 524 已不需要）。
pub const H: i32 = 520;
/// 左侧导航栏宽。
pub const NAV_W: i32 = 124;
/// 左右边距。
pub const PAD: i32 = 24;
/// 导航项高度。
pub const NAV_H: i32 = 44;
/// 右侧内容区顶部节标题高度。
pub const CONTENT_HEADER_H: i32 = 40;
/// 内容行高度。
pub const ROW_H: i32 = 46;
/// 两列排布时的列间距（快捷键页与「工具栏」页，见 `layout`）。
pub const GRID_GAP: i32 = 8;
/// 单元格内容的水平内边距（多列排布：单元格比整行窄，内边距略小）。
pub const CELLPAD: i32 = 16;
/// 悬停高亮矩形相对内容外扩的距离（让高亮四周留出余量，不再紧贴控件）。
pub const HOVER_PAD: i32 = 8;
/// 支持项目行高度（文案 + 赞赏码，高于标准行）。
pub const QR_ROW_H: i32 = 200;
/// 「工具栏」页的组头行高度（比标准行矮，五组标题不占太多竖向空间）。
/// ADR 0036 从 26 收到 24：工具组多了一个「移动」按钮（ 11 个 / 5 列 = 3 行），
/// 页脚提示与最后一排开关之间的呼吸空间只剩 2px，靠这 10px 补回。
pub const GROUP_H: i32 = 24;
/// 「工具栏」页单元格的水平内边距（多列布局，比整行小）。
pub const CELL_PAD: i32 = 6;
/// 开关胶囊宽度（绘制与命中共用，避免两处各写一份）。
pub const TOGGLE_W: i32 = 36;
/// 快捷键行右侧清除按钮边长（与键帽同高，视觉上齐平）。
pub const CLEAR_BTN: i32 = 30;
/// 键帽与清除按钮之间的间隔。
pub const CLEAR_GAP: i32 = 8;
/// 快捷键单元格高度（ADR 0041 回归「标签左、键帽右」并排后与标准行同高）。
pub const KEY_ROW_H: i32 = 46;
/// 键帽框宽度：能装下 `Shift+F1` 与「按下新键…」即可（右侧固定宽，标签区才稳定）。
pub const KEYCAP_W: i32 = 100;
/// 键帽框高度（与清除按钮同高，视觉上齐平）。
pub const KEYCAP_H: i32 = 30;
/// 路径模板输入框的高度（OUT-2 / ADR 0041）：比行矮，留出上下呼吸空间。
pub const INPUT_H: i32 = 34;
/// 快捷键页子页签行的高度（ADR 0041）。
pub const TAB_H: i32 = 32;
/// 子页签左右内边距（文字之外）。
pub const TAB_PAD: i32 = 16;
/// 分段控件内选中胶囊相对单元格的内缩：有内缩 + 外框才看得出「这是一排可点的按钮」。
pub const TAB_INSET: i32 = 3;

/// 分类页。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    General,
    /// 工具栏可见性（EDT-16，issue #143）：逐元素显隐 + 组头全显 / 全隐。
    Toolbar,
    Hotkeys,
    Save,
    Appearance,
    About,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::General,
        Section::Toolbar,
        Section::Hotkeys,
        Section::Save,
        Section::Appearance,
        Section::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => K::NavGeneral.text(),
            Section::Toolbar => K::NavToolbar.text(),
            Section::Hotkeys => K::NavHotkeys.text(),
            Section::Save => K::NavSave.text(),
            Section::Appearance => K::NavAppearance.text(),
            Section::About => K::NavAbout.text(),
        }
    }
}

/// 控件 ID。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowId {
    /// 开机自启开关。
    Autostart,
    /// 连按两次退出开关（OUT-3，issue #120）：有未保存标注时，同一键连按两次才退出。
    ConfirmCancel,
    /// 界面语言（I18N-1，issue #151 / ADR 0047）：通用页一行真下拉。
    Lang,
    /// 截图热键。
    HotkeyShot,
    /// 贴图热键。
    HotkeyPin,
    /// 再次截图键（覆盖层内按下，默认 Shift+F1，issue #118）：当前画面作为新底图重新框选。
    HotkeyRecapture,
    /// 保存（另存为对话框）键（OUT-2 / ADR 0041，默认 Ctrl+S）：覆盖层内带修饰键的组合键。
    HotkeySave,
    /// 快速保存键（OUT-2 / ADR 0041，默认 Alt+S）：按模板路径直接落盘。
    HotkeyQuickSave,
    /// 贴图边框切换热键（M2b，默认 Ctrl+F3）。
    HotkeyBorder,
    /// 长截图键（覆盖层内按下，默认 S）。
    HotkeyLongshot,
    /// 全屏键（覆盖层内按下，默认 F，ADR 0040）：光标所在显示器的整屏作为选区。
    HotkeyFullscreen,
    /// 上次区域键（覆盖层内按下，默认 R，ADR 0040）：复用上次框选区域。
    HotkeyLastArea,
    /// 工具切换键：移动选区（ADR 0036，框选完成后的缺省态；开 = 指针）。
    ToolMove,
    /// 工具切换键（M3）：矩形 / 箭头 / 文本 / 模糊 / 荧光笔 / 高亮。
    ToolRect,
    ToolArrow,
    ToolText,
    ToolBlur,
    ToolHighlight,
    ToolGlow,
    /// 工具切换键：序号标记（EDT-12）。
    ToolNumber,
    /// 工具切换键：直线（EDT-13）。
    ToolLine,
    /// 工具切换键：椭圆（EDT-14）。
    ToolEllipse,
    /// 工具切换键：手绘自由笔画（EDT-15）。
    ToolFreehand,
    /// 无修饰键风险提示（只读）。
    Widenote,
    /// 一键恢复默认快捷键（SYS-2，快捷键页底部）。
    ResetKeys,
    /// 贴图位置：屏幕中间（M2b）。开 = 居中，关 = 原位置。
    PinPosCenter,
    /// 双击选区空白的动作（ADR 0036）：开 = 贴到屏幕，关 = 复制截图。
    DoubleClick,
    /// 窗口吸附（CAP-6 / ADR 0037，issue #62）：默认开，关闭后行为与未实现本项时一致。
    WindowSnap,
    /// 控件级捕捉（CAP-6 二级 / ADR 0039）：在整窗吸附上再下钻到 UIA 子控件。
    ControlSnap,
    /// 打开日志文件夹（SYS-9 / ADR 0046）：长截图日志常开，用户只需把文件取出来；
    /// 按钮行为复用 `output::quick::open_folder`。
    OpenLogFolder,
    /// 主题：跟随系统。
    ThemeFollow,
    /// 主题：浅色。
    ThemeLight,
    /// 主题：深色。
    ThemeDark,
    /// 界面缩放（CAP-5）：自动（按显示器 DPI + 分辨率）。
    UiScaleAuto,
    /// 界面缩放：固定 100%。
    UiScale100,
    /// 界面缩放：固定 125%。
    UiScale125,
    /// 界面缩放：固定 150%。
    UiScale150,
    /// 界面缩放：固定 175%。
    UiScale175,
    /// 界面缩放：固定 200%。
    UiScale200,
    // 旧版只有一个「整条路径模板」输入框（`QuickTemplate`），ADR 0041 改成目录 + 文件名。
    /// 关于（版本与许可）。
    About,
    /// 项目地址（关于页，点击用浏览器打开）。
    Repo,
    /// 第三方组件声明入口（M5，PRD §9）。
    Licenses,
    /// 检查更新（SYS-7）：右侧显示当前更新状态，整行可点。
    CheckUpdate,
    /// 我要反馈（SYS-8）：不单独占一行，作为「关于」行右侧的按钮；
    /// 点击后把版本 / Windows 版本 / 显示器 / HDR / 显卡写进剪贴板。
    CopyInfo,
    /// 启动时自动检查更新开关（SYS-7）。
    AutoCheckUpdate,
    /// 支持项目（关于页：文案 + 微信赞赏码）。
    Reward,
    /// 快速保存后显示通知（OUT-2 / ADR 0041，默认开）。
    SaveNotify,
    /// 快速保存同时复制到剪贴板（OUT-2 / ADR 0041，默认关）。
    SaveCopy,
    /// 保存目录（OUT-2 / ADR 0041）：只读一行，点击 = 用资源管理器选目录。
    QuickDir,
    /// 文件名模板（OUT-2 / ADR 0041）：自绘输入框（不含扩展名，可含变量）。
    QuickName,
    /// 保存格式下拉（PNG / JPEG / WebP）。
    QuickFormat,
    /// 同名策略下拉（自动加序号 / 覆盖）。
    QuickOnExist,
    /// 模板预览（只读）：渲染后的真实路径。**纯展示、不可点**（ADR 0041 修订：
    /// 曾经点它会弹资源管理器，误点代价大且不是用户预期）。
    QuickPreview,
    /// 打开模板目录。
    SaveOpenFolder,
    /// 变量说明（展开浮层，列出所有可用变量）。
    VarHelp,
    /// 工具栏元素显隐开关（EDT-16）：一个元素一行（多列排布）。
    Toolbar(ToolbarSlot),
    /// 工具栏组头（EDT-16）：整组全显 / 全隐，部分可见时半选。
    ToolbarGroup(ToolbarGroup),
}

/// 行控件类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    /// 开关（整行可点）。
    Toggle,
    /// 组头：组名 + 紧跟组名的三态勾选框（整组全显 / 全隐，EDT-16）。
    Group,
    /// 热键捕获按钮（点右侧区域进入捕获）。
    KeyCap,
    /// 单选（整行可点）。
    Radio,
    /// 只读：左侧标签 + 右侧说明文本。
    Readonly,
    /// 关于块：左侧产品名、右侧版本与许可。
    About,
    /// 外部链接：右侧强调色文字，点击用浏览器打开。
    Link,
    /// 状态行：右侧按钮（文案随状态变，忙时置灰），整行可点。
    Button,
    /// 支持项目块：高行（左侧文案 + 右侧赞赏码）。
    Qr,
    /// 文本输入框（OUT-2 / ADR 0041：「保存」页的路径模板）。
    TextInput,
    /// 下拉选择（OUT-2 / ADR 0041）：点开一层真列表（`Panel::open_select`），点选项生效。
    Select,
}

/// 布局项：导航项、快捷键页的子页签、或内容行。
#[derive(Debug)]
pub enum Item {
    Nav(Section),
    /// 快捷键页的子页签（ADR 0041）：点它切换当前组的键帽行。
    Tab(Tab),
    Row(Row),
}

/// 一个子页签的几何与归属（绘制 / 命中共用）。
#[derive(Debug)]
pub struct Tab {
    pub group: KeyGroup,
    pub rect: SelRect,
}

/// 单行控件几何与标识。
#[derive(Debug)]
pub struct Row {
    pub id: RowId,
    pub kind: RowKind,
    pub label: &'static str,
    /// 标签左内边距（含 DPI 缩放）：整行为 `PAD`，两列单元格较小。
    pub label_pad: i32,
    /// 悬停高亮矩形：内容区四周外扩 `HOVER_PAD`（含圆角绘制），保证高亮有内边距。
    pub hover: SelRect,
    /// 整行矩形（命中区）。
    pub rect: SelRect,
    /// 右侧控件区（开关/按键框/说明文本）。
    pub control: SelRect,
    /// 行内清除按钮（SYS-2，仅 `KeyCap` 行有意义；其余行为空矩形）。
    /// 与 `control` 分开命中：点键帽进捕获态，点这里是直接清空。
    pub clear: SelRect,
}

/// 面板内容布局：导航项 + 当前分类的行 + 总高度。
#[derive(Debug)]
pub struct Layout {
    pub items: Vec<Item>,
    pub height: i32,
    /// 快捷键页子页签的整条分段控件外框（`None` = 当前页没有页签）。
    /// 绘制时先画这层容器，再画各项。
    pub tab_bar: Option<SelRect>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_has_nav_and_rows() {
        let l = layout(Section::General, 1.0);
        let nav: Vec<_> = l
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Nav(s) => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(nav, Section::ALL);
        let rows: Vec<_> = l
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Row(r) => Some(r.id),
                _ => None,
            })
            .collect();
        assert!(rows.contains(&RowId::Autostart));
        assert!(rows.contains(&RowId::Lang));
        assert!(l.height > 0);
    }

    /// 桌面窗口比例：任何分类页的内容都不得把窗口撑高（宽必须大于高），
    /// 行数最多的快捷键页（三列排布后 6 行 + 2 行）也必须落在固定高内。
    #[test]
    fn panel_stays_landscape_for_every_section() {
        for s in [1.0f32, 1.5, 2.0] {
            for sec in Section::ALL {
                let l = layout(sec, s);
                assert!(
                    l.height <= (H as f32 * s) as i32,
                    "{sec:?} @{s}x 内容 {} 超过固定高",
                    l.height
                );
                assert!(
                    l.height < (W as f32 * s) as i32,
                    "{sec:?} @{s}x 变成高 ≥ 宽：{} vs {}",
                    l.height,
                    (W as f32 * s) as i32
                );
            }
            assert_eq!(
                layout(Section::Hotkeys, s).height,
                (H as f32 * s) as i32,
                "@{s}x 快捷键页应落在固定高内"
            );
        }
    }

    /// 快捷键页两列（ADR 0041：标签左、键帽右）：同一视觉行最多两项，且每组只排自己的项。
    #[test]
    fn hotkeys_use_two_columns() {
        for s in [1.0f32, 1.5, 2.0] {
            for g in KeyGroup::ALL {
                let l = layout_with(Section::Hotkeys, s, g);
                let mut ys: std::collections::BTreeMap<i32, usize> = std::collections::BTreeMap::new();
                let mut n = 0;
                for item in &l.items {
                    let Item::Row(r) = item else { continue };
                    if r.kind == RowKind::KeyCap {
                        n += 1;
                        *ys.entry(r.rect.y).or_insert(0) += 1;
                    }
                }
                let want = rows_of(g).count();
                assert_eq!(n, want, "@{s}x {:?} 组应有 {want} 项", g);
                assert!(ys.values().all(|&c| c <= 2), "@{s}x {:?} 同一行不应超过两项", g);
                assert!(ys.len() <= 6, "@{s}x {:?} 行数应受控：{}", g, ys.len());
            }
        }
    }

    /// 子页签（ADR 0041）：三个页签同在一行、互不重叠、向右递增，且都落在内容区内。
    #[test]
    fn tabs_sit_in_one_row() {
        for s in [1.0f32, 1.5, 2.0] {
            let l = layout(Section::Hotkeys, s);
            let tabs: Vec<&Tab> = l
                .items
                .iter()
                .filter_map(|it| match it {
                    Item::Tab(t) => Some(t),
                    _ => None,
                })
                .collect();
            assert_eq!(tabs.len(), KeyGroup::ALL.len(), "@{s}x 应有三个子页签");
            assert_eq!(
                tabs.iter().map(|t| t.group).collect::<Vec<_>>(),
                KeyGroup::ALL.to_vec(),
                "@{s}x 页签顺序应与 ALL 一致"
            );
            let h = (TAB_H as f32 * s) as i32;
            assert!(tabs.iter().all(|t| t.rect.h == h), "@{s}x 页签高度应一致");
            assert!(tabs.iter().all(|t| t.rect.y == tabs[0].rect.y), "@{s}x 应同一行");
            for w in tabs.windows(2) {
                assert!(
                    w[0].rect.x + w[0].rect.w <= w[1].rect.x,
                    "@{s}x 页签重叠：{:?} / {:?}",
                    w[0].rect,
                    w[1].rect
                );
            }
            let nav_w = (NAV_W as f32 * s) as i32;
            let right = (W as f32 * s) as i32;
            for t in &tabs {
                assert!(t.rect.x >= nav_w, "@{s}x 页签应在导航栏右侧：{:?}", t.rect);
                assert!(t.rect.x + t.rect.w <= right, "@{s}x 页签越出面板：{:?}", t.rect);
            }
        }
    }

    /// 分组完整性（ADR 0041）：三组合集必须恰好盖住 `KEY_ROWS`，且一行只属一组。
    /// 漏归组的新行会在面板上直接消失（改表忘填 group 的静默错）。
    #[test]
    fn every_key_row_belongs_to_exactly_one_group() {
        let mut seen: Vec<RowId> = Vec::new();
        for g in KeyGroup::ALL {
            for spec in rows_of(g) {
                assert!(!seen.contains(&spec.id), "{:?} 同时出现在多个组里", spec.id);
                seen.push(spec.id);
            }
        }
        assert_eq!(seen.len(), KEY_ROWS.len(), "有行没归组（或归了不存在的组）");
        for spec in &KEY_ROWS {
            assert!(seen.contains(&spec.id), "{:?} 没有被任何子页签收录", spec.id);
        }
    }

    /// 快捷键单元格（ADR 0041）：标签左、键帽右，两者同一行；键帽框垂直居中且不高出单元格。
    #[test]
    fn keycap_label_and_caps_side_by_side() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let l = layout(Section::Hotkeys, scale);
            for item in &l.items {
                let Item::Row(r) = item else { continue };
                if r.kind != RowKind::KeyCap {
                    continue;
                }
                let top_gap = r.control.y - r.rect.y;
                let bottom_gap = r.rect.y + r.rect.h - (r.control.y + r.control.h);
                assert!((top_gap - bottom_gap).abs() <= 2, "{:?} @{scale}x 键帽应垂直居中", r.id);
                assert!(r.control.h <= r.rect.h, "{:?} @{scale}x 键帽不应高出单元格", r.id);
                assert!(r.control.x > r.rect.x, "{:?} @{scale}x 键帽应在标签右侧", r.id);
            }
        }
    }

    /// 底部两行（全部恢复默认 / 提示）**固定在右下角**（ADR 0041）：
    /// 三组行数不同（2/3/6 行），但两行的 y 必须完全一致——否则切页签时会上下跳。
    #[test]
    fn hotkey_footer_is_pinned_across_groups() {
        for s in [1.0f32, 1.25, 1.5, 2.0] {
            let mut seen: Vec<((i32, i32), (i32, i32))> = Vec::new();
            for g in KeyGroup::ALL {
                let l = layout_with(Section::Hotkeys, s, g);
                let mut btn = None;
                let mut hint = None;
                for item in &l.items {
                    let Item::Row(r) = item else { continue };
                    match r.id {
                        RowId::ResetKeys => btn = Some((r.rect.x, r.rect.y)),
                        RowId::Widenote => hint = Some((r.rect.x, r.rect.y)),
                        _ => {}
                    }
                }
                let (btn, hint) = (btn.expect("应有全部恢复默认"), hint.expect("应有提示行"));
                // 右对齐到内容区右缘、贴底部（两行 + 下边距）
                assert_eq!(btn.0 + 0, hint.0, "@{s}x 两行应同一左缘（整行行）");
                assert_eq!(btn.0, (NAV_W as f32 * s) as i32, "@{s}x 底块应占整行宽度");
                let row_h = (ROW_H as f32 * s) as i32;
                assert_eq!(hint.1, btn.1 + row_h, "@{s}x 提示行应在按钮下方一行");
                let bottom = hint.1 + row_h;
                assert!(bottom <= (H as f32 * s) as i32, "@{s}x 底块超出面板：{bottom}");
                assert!(
                    bottom >= (H as f32 * s) as i32 - (PAD as f32 * s) as i32 - 2,
                    "@{s}x 底块应贴底而不是跟着内容走：{bottom}"
                );
                seen.push((btn, hint));
            }
            for w in seen.windows(2) {
                assert_eq!(w[0], w[1], "@{s}x 切组后底块位置变了：{:?} vs {:?}", w[0], w[1]);
            }
        }
    }

    #[test]
    fn each_section_has_its_rows() {
        for sec in Section::ALL {
            let l = layout(sec, 1.0);
            let rows: Vec<_> = l
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Row(r) => Some(r.id),
                    _ => None,
                })
                .collect();
            let expected = match sec {
                Section::General => vec![
                    RowId::Autostart,
                    RowId::ConfirmCancel,
                    RowId::Lang,
                    RowId::PinPosCenter,
                    RowId::DoubleClick,
                    RowId::WindowSnap,
                    RowId::ControlSnap,
                    RowId::OpenLogFolder,
                ],
                Section::Hotkeys => {
                    // 子页签当前组（默认「截图」）的键帽 + 底部两行
                    let mut want: Vec<RowId> = rows_of(KeyGroup::Capture).map(|s| s.id).collect();
                    want.push(RowId::ResetKeys);
                    want.push(RowId::Widenote);
                    want
                }
                Section::Save => vec![
                    RowId::SaveNotify,
                    RowId::SaveCopy,
                    RowId::QuickDir,
                    RowId::QuickName,
                    RowId::QuickFormat,
                    RowId::QuickOnExist,
                    RowId::QuickPreview,
                    RowId::SaveOpenFolder,
                    RowId::VarHelp,
                ],
                Section::Appearance => vec![
                    RowId::ThemeFollow,
                    RowId::ThemeLight,
                    RowId::ThemeDark,
                    RowId::UiScaleAuto,
                    RowId::UiScale100,
                    RowId::UiScale125,
                    RowId::UiScale150,
                    RowId::UiScale175,
                    RowId::UiScale200,
                ],
                Section::About => vec![
                    RowId::About,
                    RowId::Repo,
                    RowId::Licenses,
                    RowId::CheckUpdate,
                    RowId::AutoCheckUpdate,
                    RowId::Reward,
                ],
                Section::Toolbar => {
                    // 组头 + 组内每个元素各一行（顺序即界面顺序，EDT-16）
                    let mut rows = Vec::new();
                    for g in ToolbarGroup::ALL {
                        rows.push(RowId::ToolbarGroup(g));
                        rows.extend(g.slots().iter().map(|s| RowId::Toolbar(*s)));
                    }
                    rows
                }
            };
            assert_eq!(rows, expected, "节 {sec:?} 行集合错误");
        }
    }

    /// 快捷键行（ADR 0041：标签左、键帽右）：键帽框固定宽、清除按钮在框外侧不压键帽；
    /// 标签在框左且不与键帽重叠。
    #[test]
    fn keycap_rows_have_clear_button() {
        for scale in [1.0f32, 1.5, 2.0] {
            for g in KeyGroup::ALL {
                let l = layout_with(Section::Hotkeys, scale, g);
                for item in &l.items {
                    let Item::Row(r) = item else { continue };
                    if r.kind == RowKind::KeyCap {
                        assert!(r.clear.w > 0 && r.clear.h > 0, "{:?} 应有清除按钮", r.id);
                        assert!(
                            r.control.x + r.control.w <= r.clear.x,
                            "@{scale}x {:?} 键帽框不应与清除按钮重叠",
                            r.id
                        );
                        assert!(r.clear.x + r.clear.w <= r.rect.x + r.rect.w, "清除按钮越出单元格");
                        // 标签区：从单元格左内缘到键帽框左缘，必须放得下真实字宽
                        let spec = crate::render::text::FontSpec::new(16.0 * scale);
                        let (lw, _) = crate::render::text::measure(r.label, &spec);
                        let avail = (r.control.x - (r.rect.x + r.label_pad)) as f32;
                        assert!(lw <= avail, "@{scale}x「{}」宽 {lw}px，超过标签区 {avail}px", r.label);
                    } else {
                        assert_eq!(r.clear.w, 0, "{:?} 非键帽行不应有清除按钮", r.id);
                    }
                }
            }
        }
    }

    /// 任意缩放下键帽框都能装下最长的组合键文本（`Shift+F1`）与捕获提示（`按下新键…`）。
    #[test]
    fn keycap_keeps_usable_width_at_any_scale() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let spec = crate::render::text::FontSpec::new(16.0 * scale);
            let need = ["Shift+F1", "按下新键…"]
                .iter()
                .map(|t| crate::render::text::measure(t, &spec).0)
                .fold(0.0f32, f32::max);
            let l = layout(Section::Hotkeys, scale);
            for item in &l.items {
                if let Item::Row(r) = item
                    && r.kind == RowKind::KeyCap
                {
                    assert!(
                        r.control.w as f32 >= need + 8.0 * scale,
                        "{:?} 在 {scale}x 下键帽过窄：{} < {need}",
                        r.id,
                        r.control.w
                    );
                    assert!(r.clear.w >= 20, "{:?} 在 {scale}x 下清除按钮过小", r.id);
                }
            }
        }
    }

    #[test]
    fn rows_do_not_overlap_nav() {
        let l = layout(Section::Hotkeys, 1.0);
        for item in &l.items {
            if let Item::Row(r) = item {
                assert!(r.rect.x >= NAV_W, "内容行不得进入导航区：{r:?}");
            }
        }
    }

    /// 通用页的标签必须横得下（含新增的「长截图日志（不含密码，可删）」）：
    /// 标准行的标签区 = 从 `rect.x + label_pad` 到控件区左缘，超了会和右侧控件叠在一起。
    #[test]
    fn general_labels_fit_before_controls() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let spec = crate::render::text::FontSpec::new(16.0 * scale);
            let l = layout(Section::General, scale);
            let mut avail = 0i32;
            for item in &l.items {
                let Item::Row(r) = item else { continue };
                let (lw, _) = crate::render::text::measure(r.label, &spec);
                let w = r.control.x - (r.rect.x + r.label_pad);
                assert!(lw <= w as f32, "@{scale}x「{}」宽 {lw}px，超过标签区 {w}px", r.label);
                avail = w;
            }
            // 开关行文案随状态变（ADR 0036）：动态写法同样要放得下
            for text in ["贴图位置·屏幕中间", "双击选区空白·贴到屏幕"] {
                let (lw, _) = crate::render::text::measure(text, &spec);
                assert!(lw <= avail as f32, "@{scale}x 动态文案「{text}」超过标签区 {avail}px");
            }
        }
    }

    #[test]
    fn about_reward_row_is_taller_than_standard() {
        let l = layout(Section::About, 1.0);
        let reward = l
            .items
            .iter()
            .find_map(|i| match i {
                Item::Row(r) if r.id == RowId::Reward => Some(r),
                _ => None,
            })
            .expect("关于页应有支持项目行");
        assert_eq!(reward.rect.h, QR_ROW_H, "赞赏码行高度应为 QR_ROW_H");
        assert_eq!(reward.control.h, QR_ROW_H, "控件区高度应与行一致");
    }

    /// 「工具栏」页单元格必须容得下「标签 + 开关」（5 列很窄，标签太长会压到开关上）。
    /// 用真实字体量宽，改标签或改列数时能当场报错；每档缩放都查（单元格宽是整除，
    /// 余数随缩放变化，1.0 下放得下不等于 1.5 下也放得下）。
    #[test]
    fn toolbar_cells_fit_label_and_toggle() {
        let total = crate::settings::ToolbarSlot::all().count();
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let l = layout(Section::Toolbar, scale);
            let mut cells = 0;
            for item in &l.items {
                let Item::Row(r) = item else { continue };
                let RowId::Toolbar(slot) = r.id else { continue };
                cells += 1;
                let spec = crate::render::text::FontSpec::new(16.0 * scale);
                let (label_w, _) = crate::render::text::measure(slot.label(), &spec);
                let avail = (r.control.x - (r.rect.x + r.label_pad)) as f32;
                assert!(
                    label_w <= avail,
                    "@{scale}x 「{}」宽 {label_w}px，超过单元格可用宽度 {avail}px",
                    slot.label()
                );
                assert!(
                    r.control.x + r.control.w <= r.rect.x + r.rect.w,
                    "@{scale}x 「{}」的开关超出单元格",
                    slot.label()
                );
            }
            assert_eq!(cells, total, "@{scale}x 每个元素都应有开关单元格");
        }
    }

    #[test]
    fn window_is_wider_than_tall() {
        for sec in Section::ALL {
            let l = layout(sec, 1.0);
            assert!(W > l.height, "{sec:?} 窗口应横向（宽 {W} vs 高 {}）", l.height);
        }
    }

    #[test]
    fn rows_fit_inside_window_at_any_scale() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let w = (W as f32 * scale) as i32;
            for sec in Section::ALL {
                let l = layout(sec, scale);
                for item in &l.items {
                    if let Item::Row(r) = item {
                        assert!(r.rect.x + r.rect.w <= w, "{sec:?} 行超出右边界");
                        assert!(r.rect.y + r.rect.h <= l.height, "{sec:?} 行超出下边界");
                        assert!(r.control.x >= r.rect.x, "{:?} @{scale}x 控件区应在行内", r.id);
                    }
                }
            }
        }
    }
}
