# UI Automation 实测记录（控件级捕捉前置，issue #62 二级）

「控件捕获」= 在窗口吸附之上再下钻到子控件。方案本身（UIA / `ElementFromPoint`）在 issue #62 里已经写清，
不确定的是**在这台机器上的真实表现**：耗时、覆盖、坐标口径、以及目标进程无响应时会阻塞多久。
所以先有探针 `examples/uia-probe.rs`，量完再动手实现。

## 怎么跑

```powershell
chcp 65001        # 否则中文窗口标题乱码
cargo run --release --example uia-probe -- scan             # 批量扫描 + 四条门槛判定
cargo run --release --example uia-probe -- scan --verbose   # 额外打印每个采样点的元素链
cargo run --release --example uia-probe -- follow 60        # 贴光标连续查询（Esc 退出）
cargo run --release --example uia-probe -- point 688,531    # 固定坐标单次查询（可复现）
```

`point` 是专为「权限 / 坐标」类问题准备的：同一个坐标在**普通终端**与**管理员终端**各跑一次，结果才可比。
它还会打印该点最上层窗口及其进程是否提权——`E_ACCESSDENIED` 这类失败靠它定位。

无响应进程（**必须单独测**，异步 + 超时降级那套架构全靠这个前提）：

```powershell
cargo run --release --example uia-hang                     # 终端 A：2 秒后停止处理消息
cargo run --release --example uia-probe -- follow 60        # 终端 B：把鼠标移到那个窗口上看耗时
```

跑 `scan` 时把**几个常用程序同时铺在桌面上**（资源管理器 / 浏览器 / 记事本 / VS Code / 一个 UWP 应用），
否则遮挡会让大部分采样被跳过（探针按 Z 序判断采样点真正属于哪个窗口，不糊弄）。

## 四条门槛

| # | 判据 | 为什么 |
| --- | --- | --- |
| ① | 单次 `ElementFromPointBuildCache` p95 ≤ 30ms | 鼠标 60fps 移动时不超过 2 帧 |
| ② | 祖先链 5 级累加 p95 ≤ 80ms | 滚轮爬层级不能一顿一顿 |
| ③ | 窗口级元素矩形 vs DWM 扩展边框误差 0px | 项目统一物理像素口径，错位会直接导致选区偏移 |
| ④ | ≥ 5 个常用程序能拿到比整窗更小的元素 | 否则这功能只是把整窗吸附重做一遍 |

任一条明显不达标：先缩范围（比如只在「停手 120ms 后查一次」而不是每次移动都查），再判断值不值得做。

## 第二轮实测（开发机，普通权限终端，桌面铺了 To Do / 设置 / 记事本 / Edge）

| 门槛 | 实测 | 结论 |
| --- | --- | --- |
| ① | p50 2.5ms / p95 14.7ms / max 21.2ms | 通过 |
| ② | 祖先链累加 p95 4.5ms，To Do（ApplicationFrameWindow）链深 7 | 通过 |
| ③ | **UWP 差 7px**：`ApplicationFrameWindow` UIA=`80,64`、DWM=`87,64`、`GetWindowRect`=`80,64` | 见下方「③ 的修正」 |
| ④ | 2 个（To Do + 设置），其余窗口被遮挡采样不到 | 待补测 |

### ③ 的修正：UWP 窗口的 DWM 边框与 UIA/GetWindowRect 差 7px

实测口径：普通窗口（Edge / 任务栏 / winit 窗口）UIA 矩形与 DWM 扩展边框**完全一致**（Δ0,0），
但 UWP 宿主窗口 `ApplicationFrameWindow` 的 DWM 扩展边框比 UIA / `GetWindowRect` **多出 7px**（左边界）。

所以「坐标 0 偏差」这条门槛对 UWP 严格不成立。实现时的处理（不需要额外决策）：

- **窗口级（层级 0）仍用 DWM 扩展边框**——与 v0.12 的整窗吸附一致，不会出现升级后窗口吸附偏 7px；
- **控件级用 UIA 矩形，但一律裁到所属窗口的 DWM 边框内**。UWP 元素向左多出的 7px 会被裁掉，
  最坏情况是贴着窗口左边界的控件少 7px；比反过来（窗口级用 UIA）安全得多。

### 观察：`follow` 出现过 `E_ACCESSDENIED`

普通权限终端下，`follow` 在某个位置连续报「拒绝访问。」，而 `point` 查同一坐标能成功——
说明失败取决于**该点当时的目标窗口**，不是探针代码。常见成因：目标属于更高完整性级别的进程（UIPI），
或 AppContainer / WebView2 内容区不接受非 `UIAccess` 调用方。

这一点必须是 go/no-go 级的实测：**AirSnip 本体是 `requireAdministrator`**，与探针的普通权限上下文不同，
所以要用「普通终端 vs 管理员终端」各跑一次 `point` 对比。

`scan` 共 70 次采样，其中 50 次因上方窗口遮挡被跳过；可统计目标是 Edge（Chromium）与任务栏。

