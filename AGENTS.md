# AirSnip（轻截）开发指南

## 项目概览

轻量原生截图与标注工具（Windows 11，Rust + windows-rs，纯 CPU 渲染），面向无独显迷你主机。目标：常驻 ≈0 开销、触发即用、框选/箭头/文本/高斯模糊/高亮标注、长截图、贴图。

软件名 **AirSnip（轻截）**：展示层用 `AirSnip` / `AirSnip 轻截`，技术标识（crate、exe、仓库、配置目录、任务计划、窗口类、文件名前缀）一律小写 `airsnip`。v0.5.0 之前叫 `jietu`，改名决策与存量影响见 `docs/adr/0014-rename-to-airsnip.md`。

完整需求见 `screenshot-tool-PRD.md`，架构决策见 `docs/adr/`。

## 项目状态

- 当前版本 **v0.11.0**（正式）：新增工具栏元素可逐个隐藏（EDT-16，issue #143，ADR 0033）+ 撤掉 OCR 总开关、选中对象单独删除（`Del` / `Backspace`，issue #71，ADR 0032）、快捷键表重构与「恢复默认」按钮误触发修复（issue #140）；`main` 已合并 PR #142 / #145 / #148 / #149。发布说明见 `docs/release-notes/v0.11.0.md`。
- 上一版 **v0.10.0**（正式）：新增直线 / 椭圆 / 手绘三个标注工具（EDT-13/14/15，issue #134，ADR 0030）、5 个色块全部可自定义（issue #122 / #123）、取消改为「连按两次」（issue #120，ADR 0027 后续）、长截图 HDR 下内存暴涨修复（issue #128，ADR 0029）；`main` 已合并 PR #126 / #130 / #133 / #137。发布说明见 `docs/release-notes/v0.10.0.md`。上一版 v0.9.0（正式）：新增序号标记工具（EDT-12，issue #82，ADR 0026）、自定义标注颜色（EDT-7，issue #73）、贴图按空格进入标注编辑（issue #56，ADR 0028），并修复飞书粘贴（ADR 0025）、设置面板比例失真、右键误触退出丢失标注（ADR 0027）；`main` 已合并 PR #106–#116。发布说明见 `docs/release-notes/v0.9.0.md`。上一版 v0.8.1：OCR（#61，ADR 0022）随 v0.8.0-beta.1 预发布后转正；`main` 已合并 PR #83–#104（5 项修复：HDR 桌面截图过曝发白、标注无法在已有图形上叠层绘制、托盘图标重试窗口 5 秒→3 分钟、F10 等系统键热键在截图状态失效、系统深/浅色切换后设置面板不跟随，另「取字」统一改名「OCR」）。发布说明见 `docs/release-notes/v0.8.1.md`（预发布版见 `docs/release-notes/v0.8.0-beta.1.md`）。上一版 v0.7.0：M0–M4 与 M5a/M5b 全部交付并合并，`main` 已合并 PR #32–#66（release 版黑窗修复、设置窗口比例与图标、v0.3.1–v0.6.0 发布、开机自启托盘图标消失修复 + `WM_SETTINGCHANGE` 指针加固、`app/mod.rs` 拆分、长截图改纯手动滚动 + 拼接预览 + 输入法屏蔽、保存对话框记住上次目录、界面图标统一、圆角内部控制柄 + 扩展到四类、README 重写 + 头图、提权进程托盘图标自愈、改名 AirSnip + 新应用图标、选区尺寸提示收尾、快捷键清空为「未设置」+ 恢复默认 + 托盘入口、长截图另存为、自动更新），工作区干净。M5a 交付要点：另存为对话框（PNG / JPEG / WebP 无损 + 文件名模板）、选中对象包围框与八向手柄 + `Shift` 约束（`overlay/objedit/`、`editor/transform.rs`）、方向键微调（1px / `Shift` 5px）、系统字体枚举 + 工具栏第二行字体面板（`render::fonts`、`overlay::font`、`overlay::toolbar::fontlist`）；决策见 `docs/adr/0007-drop-delete-and-layering.md`、`0008-font-picker.md`、`0010-radius-handle.md`、`0011-corner-all-kinds.md`；`cargo test` 220 项绿（M5a 时期的快照；另有 3 项手动触发的集成测试：真实网络请求、发布清单验签闭环、线上 Release 端到端，`--ignored`）。
- **发布方式**：正式版（如 v0.9.0）要在 GitHub Release 上传 `latest.json`，v0.7.0 起的老用户会通过自动更新收到本版；预发布（如 v0.8.0-beta.1）则勾 **pre-release** 且**不上传** `latest.json` —— 清单地址是 `releases/latest/download/latest.json`，GitHub 的 `latest` 只指向非 pre-release 的最新 Release，所以正式版用户不会被自动更新到 beta（`version::is_newer_than` 本身认为 `0.8.0-beta > 0.7.0`，保护完全靠 pre-release 标记）。
- 新增标注工具（issue #134，EDT-13/14/15，ADR 0030，`feat/new-annotation-tools-134`）：工具栏新增 **直线**（无箭头，`Shift` 吸附水平/垂直/45°，端点圆端帽）、**椭圆**（`Shift` 画正圆，支持填充）、**手绘自由笔画**（折线点列按 2px 步距采样，渲染用「相邻点中点 + 二次贝塞尔」平滑）。三者都纳入既有矢量对象模型（选中/移动/八向缩放/方向键微调/撤销重做自动可用），并复用 `snap_direction` / `square_corner` 做 Shift 约束。快捷键默认 **8 / 9 / 0**（延续 1-7 数字序列），设置面板可改并参与冲突检测；快捷键页键帽行 12 → 15 项（5 热键 + 10 工具，两列 8 行，仍在固定窗口高度内）。填充开关按「**当前工具或选中对象**是矩形 / 椭圆」判定（选中的矩形即使当前工具是箭头也能改填充；判据收在 `annotate::fill_enabled_for`，绘制与动作共用）。手绘拖动时原地追加点（不整列克隆），避免长笔画 O(n²)；点列用 `Arc` 共享，撤销快照不随笔画长度膨胀；草稿脏区只重绘新增段，不重绘整笔包围盒。**椭圆任一轴退化（纯横向 / 纵向拖）按误触丢弃**——这类对象 `draw_ellipse` / `hit_ellipse` 都会早退，留着就是看不见也选不中的幽灵对象。**直线只有头尾两个端点手柄**（与箭头一致，不画包围框），拖动端点改向/拉伸（`Shift` 吸附），不做八向缩放；椭圆仍为包围框八向缩放（与矩形一致）。图标 `freehand` 用 Material 官方 `draw`（笔 + 手绘轨迹）；`line` / `ellipse` 暂为占位（待换 Material Symbols，已在 `assets/icons/ui/README.md` 标注）。
- 再次截图（issue #118，ADR 0031，`feat/recapture-118`）：设置面板「快捷键」页新增**再次截图**键（`hotkey.recapture_key` / `recapture_modifiers`，默认 `Shift+F1`，可改 / 可清空 / 参与冲突检测，键帽行 15 → 16 项、仍两列 8 行）。**只在截图覆盖层运行期间生效**：热键由覆盖层线程自己注册（`overlay::recapture::register`，`RegisterHotKey` 按线程归属，`WM_HOTKEY` 直接派发到覆盖层窗口），窗口关闭即失效，失败静默（`AIRSNIP_DEBUG=1` 才打印）。按下后把**当前画面**当新底图原地重来：`original ← display`（覆盖层铺满虚拟屏，`display` 就是屏幕内容，所以**不重新抓屏、不重开窗口、不新增全屏内存**）→ 重新暗化（`surface::DIM_K`）→ `first_paint = true` → 整屏重绘；同时清空输入 / 面板 / 提示 / 取消待命 / 标注 / 选区（`recapture::reset_interaction`），工具与样式偏好保留。用途是**截取 AirSnip 自己的界面**（工具栏、选区、标注）。贴图标注编辑模式（ADR 0028，选区锁定）不注册；鼠标键不支持（`RegisterHotKey` 不认）。已知取舍：画面不含鼠标指针；底图就是「当时屏幕上的画面」，上一轮的非选区暗化会一并拍进来（这正是用户当时看到的），故隔着两轮再次截图框同一块桌面会更暗。
- 取消改为「连按两次」+ 系统对话框现代样式（issue #120，ADR 0027 后续，`fix/cancel-confirm-toggle`）：设置「常规」页新增「连按两次 Esc / 右键退出」（`config.confirm_cancel`，默认开，经 `OverlayOpts.confirm_cancel` 传入覆盖层）。有未保存标注时，`Esc` / 右键第一次只弹非模态提示（`overlay/cancel_hint.rs`：选区内部右上角，选区可用高度不到提示两倍 / 宽度塞不下则退到选区上方、上方无空间则下方；琥珀色提醒底；2s 超时自动消失，末尾 0.5s 渐隐），**同一键在 2s 内再按一次才退出**；换键或先做其它操作会解除待命；关掉开关后有标注也直接退出（分层回退其余各层不变，`Confirm` 分支由弹框改为 `arm_cancel`）。取消不再弹系统确认框。`app.manifest` / `app.dev.manifest` 声明通用控件 v6（`Microsoft.Windows.Common-Controls` 6.0.0.0）：此前未声明，`MessageBoxW` 等系统对话框回退经典样式，现统一为 Win10/11 视觉样式（抓屏 / 保存失败等提示一并受益）。
- 选中对象单独删除（issue #71，ADR 0032）：**只给键盘入口**——选中对象后按 `Del` / `Backspace` 删除它，`Ctrl+Z` 一步恢复，不连带撤销其它标注。工具栏按钮与包围框 ✕ 都不加（ADR 0007 的「控件越少越好」仍成立，包围框 ✕ 还会与圆角手柄抢位）；键位**硬编码**、不进设置面板键帽表（默认语义够用，且不想把快捷键页从 8 行撜到 9 行）。实现是 `Document::begin` + `remove` + `commit(true)` 三步（`overlay/delete.rs` 的 `delete_object`），删完立即清空 `selected`（`Vec::remove` 会让后面对象的索引前移）；**常规只重绘被删对象包围盒 + 工具栏**（撤销 / 重做要整屏重绘是因为另一份快照里对象边界不可得，这里边界已知），**唯一例外是删掉最后一个聚光对象**——聚光是「选区内整体暗化 + 聚光区恢复亮度」，删掉后亮度变化覆盖整个选区，只重绘原包围盒会留下暗斑。两处取舍：① `Del` / `Backspace` 不在冲突表里，被用户绑成工具键时**按键让让**——先试删除（`on_delete_selected` 返回 `false` 就不吞键），没选中对象时工具切换照旧生效；② 拖动 / 绘制 / 单击待定中不删（会让拖拽指向已消失的索引）；文本输入中不经过这里（`WM_KEYDOWN` 开头就交给 `on_text_key`）。序号标记**不重排**（与 EDT-12 既有措辞一致）。删掉文本对象后工具栏第二行**不强行收起**（工具仍是文本工具，那一行服务下一次输入，评审确认不改）。
- 工具栏可见性（EDT-16，issue #143，ADR 0033）：设置面板新增「工具栏」页，**逐元素显隐** 24 项（工具 10 / 色块 5 / 填充·撤销·重做·文本第二行 / 贴图·长截图·OCR / 复制·保存），组头一键全显 / 全隐（部分可见半选）。
  **隐藏 ≠ 禁用**：纯视觉偏好，快捷键、输出行为、配置一律照旧（藏了保存按钮 `Ctrl+S` 照样能存，藏了工具仍能用数字键切过去）；默认全可见，所以配置只存**隐藏集合**（`[toolbar] hidden = ["tool.ellipse"]`，稳定字符串键，不用枚举序号）。全部隐藏则工具栏整体不绘制（`layout_ex` 返回 `None`），尺寸提示与出图不受影响。开局回落（`ToolbarConfig::settle_tool` / `settle_color`）放在 `overlay/init.rs`：上次工具 / 当前色块被隐藏时落到第一个可见项，会话内快捷键仍可切过去。
  布局侧：`toolbar::layout_ex` 改接 `Shown { vis, text_row, show_output }`（显隐 + 第二行需求 + 贴图编辑模式**叠加**），**组分隔线改由布局按可见组算出**（`Toolbar::dividers`）——原先是按动作锚点查位置，藏了锚点元素就会错位。面板侧：五组统一 5 列、组头 26px 三态勾选框，不引入滚动（40 + 5×26 + 6×46 = 446 < 520）；标签长度受单元格实测字宽约束（「文本第二行」在 5 列下放不下，页内叫「字体行」，有 DirectWrite 量宽测试守门）；文件组全隐时页脚给一句「`Enter` 仍复制、`Ctrl+S` 仍保存」。
  **同批删掉 OCR 总开关**：`[ocr] enabled` 字段删除（旧值忽略、不迁移），OCR 常开、首次触发才初始化 WinRT；按钮默认可见，是否显示交给可见性，隐藏按钮**不**关掉 OCR 键（按 `T` 仍识别），彻底不用 = 快捷键页把 OCR 键清空（ADR 0017）。已知边界：极端组合（1366×768 @ 200%）下工具栏比屏幕还宽，只能左边距起画、右侧被裁，出路是隐藏元素（有单测记录）。
