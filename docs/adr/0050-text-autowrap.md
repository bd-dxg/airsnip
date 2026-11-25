# 50. 文本自动换行：宽度固化在对象上，折行与光标都交给 DirectWrite

- 状态：已采纳（v1.0.0 之后）
- 影响范围：`src/render/text.rs`（`measure_wrapped` / `draw_wrapped` / `caret_offset` / `remeasure`）、
  `src/editor/mod.rs`（`Kind::Text::wrap`）、`src/editor/transform.rs`（缩放时 wrap 随横向比例）、
  `src/overlay/text_edit_state.rs`（`wrap` 字段与光标改走 layout）、
  `src/overlay/textinput.rs`（`text_wrap_at` / 提交写入 wrap / 再编辑刷新）、
  `src/overlay/ime.rs`（候选框锚点）、`src/render/mod.rs`（绘制传 wrap）
- 相关：issue #221、ADR 0004（自绘文本、不用系统 `EDIT` 控件）、ADR 0034（IME 候选框跟随光标）、
  ADR 0049（上一批的剪贴板互通）

## 背景

issue #221：文本工具输入的文字一路向右延伸，直接超出截图选区（用户视觉上的「截图区域」）的右边界。
用户期望碰到右边界自动换行。

现状是**全程单行排版**：`render::text::measure` / `draw` 建 `IDWriteTextLayout` 时宽度给的是
`MAX_LAYOUT = 100_000`，所以 DirectWrite 的自动换行永远不会触发，多行只可能来自用户自己敲的 `\n`。

真正的难点不在「让 DirectWrite 折行」（那是给个有限宽度的事），而在**光标**：`caret_x` 是
「取光标所在行（按 `\n` 切）的前缀 → `measure` 量宽」，`caret_row` 是「数 `chars[..caret]` 里的 `\n`」。
软换行既不产生 `\n`，也无法用「行号 × 行高」定位，这两条同时失效。自己按字符累加宽度可以算，
但字距、合字、标点禁则都会让光标与字形对不上。

## 决策

### 1. 折行宽度**固化在对象上**（`Kind::Text::wrap: Option<f32>`）

- 创建时按「选区右边界 − 文字起点」定一次，至少 **1 个字宽**（否则选区窄到装不下一个字时，
  每行只有一个字、文字堆成竖列）。
- **拖动手柄缩放选区不重排**：已有标注不会因为调整裁剪范围而跳动。
- **双击再编辑、提交时按当时选区右边界刷新**：想按新宽度重排，进去重新提交一次即可。
  若算出来的宽度小于 2 em（对象已经被拖到选区右侧外），**沿用对象原来的 wrap**，而不是把文字压成一列。
- **缩放文本对象时 wrap 随横向比例缩放**，然后由 `render::text::remeasure` 按新字号 / 新 wrap 重排。
  不这样做的话，「横向拉伸 size」与「按 wrap 重排」会互相打架，包围框与字形分叉
  （ADR 0016 之后 `size` 一直只是缓存，这里把它真正当成缓存用）。

选它而不是「每次重绘按当前选区现算」：现算会让拖动选区时全体文字实时重排（不可预测，且每次重绘都要
给每个文本对象重建 layout），而且文本对象被拖到选区外 / 无选区时没有确定语义。
导出与贴图烘焙也因此与屏幕**逐像素一致**——它们读的是同一个 `wrap`。

### 2. 折行与光标都交给 DirectWrite

- `measure_wrapped(text, spec, wrap)`：宽度给 `wrap`（`None` 仍是 `MAX_LAYOUT`），
  折行点、CJK 逐字、拉丁按词、标点禁则全部由 DirectWrite 决定。
- `caret_offset(text, spec, wrap, char_index) -> (dx, top)`：`HitTestTextPosition` 直接给出
  光标 x 与所在**行顶**。`caret_x` / `caret_row * line_height` 这套手算删除，IME 候选框锚点
  （ADR 0034）跟着改成读 `(dx, top)`。
- `char` 下标 → UTF-16 码元偏移：`HitTestTextPosition` 的 `textPosition` 是码元偏移，
  而内部全是 `char`，需要 `text.chars().take(i).map(char::len_utf16).sum()` 换算（代理对占两位）。

自己实现的替代方案（把 chars 切成「显示行」再逐行绘制）被否掉：要自己实现词边界与标点禁则，
而且为了量行宽得逐字符 `measure`（长文本有性能风险），换来的是「可控但更不准」。

### 3. `size` 仍然是缓存，几何层不动

`editor::text::{bounds, rect}` 与 `editor::hit` 全都只用 `Kind::Text::size`，所以折行只要
「按 wrap 量测后写回 `size`」即可，包围框、命中、手柄、圆形控制柄都不用改结构。

### 4. 允许文字溢出选区**底部**

issue 只要求不超右边界。纵向变高是换行的必然结果，强行裁切会让「打完字看不见」，
而导出本来就是按选区裁的。输入态脏区沿用 ADR 0049 的「选区 ∪ 输入框 region」并集，
所以越出选区的文字也能正确重绘。

## 已知边界

- **行尾空格**：DirectWrite 折行会吃掉行尾空格（排版常规）。光标停在行尾空格处时视觉上可能贴到
  下一行行首附近，这是它的排版规则，不是 bug。
- 软换行不产生 `\n`，所以 `Shift+Enter` 敲的**硬换行**与自动折行是两套语义、可以共存：
  硬换行永远断开，软换行只看宽度。
- 徽标内容编辑（EDT-12 / EDT-20）不折行（`wrap: None`）：徽标是单个数字或 ✓ / ✕。
- 本批**不含**任意区间选区（拖选 / `Shift` 扩选 / 只复制其中一句），见 ADR 0049 §不做。