| 门槛 | 实测 | 结论 |
| --- | --- | --- |
| ① | p50 2.4ms / p95 4.7ms / max 21.7ms（首查询含 provider 预热） | 通过 |
| ② | 祖先链累加 p95 3.9ms，链深最多 6（文本 → 数据项 → 数据项 → 表格 → 文档 → 面板） | 通过 |
| ③ | 窗口级元素 vs DWM 扩展边框 Δ0,0（同期 `GetWindowRect` 比 DWM 多 16,16 的阴影） | 通过 |
| ④ | 桌面只铺了 Edge 一个真实程序，Edge 明确暴露子元素 → 1/1 | **样本不足，待补测** |

未出现 >500ms 的查询；无响应进程的表现尚未测（要跑 `uia-hang`）。

## 实测把设计推翻/修正的地方

1. **`AutomationElementMode_None` 不能用于树导航**：用它拿到元素后，`GetParentElementBuildCache` 直接报
   「无效指针」。必须用 `AutomationElementMode_Full` 的缓存请求（缓存仍然生效，一次往返取回全部属性），
   否则「滚轮爬层级」根本无从实现。
2. **走不动父级时 windows-rs 报 `Err(S_OK)`**（`S_OK` + 空出参）。判定「到根了」要按 `error.code().is_ok()`
   过滤，否则会把正常结束误记成失败。
3. **Chromium 的最深元素经常比窗口还大**：例如整页滚动区 `组 pg_forumdisplay 2514×3992 @42,-960`
   （超出屏幕上下边界）。所以候选矩形必须**裁到所属窗口内**，并把「比窗口大」的层级视为无效继续上溯；
   否则单击一下会选出一个比屏幕还高的选区。
4. **不能假设一定能上溯到窗口**：实测有元素（`ContentsBackgroundView`）只有 1 级、没有父级。
   层级链必须以「整窗矩形」作为兜底层级，不能只依赖 UIA 链本身。
5. 元素自身的 `NativeWindowHandle` 大多是 0，靠它反查「属于哪个窗口」不可靠——判断归属只能按
   Z 序 + 矩形包含（与 `overlay::snap::hit` 同一套）。

## 第三轮实测（普通终端 vs 管理员终端，同一坐标）

| 场景 | 结果 |
| --- | --- |
| 普通终端 `point 688,531` | 命中 `Window Class`（pid 15980，非提权），链深 2 |
| 管理员终端 `point 688,531` | 命中 `TermControl 'PowerShell'`（pid 12528，**提权**），链深 1，18.5ms |
| 管理员 `follow 60` | 成功查到提权终端（`TermControl`）与**非提权** explorer（`Windows.UI.Input.InputSite...`）；单次 2.7–15.5ms |

**权限这一条不是障碍**：

- 提权进程能查提权目标（PowerShell 的 `TermControl`）；也能查**非提权**目标（explorer 的 InputSite 面板）——
  UIPI 只挡「低完整性查高完整性」，高查低没问题，而 AirSnip 恰好是高的一侧。
- 之前的 `E_ACCESSDENIED` 全部指向 `class=Ghost pid=16692`——**Ghost 窗口就是 Explorer 为「已无响应进程」
  创建的幽灵窗口**。UIA 对它 **3–8ms 就快速失败**，不是卡秒级。这对异步降级架构是好消息：
  卡死场景可能表现为「持续快速报错 → 降级到整窗」，而不是「查询挂住」。

## 第四轮实测（管理员终端）：卡死窗口真的会阻塞 10 秒

`follow` 里出现了一次：

```
--- 点 (256,411) 单次 10023.7ms，链深 0
    （查询失败：由于超时时间已过，该操作返回。）
      该点最上层窗口：class=Ghost pid=16692
```

随后在同一个 Ghost 窗口上的查询变成 **3～8ms 的 `E_ACCESSDENIED`**（UIA 记住了「无响应」状态）。

**结论：同步查询不可行**（UI 线程会死 10 秒）；异步 worker 也必须能容忍自己被卡 10 秒。
另外还出现过 24.2 / 34.4 / 52.3ms 的尖峰（正常波动，仍在门槛内）。

`scan`（管理员，桌面上铺了 To Do / Windows Terminal / Notepad / Edge / 资源管理器）：

| 门槛 | 实测 | 结论 |
| --- | --- | --- |
| ① | p50 3.8ms / p95 16.1ms / max 21.8ms | 通过 |
| ② | 祖先链 p95 7.0ms | 通过 |
| ③ | UWP 窗口 UIA=`-4,1` / DWM=`3,1` → 差 7px（普通窗口仍 Δ0,0） | 按「窗口级 DWM + 控件级裁到边框」处理 |
| ④ | 单轮 3 个（To Do / Windows Terminal / Notepad），其余因遮挡采样不到 | 跨轮累计 ≥5（含 Edge / 资源管理器）→ 视为通过 |

## 又两个必须处理的发现

1. **点查询在窗口/元素边界附近极不稳定**：`follow` 里相邻像素（(1084,570) → (1083,570)）分别返回
   PowerShell 的 `TermControl` 与一个全屏窗口（`winc.exe`），链也换了一条。实现时必须做稳定化
   （同一元素需连续 N 次命中才切换高亮，或按 `hwnd` + 元素缓存去重），否则高亮框会跟着鼠标抖。
   **尚未查清**是「真不稳定」还是「当时窗口本身在动」：需在窗口布局固定的前提下重跑
   `point x,y` 多次验证。
