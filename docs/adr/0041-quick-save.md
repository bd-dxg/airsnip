# ADR 0041：快速保存（路径模板 + 可配快捷键）

- 状态：已采纳（v0.13 开发期）
- 影响范围：`src/settings/{quick_save,hotkey,mod,toolbar}.rs`、
  `src/settings/panel/{mod,savepage,textbox,controls,controls/keyrows,controls/layout,controls/keys,draw,interact,link}.rs`、
  `src/output/{mod,quick,recent}.rs`、`src/overlay/{quicksave,toast,opts,init,mod,annotate,annotate/output,wndproc/mod,wndproc/keys,selection/mod}.rs`、
  `src/app/{actions,pin_edit,filedlg,wndproc}.rs`、`assets/icons/ui/toolbar/quick_save.svg`
- 相关：OUT-2（保存）、SYS-2（快捷键配置，issue #140 的 `KEY_ROWS` 表）、EDT-16（工具栏可见性）、
  ADR 0016（界面缩放）、ADR 0036（移动态）、ADR 0040（快速选区键）

## 背景

现在保存截图只有一条路：按 `Ctrl+S`（硬编码在 `overlay/wndproc`）或点工具栏「保存」，
弹系统「另存为」对话框，选目录、选格式、点保存。截一张图想直接落到固定目录时要走三步交互。
用户要求：**快速保存——按一个可自定义的快捷键，直接按模板路径落盘**。

## 决策

1. **语义**：快速保存 = 按 `保存路径模板` 渲染出的路径**直接写盘，不弹任何对话框**。
   `另存为`（弹对话框）保留，两者并存、各有自己的键。
2. **触发面只在普通截图覆盖层内**。长截图状态条、贴图右键「另存为」保持原样：
   它们各有独立的输出链路与坐标系，本轮不把模板配置往那两条线程里再传一份。
   贴图编辑模式（ADR 0028，选区锁定）不生效，与长截图键 / 快速选区键的既有口径一致。
   没有选区时按下去什么也不做。
3. **两个键都进快捷键配置表**（`hotkey.save_key` / `hotkey.quick_save_key`）：
   - `保存`（另存为）默认 **`Ctrl+S`**（保持现状习惯不变，只是从硬编码搬进配置）；
   - `快速保存` 默认 **`Alt+S`**。
   `KeyBind` 新增第三类 **`OverlayCombo`**：覆盖面内生效、但**键 + 修饰键一起记录**。
   冲突检测按**目标行**的类型分粒度（不是按对方行）：目标是 `Hotkey`/`OverlayCombo` 时比较
   `(vk, mods)`；目标是 `Plain`（长截图 / 工具键，**写回时丢弃修饰键**）时按裸 `vk` 比 ——
   于是 `Ctrl+S` / `Alt+S` / 裸 `S`（长截图）三者天然共存，而「按住 Ctrl 捕获工具键」
   依然会被同键位的另一个工具键拦下（否则写回后就是两个静默重名的裸键）。
   捕获时修饰键由 `held_modifiers()` 记录；清空仍写 `UNBOUND`。
4. **`Alt+S` 的消息路径**：Alt 组合走 `WM_SYSKEYDOWN`（不是 `WM_KEYDOWN`）。
   原先该分支遇到 `alt_down` 一律透传（保留 Alt+F4 / Alt+Tab），现在**先**用 `combo_match`
   比对快速保存 / 保存，命中才接管，其余仍透传。
   覆盖层内的判定顺序：**组合键先于无修饰键的覆盖层键**，否则 `Shift+S` 之类会被长截图键抢走。
5. **路径拆成「目录 + 文件名」两段**（用户反馈「一条完整路径输入框不好用」）：
   `[quick_save] dir`（**靠资源管理器选择**，`panel` 行「保存位置（点击更换）」→ `filedlg::pick_folder`）
   + `[quick_save] name`（**可手改的文件名模板**，不含扩展名，可含变量）。
   渲染时拼成 `目录\文件名.扩展名`（`QuickSaveConfig::template`，目录末尾分隔符会去重）。
   变量：`$yyyy$ $yy$ $MM$ $dd$ $HH$ $mm$ $ss$ $ts$` + `%VAR%` 环境变量展开（目录段也支持，供迁移来的旧配置）。
   **模板不含扩展名**，格式由 `[quick_save] format = png|jpg|webp` 决定。
   **不做分辨率 / 序号变量**：同名冲突已由「自动加序号」兜住。
   迁移：0.13 开发期只有一条完整路径模板（`template`），加载时按最后一个分隔符拆成 `dir` / `name`
   （`QuickSaveConfig::migrate`），遗留字段 `skip_serializing` 不再写入。