- 标注叠层（issue #93，ADR 0023）：对象内部按下现在是绘制（拖动画新标注、无位移松手选中），**移动对象改走 `Ctrl` + 拖动本体或拖选中对象的包围框边缘**（方向键微调不变），光标与按下行为一致。按下优先级集中在 `overlay::pick::pick_target`，交互与光标共用它。
- M5 打磨进度：✅ 许可证合规（`deny.toml` + `cargo deny check` 四类全绿；`THIRD-PARTY.md` 由 `scripts/gen-third-party.ps1` 生成；设置面板「关于」页含项目地址、微信赞赏码（`assets/reward-code.png` 编译期嵌入）与第三方声明入口）；✅ 性能剖析（`docs/perf-m5.md`：热键→可见 124ms、交互 4.6ms/帧、待机 0% CPU / 1.2MB，全部红线达标；埋点 `src/perf.rs`，采集 `scripts/perf-capture.ps1` + `scripts/perf.ps1`）；✅ 打包分发（`scripts/package.ps1` → `dist\airsnip-v{版本}-win-x64.zip` + 自动更新用 `.exe`，图标 16–256 八尺寸）；⬜ 多屏混合 DPI 测试（**当前无多屏环境，暂跳**）。
- 自动更新（v0.7.0 交付，看板 #59，ADR 0018）：`src/update/` 九个子模块 —— `manifest`（清单 + ed25519 验签）、`version`（三段式版本比较，预发布不推正式版）、`endpoints`（三个 gh-proxy 镜像 + GitHub 直连兜底）、`http`（WinHTTP，不引 reqwest）、`install`（边下边算 SHA-256 + cmd 脚本替换 exe）、`check`（按镜像顺序取清单）、`state`（跨线程进度）、`notify`（后台线程唤醒界面）。
  - **验签优先于一切字段**：签名载荷为 `airsnip-update-v1\n<版本>\n<小写摘要>`，版本号参与签名，把「新版本号 + 旧包」拼一起会验签失败；纯 hash 不作为安全判断（清单与包同源，能改包的人也能改 hash）。
  - **清单存 GitHub 规范地址**，加速前缀由客户端套用（`endpoints::candidates`），换镜像域名不用重新发版 —— 与 issue 建议的「清单里写死加速地址」不同，理由见 ADR 0018 §4。
  - **更新包是裸 exe**：`package.ps1` 同时产出 zip（手动下载，含文档）与 exe（自动更新），清单指向后者。
  - **替换流程**：下载到 `%TEMP%` → 校验和比对 → 写 `.cmd` → 主进程退出 → 脚本等进程消失后 `copy /y` 覆盖并重启 → 自删；`copy` 失败仍启旧版（避免「更新完打不开」）。
  - **触发**：启动后 3s 一次性 `SetTimer`（回调里立刻 `KillTimer`，不引入常驻轮询），失败静默，发现新版本弹托盘气泡；设置面板「关于」页显示状态并可手动检查 / 一键更新，另有「启动时自动检查」开关。
  - **密钥管理**：私钥在 `%USERPROFILE%\.airsnip\update.key`（**不在仓库**），公钥写死在 `src/update/manifest.rs` 的 `UPDATE_PUBKEY_HEX`；生成用 `cargo run --example update-keygen`，签名用 `cargo run --example update-sign -- --version <ver> --sha256 <摘要>`。**换密钥等于切断旧版本的自动更新能力**。
