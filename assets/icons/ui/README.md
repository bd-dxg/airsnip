# 界面图标（UI icons）

覆盖层工具栏、长截图状态条使用的单色矢量图标。统一取自 **Material Symbols（outlined）**，
运行时由 `src/render/icon/` 解析成 `tiny_skia::Path` 绘制，颜色由主题调色板注入。

## 目录

```
ui/
  toolbar/   # 覆盖层工具栏 17 个按钮（圆角开关已移除，改为对象上的圆角控制柄）
  status/    # 长截图状态条
```

文件名即 `Icon` 枚举变体的小写形式（`Icon::Longshot` → `toolbar/longshot.svg`），
`src/render/icon/data.rs` 用 `include_str!` 一一对应，缺文件会在编译期报错。

## 文件规格（新增或替换图标必须满足）

| 项 | 要求 |
|---|---|
| viewBox | 保留素材原值（Material Symbols 为 `0 -960 960 960`，部分为 `0 0 24 24`）；渲染按 bbox 归一化，viewBox 只需自洽 |
| 颜色 | 只用 `fill="currentColor"`，不写死色值 |
| 元素 | 只允许 `<path>`（含 `fill-rule="evenodd"`） |
| 禁止 | `<g>`、`transform`、`<mask>`、`<clipPath>`、渐变、`<style>`/class、`width`/`height` |
| 居中 | 内容居中，bbox 尽量接近正方形（不统一会让视觉重量漂移） |
| 线宽 | 由素材决定；验收看墨水率，见下方「视觉重量」 |

### 为什么不再限制网格与笔画数值

v0.6.0 起整套图标统一取自 Material Symbols，它们的网格与线宽由 Google 设计规范给定，
不再需要项目自订的 1024 网格 / 85 笔画规则。渲染端只做一件事：按 bbox 归一化到
按钮的 76% 活动区，因此**真正需要守住的是墨水率区间**（见下），而不是具体坐标。

## 来源

全部取自 **Material Symbols（outlined）**，Apache-2.0，已在 `THIRD-PARTY.md` 登记。
下载地址：`https://raw.githubusercontent.com/google/material-design-icons/master/symbols/web/{名称}/materialsymbolsoutlined/{名称}_24px.svg`
清洗：仅去 `width`/`height`、给 `<path>` 补 `fill="currentColor"`，图形未改。

**唯一例外**：`highlight` 用 `format_ink_highlighter_fill1_24px.svg`（同目录的填充变体）。
它的 outlined 版笔身是细线框，在 26px / 浅色主题下几乎消失（实测 20px 下只剩底线），
填充变体才是可读的。其他图标保持 outlined。

| 文件 | Material Symbols |
|---|---|
| `toolbar/rect.svg` | `crop_free` |
| `toolbar/arrow.svg` | `arrow_outward` |
| `toolbar/text.svg` | `text_fields` |
| `toolbar/highlight.svg` | `format_ink_highlighter`（**fill1** 变体，见下） |
| `toolbar/blur.svg` | `blur_on` |
| `toolbar/glow.svg` | `flashlight_on` |
| `toolbar/fill.svg` | `format_color_fill` |
| `toolbar/undo.svg` / `redo.svg` | `undo` / `redo` |
| `toolbar/pin.svg` | `push_pin` |
| `toolbar/longshot.svg` | `unfold_more` |
| `toolbar/copy.svg` | `content_copy` |
| `toolbar/save.svg` | `save` |
| `toolbar/bold.svg` / `italic.svg` | `format_bold` / `format_italic` |
| `toolbar/minus.svg` / `plus.svg` | `remove` / `add` |
| `status/check.svg` / `cross.svg` | `check` / `close` |

历史：v0.6.0 之前这批图标来自 iconfont 上多个不同作者，**来源与授权无法确认**，已全部
替换为 Material Symbols，避免与 GPL-3.0-only 冲突。新增图标请从同一套取，不要引入授权
不明的素材（类似 iconfont / 图标聚合站的「免费商用」宣传语不构成许可证）。

## 视觉重量（实测）

78px 归一化渲染、6% 内边距下的墨水覆盖率。`cargo test` 中的图标测试会校验该指标落在
合理区间（5%～48%），防止后续新增图标视觉重量跑偏。

| 图标 | 墨水% | 图标 | 墨水% |
|---|---|---|---|
| rect | 14.5 | copy | 25.5 |
| arrow | 27.5 | save | 31.7 |
| text | 20.0 | bold | 34.0 |
| highlight | 27.0 | italic | 22.8 |
| blur | 15.4 | minus | 9.9 |
| glow | 20.9 | plus | 18.1 |
| fill | 21.8 | check | 10.8 |
| cross | 21.9 | pin | 14.7 |
| undo / redo | 20.6 | longshot | 9.2 |

全组落在 9.2%～34.0%，均在测试区间（5%～48%）内。个别图标用
`render::icon::optical_scale` 微调（`save` 0.92：软盘 bbox 偏大，不缩会在 26px 下盖过邻居）。

**验收要同时看深色与浅色主题**：线框图标在浅底上会显得更淡（实测 `highlight` 的
outlined 版就是因此换成 fill1）。新增/替换图标后请在两种底色下各看一眼 26px 渲染。

另外，`highlight` 是唯一**颜色跟主题走**的图标：工具栏按钮里它的图标色取
`Palette::highlight_ink`（深色主题荧光黄 `#FFE600`，浅色主题暗金 `#A67C00`）。
荧光黄在浅色工具栏背景上几乎看不见，不能写死。

调整图标后要重跑 `cargo test render::icon` 并更新此表（数值可直接从测试输出抄）。
