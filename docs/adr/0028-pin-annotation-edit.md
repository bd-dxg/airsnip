# 28. 贴图按空格进入标注编辑（复用截图覆盖层、小窗口就地编辑）

- **Status**: Proposed
- **Context**: issue #56 第 1 点 —— 贴图贴出来后就只是「一张不能改的置顶图」，无法再用截图工具栏补画标注。用户期望：**在贴图上按空格（或右键菜单「编辑」）进入标注编辑状态**，编辑完**原地更新该贴图**，编辑界面**复用现有截图工具栏**。
  - 现状：贴图窗口（`src/pin/`）只有拖动 / 滚轮缩放 / `Ctrl`+滚轮透明度 / 双击关闭 / 右键菜单，**没有任何标注能力**；全仓库唯一的标注编辑器是覆盖层（`overlay::Overlay`，`editor::Document` + 工具栏 + 绘制交互 + 撤销栈全绑在它身上）。
  - 现有「贴图」路径是单向的：覆盖层把选区**烘焙**成图（`baked_selection` → `GlobalAlloc` 载荷）→ 主线程建 `PinWindow`（ADR 0005）。烘焙后原始标注对象已丢弃，贴图只持有像素。
  - `capture/` 已有 `capture_region(x, y, w, h)`（长截图在用），可按区域抓屏，无需整屏。

- **Decision**:
  1. **复用覆盖层，新增「贴图编辑模式」**，而不是在贴图窗口里另起一套标注 UI（后者要复刻绘制/命中/手柄/文本输入/撤销，逻辑必分叉）。
  2. **小窗口就地编辑**（用户选定）：不给覆盖层抓整屏，而是让覆盖层临时做成一个**只包住「贴图内容 + 工具栏余量」的小窗口**。
     - 窗口矩形：宽 = `max(贴图内容宽, 工具栏宽) + 边距`；高 = `贴图内容高 + 间隙 + 工具栏高 + 边距`（工具栏默认在贴图下方，屏幕下方放不下则翻上方）。工具栏宽高用 `toolbar::layout` 以临时大 `screen` 独立测算一次即可，不依赖窗口尺寸。
     - 底图 = `capture_region` **只抓这一小块**（~ 贴图面积，几十 KB 级），窗口 `origin` 即该矩形左上角（复用了覆盖层「只看 `capture.origin_x·y` + 尺寸」的性质）。
     - 编辑模式**跳过暗化**（不调 `dim_pixmap`），窗口外的桌面完全不受影响、照常可见可切。
  3. **seed（缩放采用「甲：所见即所得」）**：
     - `seed.rect` = 贴图内容矩形（覆盖层坐标 = 屏幕矩形 − `capture.origin`）；
     - `seed.pixels` = 贴图**当前显示像素**（`PinWindow::export_content`，即 `scaled` 去边框扩展后的 straight RGBA）；
     - `Overlay::new` 把 `seed.pixels` 写进 `original` / `dimmed` 的 `seed.rect`；`selection = Some(seed.rect)`、`phase = Adjusting`、`locked_selection = true`。
     - **完成**时 `baked_selection` 烘焙的结果就是「当前显示尺寸 + 新标注」；主线程把贴图内容替换为该结果并把**缩放系数设为 1**（屏幕尺寸不变，只是内容位图变成放大后的栅格）。**未缩放的贴图（绝大多数）无任何感知变化。**
  4. **选区锁定的语义**（`locked_selection`）：
     - `pick::pick_target` 不再返回 `SelHandle`（八向手柄）与 `MoveSelection`（空格移动选区）；
     - `interaction::on_lbutton_down` 在锁定态对 `SelHandle` / `MoveSelection` / `None` 直接 return，不再 `begin_select`；
     - **不影响**对象层交互：对象绘制、`Ctrl`+对象移动、对象手柄、曲线控制柄、方向键微调全部沿用。
  5. **完成 / 取消**（覆盖层键位在编辑模式下的映射）：
     - `Enter` / `空格` / 选区内双击 → **完成**：烘焙 → `PostMessage(WM_PIN_EDIT_DONE, payload)`（`payload = GlobalAlloc[u32 w][u32 h][i32 x][i32 y][RGBA]`，`lparam` 携带目标贴图句柄）→ 覆盖层关闭 → 主线程写回**同一张**贴图并显示。
     - `Esc` / 右键 → **取消**：分层回退（先收面板 / 提交文本输入 / 有标注二次确认，`cancel_step` 不变）→ 关闭覆盖层 → 贴图保持原样。
     - `Ctrl+C` / `Ctrl+S`：仍对烘焙结果可用（复制 / 另存为）。
     - 编辑模式下工具栏**隐藏**「贴图 / 长截图 / OCR」三个入口（在覆盖层坐标里不适用）。
  6. **编辑期间隐藏该贴图**（`ShowWindow(SW_HIDE)`），完成/取消后再按新状态显示：① `capture_region` 用 `BitBlt`（无 `CAPTUREBLT`），分层窗口能否被抓到不确定，隐藏后底图确定是干净桌面；② 缩小/放大过的贴图不会在编辑窗口外露出。
  7. **入口**：贴图窗口聚焦按 `空格`（`0x20`，当前未被贴图占用），以及**右键菜单新增「编辑」项**；两者等价。编辑期间置覆盖层运行标志（复用作 `OVERLAY_RUNNING`），避免全局贴图热键在编辑中触发。
  8. **issue #56 第 2 点一并修**：`clipboard::read_image` 在「剪贴板没有图片格式」时不再弹错误框（`from_clipboard` 静默返回），只有「打开了剪贴板但读图失败」这类真异常才提示。