6. **预览与保存共用同一个渲染器**（`output::quick::render`）：预览「看到什么」就必须「存到什么」，
   两套算法迟早分叉。预览行**纯展示、不可点**（用户反馈：点一下会弹资源管理器，误点代价大）；
   弹资源管理器只走「打开文件夹」按钮（目录还没被创建时给一句提示，`ShellExecuteW` 静默失败
   会让人以为按钮坏了）。预览用**固定的时间样本**（`Panel::preview_time`，**只在打开面板时
   采样一次**），否则每次重绘（鼠标一动就重绘）都会重算 `$ss$` / `$ts$`，文字会跟着跳。
7. **同名策略可配**：`on_exist = rename（默认）| overwrite`。
   默认「自动加序号」（`name-1.png`、`name-2.png`…）：秒级时间戳下同秒连存两次也不会互相覆盖。
   序号找满 9999 个位置时**报错而不是退回原路径覆盖**（「自动加序号」的语义是永不丢数据）。
   选「覆盖」就按模板覆盖，仅适合 `latest.png` 这类固定名模板。
8. **目录不存在自动创建**（`create_dir_all`）；建不了 / 写失败弹错误框并**保留覆盖层**可重试。
   模板写坏（空串 / 只写到目录 / 文件名含 Windows 非法字符）在写盘前 `validate` 拦住并给出可读提示，
   **不做静默清洗**：预览所见即所得，改错了自己看得见。
9. **落盘在后台线程**，结果经「静态槽 + `PostMessage(WM_QUICK_SAVED)`」回到覆盖层线程：
   编码一张全屏图要几十毫秒，同步做会卡住交互线程（60 FPS 红线）。
   在途期间 `QuickSave::pending` 挡住连按，**也挡住取消**（`Esc` / 右键在在途期间不响应）：
   否则覆盖层先关掉、后台线程仍把文件写出去，用户看到的是「我取消了，但文件还是出现了」。
10. **成功后的反馈**：覆盖层内自绘轻提示（`overlay/toast.rs`，仿 `cancel_hint`），
    显示文件名、**点击 = 定位文件**、2 秒到点自动收起并结束本次截图。
    「显示通知」是设置项（默认开）；关掉就直接收尾。
    可选「同时复制到剪贴板」（默认关）：存盘成功后才复制，失败时不让剪贴板被半成品污染。
    提示可见期间只接受「收尾」：`savekeys::toast_gate` 在 `wndproc` 入口把滚轮 / 右键 / 双击 /
    系统键一律吞掉（否则提示期间还能改线宽、弹二次确认、起长截图）。
11. **工具栏新增第二个按钮**（保留原「保存」按钮的对话框语义）：
    `ToolbarSlot::QuickSave`（键 `file.quicksave`，标签「快存」），图标 `quick_save.svg`
    （Material Symbols `download`：箭头落入托盘，与磁盘图标区分）。
    标签取短名的原因：工具栏页 5 列单元格放不下「快速保存」（有量宽单测守门）。
    文件组 2 → 3 项仍是一行，工具栏高度不变；可见性元素 24 → 25。
12. **设置面板「保存」页重做**：原三个命名单选（日期+时间 / 时间戳 / 仅日期）**退役**，
    由模板取代；`NameStyle` 只作为模板缺失时的**老配置迁移来源**
    （`QuickSaveConfig::migrate` 把 0.13 开发期的整条 `template` 拆成 `dir` / `name`），
    另存为对话框的文件名也从模板预填（只取文件名段，`quick::file_stem`）。
    新行：显示通知、同时复制剪贴板、保存位置（点击更换）、文件名（输入框）、格式、如果文件存在、
    预览、打开文件夹、变量说明。
    面板首次引入**自绘单行输入框**（`panel/textbox.rs`）：字符数组 + 光标，支持插入 / 退格 / 删除 /
    左右 / Home / End / Ctrl+V / IME 结果串；**不做选中与鼠标定位光标**（点框 = 光标到末尾）。
    输入法候选框按 `CANDIDATEFORM(CFS_CANDIDATEPOS)` 锚到自绘光标（与 `overlay/ime.rs` 同法）。
    **下拉是真下拉**（用户反馈）：点开一层列表（`Panel::open_select` / `savepage::draw_list`），
    点选项写回并收起、点列表外只收起且**吞掉点击**（不穿透到下层行）、`Esc` 收起；
    展开时胶囊用强调色边框 + 强调色文字（箭头固定 `▾`，避免 `▴` 缺字形）。
    列表右对齐到胶囊、默认贴下方，下方塞不下翻到上方，最后钳回面板内（`list_rect` 有边界单测）。
    「变量说明」是右上按钮 → 浮层列出全部变量；任意点击先收起浮层并吞掉该次点击。