- OCR（一期，看板 #61，ADR 0022）：框选后按 `T` / 点工具栏「OCR」→ 覆盖层立即关闭 → 主线程弹文字气泡（先「识别中…」后结果）→ 文本区可直接改错字、拖选、右键菜单 → `Esc` 或关闭按钮收起。
  - **本地识别**：`Windows.Media.Ocr`，图片不出本机；`src/ocr/` 下 `local/`（COM 初始化、SoftwareBitmap 转换、引擎创建与等待）+ `lines`（CJK 空格规则）；真实识别有 2 项 `--ignored` 集成测试（中英文各一）。
  - **气泡**：`src/app/bubble/` 跑在**主线程**，所以覆盖层关掉后仍存活（这是选「独立窗口」而非覆盖层内自绘的理由）。形态是**划词卡片**（参照 Bob）：顶部淡图标行（图钉=置顶开关 / 关闭）+ **带 1px 边框的文本区**（聚焦时边框变强调色）+ 底部行（左「复制」/ 右「自动复制」开关，默认关），420px 宽 16px 字，无阴影无标题栏；卡片**不自动关闭**（用户 `Esc` / ✕ 决定）；自动复制开关回写 `[ocr] auto_copy`，与设置面板同一项。除文本区外任意空白处可直接拖窗口。不显示语言标签（语言信息只在「未识别到文字」的提示里出现）。图标按墨水率单独配尺寸（`draw::icon_scale`），否则图钉会看着比关闭小一号。
  - **文本区是原生 `EDIT` 子控件**（ADR 0022 §10，v0.8.0 开发期换掉自绘方案）：输入法候选框跟随光标、双击选词、`Ctrl+A/C/V/X/Z`、右键菜单、长文本滚动跟随全部由系统处理。所以卡片**不能是分层窗口**：`WS_POPUP | WS_CLIPCHILDREN` + `WS_EX_TOOLWINDOW`，圆角靠 `CreateRoundRectRgn` + `SetWindowRgn`（边缘无抗锯齿，可接受），上屏改 `GetDC` + `BitBlt`，底色 alpha 必须拉满（`BitBlt` 不认 alpha）。高度自适应：先定宽度，再 `EM_GETLINECOUNT` × `GetTextMetricsW` 行高（**软换行计入行数**，有 `--ignored` 测试守门）。细节坑：只读态收 `WM_CTLCOLORSTATIC` 不是 `WM_CTLCOLOREDIT`；`Esc` 要子类化 `EDIT` 转投父窗口；`\n` 要转 CRLF 再喂；程序自己 `SetWindowTextW` 也触发 `EN_CHANGE`，用 `syncing` 标记防重入。**行高必须先 `SelectObject` 选入 `WM_SETFONT` 那个字体再 `GetTextMetricsW`**：`GetDC` 返回的 DC 里是类默认字体，不选就报 16px，而 YaHei UI 16px 的真实行距是 21px，乘上行数会把卡片算矮两行（表现为「识别文本完整但卡片显示不全」，有 `line_height_matches_real_layout` 守门）。**卡片高度上限按选区所在显示器的工作区算**（`layout_capped`），否则选区靠屏幕下方时弹窗溢出屏幕。`AIRSNIP_DEBUG=1` 会把送给引擎的原图与识别结果落盘到 `%TEMP%\airsnip-ocr-input.png` / `airsnip-ocr-text.txt`，用于分辨「图裁错了」还是「引擎漏了」。
  - **无总开关**（issue #143）：OCR 常开，`[ocr] enabled` 已删除（旧值忽略、不迁移）；按钮默认可见，是否显示交给「工具栏可见性」，按 `T` 仍可用，彻底不用 = 快捷键页把 OCR 键清空为「未设置」。识别语言 `language`（`auto` 跟随系统首选，设置面板「OCR 翻译」页点一行循环切换）。**首次触发才初始化 WinRT**，待机零成本不变。
  - **输入是选区原图**（不含标注）：用户画的箭头/马赛克不该污染识别结果。
  - **两个实测坑**（写进了 ADR 0022 §8）：位图字节要走 `IBufferByteAccess`（不是 `IMemoryBufferByteAccess`），且 `Buffer::Create` 后必须 `SetLength`；**不调用 `RoUninitialize`**（windows-rs 的激活工厂缓存会悬空，再次 `RoInitialize` 后直接 `0xC0000005`）。
  - **识别率**：**统一 2 倍最近邻放大**（小字号下容易按偏旁拆字）；拼接后把「两侧都是 CJK」的空格全部剥掉。
    早期写的是「长边 ≤1200 才放大」，实测是错的：2003×160 的宽截图 1x 拆 4 处偏旁（`亻旦`/`氵益出`/`氵则量`），2x 全对，**3x 反而变差**。现在固定封顶 2 倍，仅受引擎 `MaxImageDimension` 与像素预算（放大后 ≤16M 像素 ≈ 64MB，4K 全屏选区因此不放大）限制。
  - **行序必须自己排**（v0.8.0 开发期修，ADR 0022 §5/§8）：`OcrResult::Lines()` 的返回顺序**不是阅读顺序**（实测「第1行左段、第2行左段、第1行中段……」交错），而且同一视觉行会被字重变化切成多段。现在每个 `OcrLine` 取 `Word.BoundingRect` 的并集当包围盒，先按**垂直中心**归并成视觉行（阈值半个行高），再行内按**左边界**排序、多段合回一行。
