# 界面图标（UI icons）

覆盖层工具栏、长截图状态条使用的单色矢量图标。全部为**实心填充剪影（filled silhouette）**，
运行时由 `src/render/icon/` 解析成 `tiny_skia::Path` 绘制，颜色由主题调色板注入。

## 目录

```
ui/
  toolbar/   # 覆盖层工具栏 18 个按钮
  status/    # 长截图状态条
```

文件名即 `Icon` 枚举变体的小写形式（`Icon::Longshot` → `toolbar/longshot.svg`），
`src/render/icon/data.rs` 用 `include_str!` 一一对应，缺文件会在编译期报错。

## 文件规格（新增或替换图标必须满足）

| 项 | 要求 |
|---|---|
| viewBox | `0 0 1024 1024` |
| 颜色 | 只用 `fill="currentColor"`，不写死色值 |
| 元素 | 只允许 `<path>`（含 `fill-rule="evenodd"`） |
| 禁止 | `<g>`、`transform`、`<mask>`、`<clipPath>`、渐变、`<style>`/class、`width`/`height` |
| 网格 | 内容居中，bbox 建议 700–900 单位见方（渲染端按 bbox 归一化，但不统一会让视觉重量漂移） |
| 笔画 | 框类笔画宽 **85**；线宽低于 **60** 会在 26px 下糊掉 |
| 挖洞 | 用 `fill-rule="evenodd"`，或同 path 内反向 winding（顺时针外轮廓 + 逆时针内洞） |

### 为什么是这些数字

工具栏按钮 26px（物理像素），图标活动区约 20px。1 个 CSS 像素 ≈ 40 个网格单位，
因此 **60 单位 ≈ 1.5px** 是能稳定分辨的下限，85 单位 ≈ 2.1px 是框类笔画的基准。

## 来源

| 文件 | 来源 |
|---|---|
| `toolbar/{fill,round,undo,redo,bold,italic,minus,plus}.svg` | 本项目自绘（原有手绘几何图标没有对应素材，按本文件规格生成） |
| `toolbar/fill.svg` | 本项目自绘（油漆桶）：桶身梯形 + 桶沿 + 提手半圆环 + 漆滴 |
| `toolbar/arrow.svg` | 本项目自绘（用户提供版本的笔画过粗，按反馈调细至墨水 28%） |
| `toolbar/{rect,text,blur,glow,longshot,highlight,pin,copy,save}.svg` | 用户提供的图标集，仅清洗（去 `p-id`/`class`/DOCTYPE/写死色值；`copy` 删除一个孤立杂点 path） |
| `status/{check,cross}.svg` | 同上 |

用户提供的图标集**来源与授权待补**。入库前必须确认其许可证与 GPL-3.0-only 兼容，
并在 `THIRD-PARTY.md` 登记。

## 视觉重量（实测）

78px 归一化渲染、6% 内边距下的墨水覆盖率。`cargo test` 中的图标测试会校验该指标落在
合理区间，防止后续新增图标视觉重量跑偏。

| 图标 | 墨水% | 图标 | 墨水% |
|---|---|---|---|
| rect | 11.1 ⚠️ | copy | 26.8 |
| arrow | 28.3 | save | 29.2 |
| text | 19.0 | bold | 31.1 |
| highlight | 32.1 | italic | 25.3 |
| blur | 12.8 ⚠️ | minus | 16.6 |
| glow | 27.1 | plus | 27.6 |
| fill | 40.5 | check | 14.9 |
| round | 26.0 | cross | 24.4 |
| undo / redo | 19.2 | pin | 23.1 |
| longshot | 28.4 | | |

⚠️ `rect`（虚线框 + 加号）与 `blur`（散点）在 26px 下笔画低于 1.5px，边缘会发灰。
保留原样是因为它们是用户指定的素材；若后续反馈"看不清"，可换成本项目自绘的实心方框 /
圆 + 粗弧环版本（见 git 历史 `feat/icon-unify` 分支早期提交）。

`fill`（油漆桶）是本项目自绘的实心剪影：桶身梯形 + 桶沿 + 提手半圆环 + 漆滴。
因桶身全实心，墨水 40.5% 是整组最高，在 26px 下最抢眼。若想降到与邻居齐平
（≈29%），把桶身改为粗边框梯形（`fill-rule="evenodd"`，边框 85）即可，素材结构不变。
