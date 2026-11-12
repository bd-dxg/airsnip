# ADR 0034：自绘文本要把光标位置报给输入法（候选框跟随）

- 状态：已采纳（v0.10.x 开发期；坐标口径在 v0.11.1 开发期修正，见决策 1）
- 影响范围：`src/overlay/ime.rs`（新增 `sync_ime_window_pos` / `caret_client_pos`）；`src/overlay/{textinput,wndproc/mod}.rs`
- 相关：issue #139、ADR 0004（自绘文本 + 直接处理 IME 消息）、ADR 0016（界面缩放）、ADR 0022（OCR 气泡用原生 `EDIT` 作对照）

## 背景

问题反馈（issue #139）：在截图里用文本工具打中文时，**候选框不跟随光标**，飘到屏幕别处，而且每次位置都不一样。
用户实测于微软拼音与清风输入法，2K 屏。

根因不是 IME 的 bug，而是 ADR 0004 的选择留下的缺口：标注文本是自绘的（`tiny-skia` 画字 + 自己画光标），
窗口里**没有系统 caret**，也从未调用过 `ImmSetCandidateWindow` / `ImmSetCompositionWindow`。
输入法因此拿不到插入点，只能按自己的默认逻辑摆候选框（回退到窗口默认位置或鼠标位置，取决于输入法实现）。

对照组：OCR 气泡（ADR 0022 §10）用的是原生 `EDIT` 子控件，候选框跟随、双击选词、右键菜单全部由系统处理，
从没有这个问题 —— 说明缺的正是「把位置告诉输入法」这一步，而不是输入法本身挑食。

## 决策

### 1. 主动上报光标位置：`CANDIDATEFORM(CFS_CANDIDATEPOS)` + `COMPOSITIONFORM(CFS_POINT)`

`Overlay::sync_ime_window_pos()`（`overlay/ime.rs`）在输入态下取 `ImmGetContext(hwnd)`，同一锚点设给两个窗：

```rust
CANDIDATEFORM   { dwStyle: CFS_CANDIDATEPOS, ptCurrentPos: (x, y) }  // 候选框
COMPOSITIONFORM { dwStyle: CFS_POINT,        ptCurrentPos: (x, y) }  // 组合串锚点
```

- **必须是 `CFS_CANDIDATEPOS`，不能留 `CFS_DEFAULT`**：后者等于「让输入法自己决定」，就是本次 bug 的原状。
- **坐标是客户区坐标**（v0.11.1 开发期修正）：覆盖层是无边框 popup（`WS_POPUP`，无标题栏 / 边框），客户区
  原点就是窗口左上角、也就是画布原点，所以直接用画布坐标，不叠加虚拟屏原点。
  **踩过的坑**：最初按「屏幕坐标」传（画布 + `capture.origin_*`）。普通截图下两者恰好等价——覆盖层
  铺满虚拟屏、原点 (0, 0)——所以看不出问题；**贴图编辑**（ADR 0028）的窗口只有贴图那么大、坐在
  `(814, 463)` 这种位置，候选框就整体偏出一个窗口原点的量，表现为「普通截图跟随、贴图编辑不跟随」。
  实测（微软拼音）把 `CANDIDATEFORM` 与 `COMPOSITIONFORM` 同时改成客户区坐标后两边都跟随；
  只改其中一个（候选框屏幕 / 组合串客户区）不生效，两个结构体必须同口径。
  纯函数 `caret_client_pos(edit)` 只依赖输入态、与窗口 / 屏幕位置无关，单测守着「不叠加原点」。
- **锚点取光标底部**（`pos.y + caret_row * line_height + pad*0.4 + font_size*1.1`），与 `textinput/draw.rs`
  画自绘光标的算式逐项一致 —— 两处若各写一份，改字号或行高时会悄悄错位。

### 2. 调用时机：只在「输入态建立」与「组合变化」时，不挂定时器、不进重绘路径

- 进入输入态：`begin_text_input`（新建）/ `begin_edit_text`（双击再编辑），紧跟 `set_ime_enabled(true)`。
- `WM_IME_STARTCOMPOSITION`：**这是关键时机**，输入法正要显示候选框。
- `on_text_composition` 里组合串/结果串变化后：组合串变长会让光标右移，候选框要跟着走。

组合期间光标不移动（移动会打断组合），所以单次上报就够；每次按键多一次 `ImmGetContext` /
`ImmReleaseContext` 的开销可忽略，比挂 `WM_IME_NOTIFY` 之类的分支简单得多。

### 3. 不采用的两条路

- **`CreateCaret` + `SetCaretPos` 养一个系统 caret**：会画出系统光标，与自绘的描边光标重叠；
  而且它只喂 `GetCaretPos`，各家输入法（尤其 TSF 路线）是否读它没有保证，等于赌实现。
- **隐藏 `EDIT` 子控件承载输入**：ADR 0004 已否（`WS_POPUP | WS_EX_TOOLWINDOW` 窗口的焦点链、
  位置/字体同步、回车 Esc 桥接都要额外维护），这里没有推翻它的理由。

## 影响

- 新增 2 项测试（`overlay::ime`）：锚点只取决于输入态（客户区坐标，不叠加屏幕 / 虚拟屏原点）、
  换行与行内输入时锚点随光标位移。
- **坐标口径敏感**：客户区坐标是被实测验证过的口径（微软拼音）。若后续遇到只认屏幕坐标的第三方输入法，
  那是一个一行改动（`caret_client_pos` 结果 + 窗口左上角），不需要重新架构；但两种口径无法同时满足，
  按输入法实测反馈决定，不预先增加分支。
- 只对**认 IMM32 `CANDIDATEFORM`** 的输入法有效。微软拼音（Win11 默认，走 TSF↔IMM32 桥）已实测通过；
  TSF-only 且忽略该结构的第三方输入法若仍不跟随，就得补 text store（`ITfThreadMgr` / `ITextStoreACP`），
  那是一个量级更大的改动，先等 issue #139 的第三方输入法实测反馈再定。
- 组合串仍由我们自绘（既有行为），`COMPOSITIONFORM` 只是告诉输入法锚点。若某输入法在 IMM32 路径下
  也自绘组合串，会出现两份 —— 未观察到该现象，出现时再按输入法逐个处理。
- 光标停在已有文字中间开始组合时，锚点用 `caret_x()` 的真实光标位置；而既有绘制把组合串画在光标
  左侧（`draw_edit` 的下划线 `x0 = caret_x - comp_w`），这个中段组合的显示细节不在本次范围内。