- 贴图标注编辑（issue #56，PIN-7，ADR 0028，`feat/pin-annotation-edit`）：贴图窗口聚焦按 `空格` 或右键菜单「编辑」→ 进入标注编辑态。**复用截图覆盖层**：把它临时做成只包住「贴图内容 + 工具栏」的小窗口（`capture::capture_region` 只抓这一块、跳过暗化，窗口外桌面不受影响），底图用 `surface::stamp` 盖印贴图当前显示像素（`PinWindow::export_content`），选区锁死为内容矩形（`Overlay::locked_selection`，`pick`/`interaction` 不再返回选区手柄与空格移动选区），直接进入标注态。完成（`Enter` / `空格` / 选区内双击）= 烘焙结果经 `WM_PIN_EDIT_DONE` 写回**同一张贴图**（`pin::apply_edit` → `replace_content`：内容替换、缩放复位 100%、按原边框重新烘焙、内容左上角位置不变）；取消 / 复制 / 保存 = 贴图保持原样恢复显示（`WM_OVERLAY_DONE` 里以 `App::pin_edit_target` 兜底 `pin::show`）。编辑期间输出组（贴图 / 长截图 / OCR）整体不出现（`toolbar::layout_ex` 的 `show_output`），`Enter` 键改走 `finish_output`（普通截图仍为复制）。**缩放取舍**：按“所见即所得”在显示像素上编辑，完成即把内容换成该栅格（再放大更糊，已接受）。**issue #56 第 2 点同批修**：剪贴板无图片（复制文字后按 F3）不再弹错误框（`clipboard::ReadError::NoImage` 静默）。
- 自定义标注颜色（issue #73，EDT-7，`feat/custom-color`）：工具栏颜色组为 5 个可编辑色块（issue #73 时是 5 个预设色块 + 1 个自定义色块，issue #122 / #123 后取消第六格）。**不用系统 `ChooseColorW`**（太丑、且从屏幕左上角弹出）——改为**覆盖层内自绘取色面板**（`overlay/toolbar/colorpanel/`，仿 `fontlist`：`mod.rs` 布局 / 命中、`paint.rs` 绘制、`interact.rs` 交互）：色相条（横向渐变）+ 饱和度 / 明度方块（纯色底 + 白 / 黑渐变叠加）+ 2×6 快捷色块，从「＋」色块**上方**水平居中展开（空间不足翻到下方、钳回屏内）。按下即取样、拖动连续预览；面板内空白点击只消费不改状态。**拖动手感优化**：拖动期间只重绘面板区域（不重绘工具栏——每帧重画整条工具栏很贵），工具栏色块留到松手时更新；用 HSV 是否变化去重（同一像素内移动直接跳过重绘；看 HSV 而非结果 RGB，否则灰度区拖色相条时标记不跟手）。**实时预览 + 单步撤销**：拖动时若有选中对象，按下先 `doc.begin` 开一次快照、拖动中直接改 `style.color` 并只重绘其脏区（不写历史），松手按“是否真变色” `doc.commit` 归档为一步撤销；快捷色块走常规 `style_selected`。`custom_color` 写回配置同前（`WM_OVERLAY_DONE` 落盘）。调色板与 HSV / 快捷色常量 / 运行时记忆集中在 `editor/palette.rs`（`SWATCH_COUNT` / `DEFAULT_COLORS` / `QUICK` / `hsv_to_rgb` / `rgb_to_hsv`）。**后续（issue #122 / #123）**：取消单独的第六格自定义色块，改为 5 个色块都可编辑——**单击使用、双击改色**：工具栏动作在按下时生效，而 `CS_DBLCLKS` 下双击的第二击是 `WM_LBUTTONDBLCLK` 而非第二次按下，所以第一击照常选色、`WM_LBUTTONDBLCLK` 里命中色块才展开面板（`Overlay::on_swatch_dblclick`），无需计时器、单击无延迟；面板改写当前色块 `colors[color_index]`（面板打开期间任何工具栏点击都会先收起面板，下标不会变）。色块有悬停提示「#RRGGBB · 双击改色」（第一段十六进制色值、第二段操作方式，改色后重新悬停即更新，ADR 0020 后续）。5 色随 `OverlayOpts.colors` 传入覆盖层，覆盖层线程写 `editor::colors_store`，主线程在 `WM_OVERLAY_DONE` 取出写回 `config.swatch_colors`（同 `output::recent` 的保存偏好模式），下次截图沿用；旧配置的 `custom_color` 不迁移（被忽略，5 色回退默认）。
- 界面图标统一（ADR 0009）：覆盖层工具栏与长截图状态条的图标改为 `assets/icons/ui/` 下的 SVG 资源（`render/icon/` 解析 + 按包围盒归一化绘制），删掉 17 个手绘几何图标与重复实现；`overlay/shape.rs` 承接共用绘制小工具。规格与素材来源见 `assets/icons/ui/README.md`，视觉重量由 `render::icon` 的 4 项单测守门。**v0.6.0 换源（ADR 0015）**：原先 11 个图标来自 iconfont 多作者素材、来源与授权无法确认；现全组 20 个图标统一换成 **Material Symbols（outlined，Apache-2.0）**，风格从「粗细不一的混搭」变为统一的线框风，许可证链也干净了（已在 `THIRD-PARTY.md` 登记）。
- 圆角改为对象上的内部控制柄（ADR 0010）：删掉工具栏二值「圆角」开关（`Action::ToggleRound` / `State.round` / `round.svg`），选中对象时在包围框内侧右上角显示一个白色小圆，朝中心拖增大、朝角拖减小，上限短边一半；新增自绘圆角光标（`overlay/cursor`）。圆角读写集中在 `editor/corner.rs`。**已扩展到四类**（`feat/corner-all-kinds`）：矩形 / 高亮 / 高斯模糊 / 聚光灯共用同一 `corner` 字段与手柄，三处渲染（高亮逐像素插值、模糊备份后按遮罩恢复、聚光恢复亮度）统一走 `render/mask.rs`；`corner == 0` 仍走原快速路径。
- 长截图与输入法打磨（待验证）：自动滚动模式已移除（靠「连续静帧」判定到底会误判，拼接提前结束），改为纯手动滚动 + `Enter` 收尾；长截图键从硬编码 `S` 改为配置项 `hotkey.longshot_key`（默认 `S`，设置面板「快捷键」页可改并参与冲突检测）；截图覆盖层打开后禁用 IME，仅在文本输入时恢复，避免 `S`/回车等按键被输入法截走。
- 文本输入的输入法候选框跟随光标（issue #139，ADR 0034，`fix/ime-candidate-139`）：标注文本是自绘的（ADR 0004），此前**从不告诉 IME 光标在哪**，候选框只能按 IME 自己的默认逻辑摆（用户实测微软拼音 / 清风输入法都飘，且每次位置不同；对照组是 OCR 气泡用原生 `EDIT`，从没这个问题）。现在 `overlay/ime.rs` 的 `sync_ime_window_pos` 用 `CANDIDATEFORM(CFS_CANDIDATEPOS)` + `COMPOSITIONFORM(CFS_POINT)` 上报自绘光标**客户区坐标**（覆盖层是无边框 popup，客户区原点 = 画布原点，所以直接用画布坐标；纯函数 `caret_client_pos` 与 `textinput/draw.rs` 画光标的算式逐项一致，两处各写一份会在改字号/行高时悄悄错位）。**坐标口径踩过的坑**（issue #152 期间发现）：首次实现按「屏幕坐标」传（画布 + `capture.origin_*`），普通截图下两者恰好等价 —— 覆盖层铺满虚拟屏、原点 (0,0) —— 但**贴图编辑**（ADR 0028）的小窗口坐在 `(814,463)` 这种位置时，候选框整体偏出一个窗口原点，表现为「普通截图跟随、贴图编辑不跟随」；实测两个结构体必须同时用客户区坐标，只改其中一个不生效。调用时机：进入输入态（新建 / 双击再编辑）、`WM_IME_STARTCOMPOSITION`（输入法正要显示候选框，关键时机）、每次组合串变更后；无定时器、不进重绘路径（组合期间光标不动，单次上报即可）。只对认 IMM32 `CANDIDATEFORM` 的输入法有效——微软拼音已实测通过，TSF-only 且忽略该结构的第三方输入法若仍不跟随，才需要补 text store（`ITfThreadMgr` / `ITextStoreACP`），等 issue #139 的实测反馈再定。
- 长截图结果预览：`longshot/preview.rs` 在视口左/右剩余空间开一个分层预览窗（底边与视口底边对齐、背景透明，只绘缩略图），显示拼接结果的等比缩略图（`Canvas::thumbnail` 按目标像素抽样，成本与图高无关）；状态条不再显示像素计数，只留操作提示。
- 已知问题记录：覆盖层线程曾因 `WM_DESTROY` 同步派发导致 `GetMessageW` 永久阻塞，每次截图泄漏一份全屏位图集合（每张 +57MB），已在 `wndproc.rs` 的 `WM_DESTROY` 分支加 `PostQuitMessage(0)` 修复；修复后截图退出常驻内存稳定在 ~24MB（debug，代码页 + 堆缓存）。
- 已知问题记录（已修）：开机自启后托盘图标消失的根因是 `theme::is_theme_change` 把 `WM_SETTINGCHANGE` 的 `lParam` 无条件当宽字符串读（`PCWSTR::to_string` → `wcslen`），登录阶段系统/第三方广播 `lParam = 0` 时 `wcslen(NULL)` 访问冲突（`0xC0000005`，`ucrtbase.dll+0x7088f`）直接杀掉常驻进程 —— 任务计划本身是成功的。已改为 `VirtualQuery` 校验内存可读性 + 区域内限长读取（不再走 `wcslen`，垃圾指针与无终止符字符串均安全返回 false），并补 `TaskbarCreated` 广播兜底（explorer 就绪/重启后重新 `NIM_ADD`，见 `app::tray::is_taskbar_created`）。
- 已知问题记录（已修）：选了「跟随系统」却在切换深/浅色时界面不变。两个原因叠加：①主窗口 `handle_setting_change` 只 `let _ = theme::system_theme()`（M0 占位），没有任何消费方；②唯一常驻可见窗口是设置面板，它不处理 `WM_SETTINGCHANGE`，而且 AirSnip 以管理员运行，用户级进程（设置应用/explorer）发的主题广播会被 UIPI 挡在高完整性窗口之外。现在面板自处理 `WM_SETTINGCHANGE`，并用 `settings/panel/theme_watch.rs` 起一个 `RegNotifyChangeKeyValue` 事件监听线程兜底（阻塞等待，无轮询；面板关闭时 drop 停止），变化时投递 `WM_APP + 0x51` → `Panel::refresh_system_theme`（仅 `follow` 生效，指针比较判断调色板是否真变）→ 重绘 + 更新 DWM 标题栏。覆盖层/气泡/托盘菜单本身每次打开都重新 `resolve`，无需常驻监听。
- 开发由 AI 驱动，本文件是进入此仓库的 agent 的上下文。
- 改名 AirSnip（ADR 0014，v0.5.0 之后）：`jietu` → `airsnip` 全量替换 32 个文件 / 86 处，含 crate 名、exe、仓库地址、配置目录 `%APPDATA%\airsnip`、任务计划名、单实例 mutex、窗口类、保存目录 `图片\airsnip`、文件名模板、环境变量 `AIRSNIP_TIMING` / `AIRSNIP_DEBUG`。**未做存量迁移**：升级后需手动删旧任务计划（`schtasks /delete /tn jietu /f`，需管理员）、退出旧版 exe（mutex 已改名，新旧可并存抢热键）、重设一次设置；旧截图留在 `图片\jietu`。`scripts/perf-capture.ps1` 依赖窗口类名与 `[airsnip]` 日志前缀，脚本须与程序同版本。
- 选区提示只留「宽 × 高」+ 字体加大（PR #50，v0.6.0 收尾）：
  - 删掉 HUD 后半段的 `@(x,y)` —— 那是虚拟桌面物理像素，普通用户看不懂。`surface::draw_info_text` 与 `geometry::text_rect()` 两处格式串同步改为 `format!("{} × {}", sel.w, sel.h)`（后者用于估算文本宽度供脏区与工具栏避让），需求侧 `screenshot-tool-PRD.md` 的 `CAP-2` 同步改为「显示选区尺寸」。
  - 字号从 `DEFAULT_GUI_FONT`（11px）提到 **16px 加粗**（`INFO_FONT_SIZE`），信息块高 `INFO_TEXT_H` 22 → 28。
  - **不再用 GDI 文本**：原 `TextOutW` + 灰度抗锯齿在 16px 下 hinting 会把笔画吸到整像素，同一串数字里有的 1px 有的 2px（看起来粗细不均）；关 ClearType 避彩边后更明显。现改为走 `render::text`（DirectWrite 栅格化，与工具栏/标注文字同一套），画在 `display` Pixmap 上、随脏区一起 `present_region` 上传，`surface::draw_info_text` 不再接 `HDC`。
  - 文字与工具栏的避让抽为 `geometry::info_text_below`（两处共用同一侧向决策），`text_rect` 增 `screen_h` / `toolbar_h` 参数；工具栏仍是**选区下方优先**，下方塞不下才翻到上方，并新增 `toolbar_defaults_below_selection` 测试守住该行为。