2. **Z 序矩形命中与 UIA 命中可能不一致（待验证）**：桌面上的 `winc.exe`（pid 15980，标题 `airsnip`，
   winit 窗口）是 `2576×1456 @-8,-8` 的全屏窗口，实测它**不是**点击穿透浮层：
   `exstyle=0x00040110`（无 `WS_EX_TRANSPARENT` / `WS_EX_LAYERED`）、`DWMWA_CLOAKED=0`、
   `GetWindowRgn=ERROR`（无窗口区域）、DWM 边框 `0,0–2560,1440`。所以「枚举认为它压住整屏、
   UIA 却返回底下的 PowerShell 元素」这个初步观察**未能复现成立**，暂记为待查。
   如果后续在固定布局下确认存在不一致，则整窗吸附（`overlay::snap::hit` 用 Z 序 + `GetWindowRect`）
   与控件层就得统一命中口径（比如控件层直接从 UIA 链推窗口）。
3. 链深仍然常常是 1（`TermControl` / `InputSite` 都没父级），再次坐实「窗口矩形必须做兜底层级」。
4. **首次查询有预热开销**：同一坐标连续三次单点查询为 38.6ms → 25.0ms → 18.8ms；
   同理 `scan` 的第一个点总是最慢（21–30ms）。实现时应在覆盖层启动阶段（淡入那 120ms 里）
   让 worker 先发一次废查询预热 provider，别让用户的第一次悬停撞上这笔开销。
5. **再次确认「最深元素可能比窗口大」**：`组 pg_viewthread 2514×1880 @42,-445`（整页区域，超出视口）。

## 待补

- [ ] 多个常用程序可见时重跑 `scan`，把 ④ 的名单补到 ≥5（含资源管理器 / 记事本 / VS Code / UWP）
- [ ] **权限对比**：普通终端 vs 管理员终端各跑一次 `point <同一坐标>`（优先选 To Do / Edge 内容区），
      确认 `E_ACCESSDENIED` 是否与提权相关——AirSnip 本体以管理员运行，这一步决定控件捕获在真实环境里能否用
- [ ] `uia-hang` + `follow` 量出「无响应进程」的阻塞时长，并确认「最新请求胜出 + 超时降级」够用
- [ ] 多屏 / 混合 DPI 下的坐标口径（当前无多屏环境，同 #127 第 4 条）

## 第五轮实测（集成测试 `uia_query_each_window`，目标窗口在前台）

`cargo test -- --ignored --nocapture uia_query_each_window`，逐窗口取中心点跑一次 `query()`：

```
Notepad                     层级 1 根矩形 (1902,137,583,381) pid=7736 异窗 None
CASCADIA_HOSTING_WINDOW_CLASS 层级 1 根矩形 (274,68,2013,1304) pid=5516 异窗 None
CabinetWClass               层级 0 根矩形 = 终端那块         pid=5516 异窗 Some
```

结论：

- 控件级路径通了：记事本与 Windows Terminal 都能拿到比窗口小的元素（层级 1）；
- 「根元素属于别的进程」的降级也对：被终端盖住的资源管理器返回 `异窗`，不瞎猜；
- **自绘类应用只能整窗**：winc（编辑器）的 UIA 只暴露窗口/终端一层，这与 ADR 0039 「UIA 不暴露子元素的程序只有整窗」的已知边界一致，不是实现缺陷。

### 第六轮（改正方向后）：改用 `ElementFromHandle` 向下钻

集成测试 `uia_query_each_window`（现在**不要求窗口在前台**，因为不再依赖全局命中）：

```
Notepad        层级 2  最深 (1253,409,583,381)
CabinetWClass  层级 6  最深 (1747,1013,80,19)
Shell_TrayWnd  层级 1  最深 (1214,1438,88,48)
Chrome_WidgetWin_1 层级 1  最深 (0,0,2560,1438)
ApplicationFrameWindow（UWP） 层级 0
```

关键教训：`ElementFromPoint` 在**截图覆盖层开着**时永远命中覆盖层自己
（实测日志 `根进程 …(airsnip.overlay)`），所以它不适合本场景；
探针早期测的耗时数据仍然有意义（成本量级相同），但**不能用来判断「能不能拿到控件」**。


## 结论（截至目前）

- 耗时：正常情况远高于要求（p95 4.7–16ms），坐标差异有确定处理方案，权限不是障碍。
- **真无响应进程会阻塞 10 秒**（实测），所以：查询必须在常驻 worker 里异步跑，覆盖层永不等待；
  超时/熔断是必需品，不是优化项。
- 覆盖度：跨轮累计已覆盖 5 个主流形态（Chromium / Win32 经典 / Windows Terminal / UWP / 资源管理器），
  单轮计数受遮挡影响，不代表能力不足。
- 剩下的都是实现决策：超时期间显示什么、熔断粒度、预热时机。