- **Alternatives considered**:
  - **全屏覆盖层（原方案）**：复用最省事，但要再抓一次整屏（~57MB 峰值）、冻结并暗化整个桌面。用户否决，改小窗口。
  - **在贴图窗口内自绘精简工具栏**：改动看似小，但要复刻 `pick::pick_target` / `interaction` / `objedit` / 文本输入 / 撤销，且工具栏绘制与命中也要分叉成第二套——长期维护成本高。否决。
  - **把标注状态机抽成独立 `Annotator` 结构，覆盖层与贴图窗口共用**：最干净，但要动 `Overlay` 几乎全部字段与 `impl`，回归面最大。若未来要「不抓屏、无覆盖层窗口」的原地编辑再考虑。
  - **不抓屏，余量用纯色底**：可行，但工具栏两侧留白看不到桌面，观感差；抓一小块成本极低，保留。
  - **靠双击进入编辑**（Snipaste 习惯）：与现有「双击关闭贴图」冲突。用户明确要空格。

- **Consequences**:
  - 更容易：贴图第一次可被修改；标注能力与截图编辑器**天然一致**（新增工具自动可用）；「选区锁定」是正交小开关，不污染既有框选路径；内存开销从「整屏」降到「贴图 + 余量」。
  - 更难／代价：
    - 覆盖层窗口不再总是整屏，`run()` 需支持「以给定矩形建窗」（实际上 `capture` 已携带 origin 与尺寸，改动有限）；
    - 工具栏宽高需在起覆盖层前先算一次（新增一个小助手），窗口矩形随之确定；
    - 「甲」的代价：缩放过的贴图编辑后内容是**放大后的栅格**，再放大更糊（用户已接受）；
    - 主线程须新增两条消息（`WM_PIN_EDIT_REQUEST` / `WM_PIN_EDIT_DONE`）并维护「被编辑贴图句柄」，编辑期间该贴图隐藏。
  - 新增状态：`Overlay::locked_selection: bool`、`Overlay::pin_edit_target: Option<usize>`、`OverlayOpts.pin_edit: Option<PinEdit>`（`{ rect, pixels }`）；`pin` 模块新增「编辑请求 → 主窗口」与 `PinWindow::replace_content`。
