# ADR 0025：剪贴板图片的 CF_DIB 采用自下而上存储

- 状态：已采纳（v0.8.1 之后）
- 影响范围：`src/output/mod.rs` 的 `copy_to_clipboard`（CF_DIB 写入路径）
- 相关：issue #89、OUT-1、ADR 0001（纯 CPU 渲染）

## 背景

issue #89：截图后复制到剪贴板，**无法粘贴进飞书聊天框**，而微信客户端正常。

AirSnip 原写入的 `CF_DIB` 是「自上而下」编码：

```
28 00 00 00 | 宽 | 高 = 负值（自上而下） | 1 | 32bpp | BI_RGB | biSizeImage = 0
```

而 GDI（`.NET Clipboard.SetImage`）、Windows 截图工具与主流截图工具统一写
**正 `biHeight`（自下而上）**，并填好 `biSizeImage`。

### 根因

飞书桌面端（本机 8.4.7）是基于 Chromium 147 的深度定制客户端。其消息编辑器通过自有的原生桥
读剪贴板图片（`messenger.asar` 中可见 `[Editor] clipboard.readImage` 日志前缀与
`AM("image/*","base64")` 调用），**不走 Chromium 标准的 `ReadPng`**——后者会优先读取我们同时写入的
`PNG` 注册格式，从而掩盖问题。也就是说飞书这边**直接解析 `CF_DIB`**。

原生的 DIB 解析器普遍按「正高度」推算行序与像素数据量；遇到负 `biHeight` 时按
`biWidth * biHeight * bpp / 8` 会算出负数/空，于是判定为无有效图像而拒绝粘贴。
微信的解析器能兼容负高度，所以出现「微信行、飞书不行」的差异。

## 决策

**`copy_to_clipboard` 写出的 `CF_DIB` 改为自下而上（`biHeight` 为正），写入时逐行反转像素；
并补齐 `biSizeImage = width * height * 4`。**

`CF_DIB` 的 `BITMAPINFOHEADER` 约定：`biHeight` 为正表示自下而上（首行为图像底行），
为负表示自上而下。改动后与 GDI / 主流截图工具一致，最大化兼容性。
`PNG` 注册格式与 `CF_BITMAP`（由 Windows 从 DIB 合成）不变，本次不新增格式。

### 验证

- 修复后用户实测：**飞书可粘贴，微信仍正常**。
- 行序无翻转：用 2×2 四色图案（左上红/右上绿/左下蓝/右下黄）复制后经
  `System.Windows.Forms.Clipboard.GetImage()` 读回，四角颜色正确。
- `cargo test` 280 项全绿；AirSnip 自身贴图读图（`src/pin/clipboard.rs`，PNG→CF_DIB→CF_BITMAP）
  对正负高度都支持，无回归。

## 未采纳方案

- **继续用自上而下**：合法但非主流，是本次不兼容的根因。
- **额外显式写 `CF_DIBV5` / `CF_BITMAP`**：Windows 已能自动从 `CF_DIB` 合成这两个格式
  （实测剪贴板中同时出现），重复提供无收益，反而增加维护面。
- **只补 `biSizeImage`、不改方向**：无法覆盖「按高度推算行序」的解析器。

## 影响

- 仅复制方向与一个头部字段变化，无性能与依赖影响（`cargo deny` 配置不变）。
- 后续任何新的剪贴板图片写出路径都应遵循「自下而上 + 正确 `biSizeImage`」，避免再次踩坑。