- 覆盖层界面按显示器缩放（v0.7.0，ADR 0016）：设置面板「外观」页新增「界面缩放」（自动 / 100% / 125% / 150% / 175% / 200%，默认自动），`overlay/ui.rs` 的 `Ui` 把 1080p / 100% 基准像素换算成当前屏幕物理像素（`ui.px()` / `ui.f()`），所有界面尺寸常量改为 `*_BASE`。
  - **自动 = `max(显示器DPI/96, (屏幕面积/1920×1080)^0.3)`**：纯 DPI 方案在 **4K@100%** 下系数仍是 1.0（DPI 就是 96），分辨率项兜底到约 1.52；取较大值保证 4K@150% / 200% 不被重复放大。指数 0.3 而非 0.5 —— 4K@100% 要的是「别太小」，不是「等比放大一倍」。
  - 系数按**光标所在显示器**取（`GetCursorPos` + `MonitorFromPoint` + `GetDpiForMonitor` + `GetMonitorInfoW`）；覆盖层横跨虚拟屏，`GetDpiForWindow` 只会给主屏。夹在 `[1.0, 3.0]`。
  - 覆盖范围：工具栏、选区/对象手柄、尺寸提示、字体面板、长截图状态条 / 视口边框 / 预览窗、自绘光标（十字 / 抓手 / 圆角）。**标注内容（线宽、字号、文字）不缩放**，它们是导出内容，跟着 DPI 变会改输出结果。
  - 自绘光标按画布尺寸缓存，换屏导致尺寸变化时先 `DestroyCursor` 再重建；窗口类 `hCursor` 改用系统 `IDC_CROSS` 兜底，避免窗口类持有会被销毁的句柄。
  - 设置面板也用同一套系数（`Panel::scale = Ui::resolve(config.ui_scale).scale()`，不再是 `GetDpiForWindow/96`）：在面板里选完倍率，面板当场就缩放（`fit_window` 重建 DIB + 重排窗口 + 夹回屏幕内、清空 `hover`），不必关面板再截图。