13. **快捷键页拆成三个子页签**（用户反馈「20 项平铺太乱」）：按**功能**分组 ——
    「截图」（截图 / 再次截图 / 全屏 / 上次区域 4 项）、
    「输出」（保存 / 快速保存 / 贴图 / 贴图边框 / 长截图 5 项）、
    「工具」（移动 + 10 个标注工具 11 项）。分组归元数据表：`KeyRowSpec` 新增 `group: KeyGroup` 字段，
    表的顺序改成「同组相邻」，新增一行也必须填 `group`（漏填会被 `every_key_row_belongs_to_exactly_one_group` 拦下，
    否则那一行在面板上直接消失）。
    工具行的标签同时去掉「工具·」前缀（分组已经表达了上下文，腾出的宽度给键帽框），
    代价是冲突提示变成「按键 矩形 已被其他快捷键使用」——已接受。
    布局：`layout_with(section, scale, group)`，子页签是新的 `Item::Tab`（与行分开，有自己的悬停态 `tab_hover`）。
    页面高度取**三组里最高的**（工具组 6 行），所以切组时窗口不跳高。
    子页签是**纯视图状态**（`Panel::hotkey_group`，不落配置，默认「截图」）；切换时先结束热键捕获，
    避免捕获态挂在一个即将消失的键帽行上。冲突检测仍扫全表 20 项（不因当前组而漏判）；
    底部的「全部恢复默认」与「提示」两行**固定锚在内容区右下角**（三组行数 2/3/6，
    跟着最后一行走的话切页签时会上下跳）：位置按 `H - 2·ROW_H - PAD` 算，两行都右对齐；
    位置一致性由 `hotkey_footer_is_pinned_across_groups` 守门。
    「恢复默认」重置**全部三组**，文案改为「全部恢复默认」以免被误读成「只重置当前组」。
    顺带：面板高 **524 → 520** —— 20 项平铺的 7 行不再存在，快捷键页内容高落到 440（最高仍是工具栏页 482）。
14. **子页签做成分段控件、键帽格回到「标签左、键帽右」**（用户两条反馈）：
    - 页签：三等分等宽单元格拼成一条**分段控件**（共享圆角外框 + 选中项内缩成强调色胶囊，
      `Layout::tab_bar` + `TAB_INSET`）。之前只是三个并排文字，看不出「这里能点」。等宽很重要：
      按文字量给宽会让选中态/不同文字下宽度跳动。
    - 键帽格：ADR 0040 的「标签在上、键帽在下」叠放是为了 20 项三列（格宽 188）时也塞得下；
      分组后每组只有 11 项以下，改用**两列**（格宽 270），标签与键帽回到同一行，与其它设置页的行观感一致。
      单元格高 46（= `ROW_H`），键帽框固定宽 `KEYCAP_W` = 100（容得下 `Shift+F1` 与「按下新键…」），
      清除按钮在键帽框外侧、不重叠（`keycap_rows_have_clear_button` 同时守标签区宽度）。
      因此 `KEY_CELL_H` / `KEY_LABEL_TOP` / `KEYCAP_BOTTOM` 三个叠放专用常量删除，`KeyCap` 行不再走特例分支。

## 踩过的坑（回归教训）

1. **页签文字被容器底盖掉**：分段控件的容器必须在 `Tab` 项**之前**画。当时把容器底的绘制
   写在了 `items` 循环之后，高度/几何全对、测试全绿，界面上却是三个空框。
   现由 `settings/panel/render_tests.rs` 守门：真离屏渲染一页，断言每个页签内缩区域的
   **颜色种类数 ≥ 3**（底色 + 胶囊 + 文字 + 抗锯齿）。把绘制顺序改回去，该测试立刻变红。
2. **`WM_TIMER` ID 撞车**：快速保存提示的 `TIMER_ID` 曾写成 **5**，与候选框过渡动画的 5 相同；
   吸附动画一触发（窗口吸附默认开），`WM_TIMER(5)` 会被提示的处理器抢走 → `finish_quick()` → 关窗。
   表现为**按 F1 截图闪一下就消失**。现为 6，并由 `overlay::wndproc::tests::timer_ids_are_unique`
   把五个定时器 ID（tip 2 / cancel_hint 3 / fade 4 / 候选框动画 5 / 提示 6）钉在同一张表里。
