# 长截图：已知问题与抓屏排查记录

记录「长截图拼不出来」的排查过程与结论，避免下一轮重复实验。
排查用的临时代码已全部删除（重建方法见末节），帧对分析工具留在 `examples/longshot-shift.rs`。

## 结论（先看这段）

**不支持半透明窗口**（背景不透明度 < 100%）。典型是 Windows Terminal 的默认配置
（外观 → 背景不透明度 70%，默认就开着）；「启用亚克力材料」**不是**原因，开着它但把
不透明度调到 100% 也不彩响（实测见下）。表现为状态条正常、滚轮也在滚，但预览缩略图不变长、
结果只有首屏。

规避办法：把目标窗口的**背景不透明度调到 100%**，再长截图。README / README.en.md 的
「长截图」段落已写明这条限制；拼接失败时状态条也会直接提示。

---

## 现象

1. **目标窗口是 Windows Terminal 时拼不出来**（v0.11.0 首次复现，与 #152 的贴图改动无关）。
   状态条正常出现、滚轮滚下去终端内容可见地上移，拼接却毫无进展。
   - 记事本、资源管理器、Chrome 都正常。
2. **视口内有动态内容**（动画徽章、光标闪烁、进度条）时拼接也会失败。用户观察，未系统复现。

> 注意：v0.14.0 起状态条**不再显示「已拼 N px」**（只留操作提示），判断拼接是否推进
> 请看视口旁边的**预览缩略图**有没有变长。

## 第一层根因：GDI 抓不到半透明窗口的内容（2026-11-23 定位）

三组 A/B 实测（同一个终端窗口、同一页面、Windows Terminal 默认配色，「外观」页）：

| 不透明度 | 亚克力材料 | 结果 |
|---|---|---|
| 100% | 关 | **能拼** |
| 100% | **开** | **能拼** ← 亚克力不是原因 |
| 70%（默认） | 开（默认） | **抓不到内容** |

结论只有一个变量：**不透明度是否 100%**。

机制：半透明窗口的内容由 DWM 在合成器里生成（文本层 + 背景 alpha 混合）。
`BitBlt` 读的是 GDI 重定向表面，上面没有这层内容 —— 抓到的只有背景在刷新。

这也解释了为什么 v0.11 的几种后端「一起失败」：`BitBlt`、`BitBlt + CAPTUREBLT`、
`PrintWindow(RENDERFULLCONTENT)` 读的都是同一套 GDI 表面，一起失败是应该的。

## 第二层根因：换 DDA 能抓到帧，但帧无法对齐（2026-11-24 实测）

DXGI 桌面复制（DDA）抓的是 DWM 合成后的最终图像，**确实能拿到终端内容**。
在一台开着亚克力的终端上跑（`AIRSNIP_LONGSHOT_CAPTURE=dda`，实现见末节重建方法）：

- `AIRSNIP_DEBUG` 日志里每拍 `capture#N hash=` **都不相同** → 抓到新帧了；
- 但每拍都是 `push: offset=0 … confident=false` + `failed: locate mismatch`；
- 异常帧落盘的 PNG 拿去跑 `examples/longshot-shift.rs`（签名口径与 `stitch.rs` 一致），结果：

```
prev 与新帧都是 1024×782
s=0（同位置）匹配率 0.477
所有 s≥1          匹配率 0.000        ← 任何位移都配不上
不匹配行的每像素平均差 p50=42.3 / p90=60.0（0-255）   ← 内容级差异，不是抖动
完全一致的行只在「同位置」出现，没有任何「prev y=400 → new y=100」的平移关系
```

即：相邻两拍之间，画面不是「平移了一段」，而是**一部分行没变、另一部分行整个变了**。
这种帧对，拼接算法怎么写都拼不出来 —— 问题在抓帧，不在 `stitch`。

机理上就是 alpha 混合：文字像素与背景按不透明度混合，每拍的合成结果都不一样，
像素级哈希（`stitch` 的行签名）永远对不上。终端的平滑滚动动画会让情况更糟，但不是必要条件
（A 组不透明时同样有平滑滚动，却能拼）。

### 帧对分析工具（保留）

`examples/longshot-shift.rs`：读两张落盘帧，算「每个候选位移的匹配率」与「不匹配行的像素差」。
签名口径与 `src/longshot/stitch.rs` 一致，所以它算出的数字能直接对照日志。

```powershell
cargo run --example longshot-shift -- "$env:TEMP\airsnip\airsnip-longshot-failed-1-prev.png" `
    "$env:TEMP\airsnip\airsnip-longshot-failed-1-new.png"
```

用途：将来有人报「拼接不动」，先看 `%TEMP%\airsnip\` 里有没有 `failed-*-prev/new.png`
（异常帧落盘是常开的，上限 6 对），跑一遍就能区分「算法没选对位移」与「帧本身对不上」。

## 已试过的抓屏后端

| 后端 | 不透明窗口 | 半透明窗口 |
|---|---|---|
| GDI `BitBlt`（默认） | ✓ | ✗ 帧不变 |
| `BitBlt` + `CAPTUREBLT` | ✓ | ✗ |
| `PrintWindow(PW_RENDERFULLCONTENT)` + 裁剪 | ✓ | ✗ 帧不变 |
| DXGI 桌面复制（DDA） | ✓ | △ 帧会变，但帧对无法对齐 |

**已排除**：不是我们自己的置顶窗口（边框 / 状态条 / 预览）遮挡导致。用隔离探针
（不起 AirSnip、直接抓前台窗口两帧比对）测过，差异像素为 0，结论一致。

## 若要继续做

- **匹配算法改为容差式**（行内像素差 < 阈值，而不是精确哈希）：可能让半透明帧也能对齐，
  但阈值要吃掉背景噪声又不能误匹配，且会影响所有场景；不可靠就不值得做。
- **`Windows.Graphics.Capture`**（WinRT）：需要一次授权 UI、D3D 设备，与「纯 CPU 渲染 /
  待机零开销」的约束冲突，属于 ADR 级别的改动。
- **等画面稳定再拼**（连续两拍完全一致才处理）：能治平滑滚动动画，治不了半透明混合。

## 需要时怎么重造探针

- **强制 SDR 屏也走 DDA**：`capture/hdr/output.rs` 的 `hdr_output_intersecting` 加一个
  `require_hdr: bool` 参数（`false` 时不跳过 SDR 输出），`capture/hdr/session.rs` 加
  `HdrRegionSession::open_any` 调用它，`capture/region.rs` 的 `RegionCapture::new` 里按
  环境变量 `AIRSNIP_LONGSHOT_CAPTURE=dda` 选 `open_any` / `open`。
  （本次实验就是这么做的，验证完已回退，git 历史里可查。）
- `capture/mod.rs`：`capture_region_bitblt_opts(x, y, w, h, capture_layered)`、
  `capture_region_printwindow(x, y, w, h)`（`PrintWindow` 在 `windows` crate 里挂在
  `Win32_Storage_Xps` 特性下，探针用 `#[link(name = "user32")]` 直接声明，避免为一个函数加特性）。
- `#[ignore]` 测试 `capture::tests::manual_grab_probe`：抓前台窗口两帧、打印差异像素数并存 PNG
  （`cargo test manual_grab_probe -- --ignored --nocapture`）。