## 技术栈

- 语言：Rust 1.98（edition 2024）
- 系统 API：`windows` crate（windows-rs 0.62.2）
- 渲染：`tiny-skia` 0.12（纯 CPU 矢量绘制）+ DirectWrite 文本栅格化
- 配置/编码：`serde` + `toml`、`png`
- 链接：**静态链接 MSVC 运行时**（`.cargo/config.toml` 的 `+crt-static`，ADR 0021）——发布 exe 不依赖 VC++ Redistributable

## 常用命令

- 编译检查：`cargo check`
- 运行：`cargo run`（GUI 桌面程序，需 Windows 桌面会话；启动后托盘常驻，F1 截图 / F3 贴图 / 覆盖层内 T 触发 OCR）
- 构建发布版：`cargo build --release`（已配 `lto="fat"`、`opt-level="z"`、`panic="abort"`、`strip=true`）
- 测试：`cargo test`（覆盖：抓屏/剪贴板/对象模型与撤销栈/矢量栅格化/曲线与命中/文本测量与字体（粗斜体渲染、族间测量、未知族名回退）/系统字体枚举/长截图拼接/WebP/变换与手柄/光标映射/布局/字体面板/设置面板布局与 DPI 缩放/设置变更消息内存校验/图标 SVG 解析与视觉重量/圆角读写与手柄/圆角遮罩覆盖率与回贴/OCR 包围盒重排与密集文本不漏行/宽图放大与像素预算/气泡定位不溢出屏幕/EDIT 行高与行数与长文本不截断/工具栏重建判据（缺布局必建）/输入法候选框锚点（副屏负原点、换行位移））；`--ignored` 的是需手动触发的集成测试：真实网络请求、发布清单验签闭环、线上 Release 端到端。**不写用例数量**：每加一个用例就要改一次，维护成本白花，看 `cargo test` 输出即可。
- 格式化检查：`cargo fmt --check`（提交前必须通过）
- 性能采集：`scripts/perf-capture.ps1`（热键延迟 + 帧耗时，需管理员）与 `scripts/perf.ps1`（待机 CPU/内存）；报告见 `docs/perf-m5.md`
- 打包发布：`scripts/package.ps1`（构建 release → 校验体积/图标/PE 导入表 → 输出 `dist\airsnip-v{版本}-win-x64.zip` 与自动更新用的 `.exe`）
- 生成更新清单：`scripts/gen-latest-json.ps1 -Notes "说明"`（算 SHA-256 → 调 `update-sign` 签名 → 输出 `dist\latest.json`，需先跑 package.ps1）
- 更新密钥：`cargo run --example update-keygen`（私钥写 `%USERPROFILE%\.airsnip\update.key`，公钥打印出来填进 `UPDATE_PUBKEY_HEX`）
- 许可证与依赖检查：`cargo deny check`（配置在 `deny.toml`；依赖变更后重跑 `scripts/gen-third-party.ps1` 更新 `THIRD-PARTY.md`）

