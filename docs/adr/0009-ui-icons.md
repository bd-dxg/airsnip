# 9. 界面图标统一（SVG 图标集 + 包围盒归一化）

- **Status**: Accepted
- **Context**: 覆盖层工具栏的 19 个按钮图标、长截图状态条的 3 个按钮图标，此前全部是**手绘几何**（`overlay/toolbar/icons.rs` + `icons/actions.rs`）：每个图标各写一套 `PathBuilder`、各自的线宽（1.4/1.5/1.6/1.8/2.0/2.2）、各自的端点样式（Round 与 Butt/Miter 混用）、各自硬编码的偏移魔数（`r.x + 6.0`、`r.x + 8.0`）。结果是 17 个图标 17 种画法，视觉重量差 2–3 倍，且撤销图标在 `longshot/bar.rs` 里被重复实现了一遍（半径 6.5 vs 6.0）。B/I/字号加减号又走 DirectWrite 字形，与几何图标不同源。需要一个可维护、可替换、风格一致的方案。
- **Decision**:
  1. **图标是资源文件，不是代码**：`assets/icons/ui/{toolbar,status}/*.svg`，文件名即 `Icon` 变体的小写形式（`Icon::Longshot` → `toolbar/longshot.svg`），`render/icon/data.rs` 用 `include_str!` 一一对应，缺文件编译期报错。
  2. **选 SVG 而非字体图标**：本项目的渲染管线是 tiny-skia 矢量 + DirectWrite 文本。SVG path 能直接喂给 `tiny_skia::Path` 并用 `fill_path` 绘制，颜色由主题调色板注入；字体图标只能走 DirectWrite glyph run，线宽/端点/描边全部失控，且小尺寸（26px）下更容易糊。嵌入自定义字体还要 `AddFontMemResourceEx` + 自建 font collection，约 200 行平台代码；而 SVG 只需一个约 180 行的解析器。体积上 20 个 SVG 约 11 KB，一套图标 TTF 要 30–80 KB。
  3. **解析器只做受控子集**：`render/icon/svg/` 支持 `<path d>` 的 `M L H V C S Q T A Z`（绝对+相对）、`fill-rule="evenodd"`、跳过 `fill="none"`；不支持 `<g>`/`transform`/渐变/`style`。规范写在 `assets/icons/ui/README.md`，新增图标必须遵守，解析器不为兼容性做妥协。
  4. **绘制按包围盒归一化**：`render::icon::draw` 取该图标所有 path 的并集包围盒，等比缩放到按钮边长的 76%（其余为光学留白）并居中。这样素材原始 viewBox 是 1024/1000/1035×1024 都无所谓，也不会出现「有的图标看着大、有的看着小」。少数结构上必然更重的图标用 `optical_scale` 微调（实心油漆桶 0.95、字号加减号 0.78）。
  5. **单一图标库，全程序复用**：覆盖层工具栏、长截图状态条共用同一套 `Icon`；`overlay/shape.rs` 承接原先塞在 `toolbar/icons.rs` 里的绘制小工具（`paint_of`/`rounded_rect`/`rounded_path`），设置面板与右键菜单不再跨层引用 `toolbar::icons`。
  6. **状态语义统一**：`normal = icon` / `disabled = icon_dim` / `active = accent + accent_soft 底`；开关类（填充、圆角、B、I）额外用 `accent` 图标色表示「已开启」。高亮工具的荧光黄是唯一的颜色白名单例外。
  7. **视觉重量由测试守门**：`render::icon` 的 4 项单测固定「每个图标能解析」「不顶到按钮边缘」「26px 下墨水覆盖率 5%–45%」「48px 下同样居中」。新增图标跑偏会直接测试失败，而不是等肉眼发现。
- **Consequences**:
  - 更容易：改图标 = 换一个 SVG 文件，不用碰 Rust；新增图标只需加文件 + 一行表项；撤销/对勾/叉等图标不再重复实现；工具栏绘制代码从 219+184 行降到 154 行，整个改动净减 577 行 Rust。
  - 更难／代价：多了一个自维护的 SVG 子集解析器（约 180 行），遇到不支持的特性会静默丢弃该 path —— 所以规格必须写死并在 README 里说明；图标是编译期嵌入，改图标需要重新编译。
  - 素材风格目前是混合的：`rect`（虚线框）、`blur`（散点）来自外部图标集，笔画在 26px 下低于 1.5px，边缘会发灰（墨水 11.1% / 12.8%，是全组最轻的两个）。已在 `assets/icons/ui/README.md` 标注；若后续反馈看不清，换成自绘的实心版本即可。
  - 遗留：外部图标集（`arrow/highlight/pin/copy/save/rect/text/blur/glow/longshot/check/cross`）的**许可证与出处未登记**，发布前必须确认与 GPL-3.0-only 兼容并补进 `THIRD-PARTY.md`。
  - 后续（用户提出，未实现）：圆角改为在对象上提供内部控制柄（矩形/高斯模糊/高亮/聚光灯通用），届时可移除工具栏的「圆角」开关按钮。
