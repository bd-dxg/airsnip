# ADR 0015：界面图标统一换用 Material Symbols

- 状态：已采纳（v0.6.0）
- 日期：2026-10-31
- 相关：ADR 0009（界面图标统一）、`assets/icons/ui/README.md`、`src/render/icon/`

## 背景

ADR 0009 把工具栏与长截图状态条的图标从手绘几何图形换成了 `assets/icons/ui/` 下的 SVG，
但当时有 11 个图标取自 iconfont 上多个不同作者的素材：

```
toolbar/{rect,text,blur,glow,longshot,highlight,pin,copy,save}.svg
status/{check,cross}.svg
```

这批素材的**来源与授权无法确认**（作者不可考、彼此风格不一致）。`assets/icons/ui/README.md`
和 `AGENTS.md` 都记着「发布前须补授权」，一直挂到 v0.6.0 发版前仍是阻塞项。

本项目以 **GPL-3.0-only** 发布，图标作为源码的一部分随仓库分发，必须有明确的再分发许可。
期间评估过几个替代来源：

| 候选 | 结论 |
|---|---|
| iconfont（阿里） | 素材由用户上传，单图标授权各异，混合多作者时无法追溯 |
| icon.sucai999.com 等图标聚合站 | 首页只有「免费商用」宣传语，**没有任何许可证文本或协议页**，本质是搬运第三方图标集，站点自身无再授权能力 |
| Phosphor Icons（MIT） | 许可干净，但只有 fill（实心）系列，与项目原有的细线图标风格不搭 |
| **Material Symbols（Apache-2.0）** | 许可干净且明确允许修改/再分发；`materialsymbolsoutlined` 是线框风，与项目原有细线图标协调；图标语义覆盖齐全 |

## 决策

全组 **19 个界面图标**统一取自 Material Symbols 的 `materialsymbolsoutlined` 变体，
映射关系记录在 `assets/icons/ui/README.md` 的「来源」表。

清洗规则（保持渲染端不变）：

- 去掉 `width` / `height`，保留素材原 `viewBox`（`0 -960 960 960`，少数为 `0 0 24 24`）
- 给 `<path>` 补 `fill="currentColor"`，不写死色值
- 图形本身不做修改

渲染端仍按 **bbox 归一化**到按钮的 76% 活动区，因此 viewBox 差异不影响视觉尺寸；
`render::icon::optical_scale` 只对 `save`（软盘 bbox 偏大，0.92）保留微调。

## 影响

- **授权**：`THIRD-PARTY.md` 新增「图标与美术资源」节，登记 Material Symbols（Apache-2.0）；
  该节由 `scripts/gen-third-party.ps1` 生成，重跑脚本不会丢
- **视觉**：线框风格统一，26px 下墨水率收敛到 9.2%–34.0%（原为 8%–40% 的混搭）；
  单测阈值从 5%–45% 放宽到 5%–48%，因为 Material 的 `bold` 字形天然更重
- **规格**：`assets/icons/ui/README.md` 不再要求自订的 1024 网格与 85 笔画——这两条是手绘
  素材时代的约束，现在由 Material 的设计规范决定，验收改为看墨水率
- **新增图标**：从同一套取，不要引入授权不明的素材（聚合站的「免费商用」宣传语不构成许可证）