## 开发流程（AI 驱动约定）

1. **禁止在主分支（main）直接开发**：任何功能、修复、优化都必须新建分支（命名建议 `feat/*`、`fix/*`、`docs/*`），完成后以 Pull Request 提交、经审查后合并回 main。main 只接受 PR 合并，不允许本地直接 push。
2. **先验证，后提交**：完成实现后先自行编译/测试，然后把改动交给用户手动验证（运行、体验、确认交互）。**未得到用户明确确认前，禁止执行 `git commit`、`git push`、创建 PR 或合并**。只有用户说“验证通过 / 可以提交”后才走提交与 PR 流程。
3. 按里程碑推进（PRD §7：M0 → M5，二期另行），每次只实现当前里程碑需求，不做投机性代码。
4. 实现功能前先同步文档：不可逆/高杠杆决策记入 `docs/adr/NNNN-*.md`；需求变更落在 PRD 对应编号。
5. 每个源文件保留头部 `SPDX-License-Identifier: GPL-3.0-only`。
6. 引入新依赖前确认许可证与 GPL-3.0-only 兼容，并说明用途。
7. `windows` crate 按需开启 features，不一次性全量开启。

## 代码约定

- 目录结构固定为 PRD §6.2 模块划分：`app/`、`capture/`、`overlay/`、`pin/`、`editor/`、`render/`、`longshot/`、`output/`、`settings/`、`theme/`、`ocr/`（二期）。
- **单文件源码 ≤300 行**（硬性；`#[cfg(test)]` 测试块不计入）。超出时按职责拆出子模块（如 `overlay/` 下的 `wndproc/geometry/surface/cursor/selection/interaction/annotate/toolbar/`），子模块内 `pub(super)` 共享父模块实现。
- 模块 doc 注释中标注对应需求编号（SYS-1、CAP-2、EDT-1…），便于追溯。
- 标注采用矢量对象模型（底图不变，对象列表叠加），渲染用脏矩形局部重绘。
- 所有坐标统一为物理像素（Per-Monitor V2 manifest）。
- 一期界面仅简体中文，代码层预留 i18n。