3. **输入框文字溢出**：非编辑态的路径模板原本直接画到底（不裁剪），长模板会越过框右缘压到边框上；
   现与「预览」行共用 `savepage::fit_tail`（尾部截断 + `…`）。
4. **剪贴板读取的 use-after-free**：`read_text_from_clipboard` 先建了指向剪贴板内存的切片、
   再 `GlobalUnlock`、之后才解码——那块内存随时可能被回收。表现为**全量并行跑测试时约 20%
   概率以 `STATUS_HEAP_CORRUPTION` (0xc0000374) 随机崩**。修法：先 `String::from_utf16_lossy`
   拷出来，再 `GlobalUnlock`。
5. **剪贴板用例的并行竞争**：剪贴板是进程级共享资源，一个用例的 `EmptyClipboard` 会把另一个
   用例刚取到的 `HGLOBAL` 变成悬野句柄（同样坏堆）。现在 `output::clipboard::test_lock()`
   把所有碰剪贴板的用例串起来（含既有的图片剪贴板用例）。修完连跑 15 次全绿。
6. **冲突检测拿「对方行」的类型判粒度会漏判**：最初写成「`Plain` 只在候选不带修饰键时才算撞车」，
   而 `Plain` 行的 `set` 是**丢掉修饰键**再写回 —— 于是「按住 `Ctrl` 捕获工具键」能绕过检测，
   写回后得到一个与其他 Plain 行静默重名的裸键。现改为按**目标行**的类型归一口径，
   并由 `plain_target_ignores_held_modifiers` 守住。

## 影响

- **配置新增 `[quick_save]` 段**：`dir`（`Option<String>`，`None` = 用默认目录）、
  `name`（`Option<String>`，`None` = 用默认文件名模板）、`format`、`on_exist`、`notify`、`also_copy`。
  旧配置没有该段时全部取默认，升级后第一次快速保存落到
  `%USERPROFILE%\Pictures\airsnip\airsnip_<日期>_<时间>.png`。
  0.13 开发期的单字段 `template` 只做迁移来源（`skip_serializing`，加载时拆成 `dir` / `name`）。
- **新增模块**：`settings/quick_save.rs`（配置 + 遗留迁移）、`output/quick.rs`（模板渲染 / 校验 / 同名 / 落盘 / 资源管理器定位）、
  `overlay/quicksave.rs`（触发、结果回传与 `QuickSave` 运行时状态）、`overlay/toast.rs`（轻提示）、
  `settings/panel/savepage/{mod,select,help}.rs`（保存页行级逻辑 / 两个真下拉 / 变量说明浮层）、
  `settings/panel/textbox.rs` + `editkeys.rs`（自绘输入框与它的消息翻译）。
- **`SavePrefs` 新增 `name_template`**：另存为对话框的文件名预填走模板（只取文件名段）；目录仍用「上次保存目录」。
- **`output::read_text_from_clipboard`**：输入框 Ctrl+V 用（此前只有写剪贴板的函数）。
- **`app::filedlg::pick_folder`**：`IFileOpenDialog` + `FOS_PICKFOLDERS`；「保存位置（点击更换）」只替换模板的目录段，
  文件名段（含变量）原样保留。
- `KEY_ROWS` 18 → 20（新增「保存」/「快速保存」）+ 每行新增 `group`（ADR 0041 子页签），
  `ToolbarSlot::all()` 24 → 25、`Icon::ALL` 加 `QuickSave`。
- **面板/覆盖层若干文件拆子模块**（本批把几个文件推过 300 行硬上限）：
  `panel/{savepage,widgets}`、`controls/{sections,keyrows/table}`、`overlay/{quicksave,toast,selection/overlays,wndproc/{savekeys,keydown}}`、
  `overlay/selection/overlays.rs`（提示气泡 / 二次确认 / 轻提示三个屏显元素）。

## 已知限制 / 取舍

- 输入框不支持选中与鼠标定位光标、光标不闪烁：路径模板通常只改几个变量，输入完就走。
- **Win 键组合**在覆盖层里可能被系统先吃掉（`Win+*` 属系统保留）；`combo_match` 认 Win 位，但实测覆盖有限。
- 面板 524 高在 720p + 125% 缩放下接近屏幕可用高度，靠既有「夹回屏内」逻辑兜底，极端情况会被裁。
- 工具栏再宽一格：1366×768@200% 下已接近极限，出路仍是「隐藏不用的元素」（ADR 0033 的既有结论）。
- 保存 / 快速保存键**不支持鼠标键**（ADR 0019 的中键 / 侧键路径不接管这两个键），只认键盘。