## 性能红线（必须守住）

- 待机 CPU 0%（无轮询、无定时器，长截图期间除外）
- 待机内存（Private Working Set）≤ 15 MB
- 热键到覆盖层显示 ≤ 150 ms（1080p）/ ≤ 250 ms（4K）
- 交互 ≥ 60 FPS，不依赖 GPU 合成

## 注意事项

- **程序始终以管理员权限运行**（manifest `requireAdministrator`，见 ADR 0003）：不要改回普通权限，否则无法覆盖/操作管理员窗口；开机自启必须用任务计划（`schtasks`）而非 HKCU Run 键。
- 无 GPU 可用：模糊、缩放、重采样等重操作需盒式近似 + 局部缓存 + 低质量预览，禁止全帧逐像素重算。
- 全局热键 F1/F3 会抢占其他程序按键，设置面板必须给出提示；注册失败要检测并提示冲突。
- 长截图（M4）是最大风险点：采用纯手动滚动（自动滚动易误判到底，已移除），拼接要处理固定区域（页眉/页脚），失败提供撤销最后一段入口。
- 贴图窗口用 `WS_EX_TOPMOST | WS_EX_TOOLWINDOW` 分层窗口，窗口静止时禁止任何重绘。
- 二期 OCR/翻译涉及第三方服务：默认关闭、首次显式授权、密钥用 DPAPI 加密、不留存请求内容。
- **禁止移除 `.cargo/config.toml` 的 `+crt-static`**（ADR 0021）：MSVC 默认动态链接 CRT（`/MD`），去掉后 exe 会导入 `VCRUNTIME140.dll` 与 `api-ms-win-crt-*.dll`，用户机器没装 VC++ 运行库就双击报「找不到 VCRUNTIME140.dll」（已有真实用户反馈）。`scripts/package.ps1` 会解析 PE 导入表拦下这类回归，但**不要靠它兜底**。
- 自动更新私钥（`%USERPROFILE%\.airsnip\update.key`）**绝不入库、不进日志、不外传**；它同时决定「旧版本能否继续自动更新」，丢了只能让用户手动下载新版。
- 发布前跑 `cargo deny check`（配置 `deny.toml`）；依赖变更后用 `scripts/gen-third-party.ps1` 重新生成 `THIRD-PARTY.md`。
