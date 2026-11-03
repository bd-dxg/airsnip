# ADR 0019：鼠标按键作为全局热键（Raw Input）

- 状态：已采纳（v0.8.0 开发期，对应看板 #67 的附带需求）
- 影响范围：`app::mouse`（新增）、`app::hotkey`/`app::wndproc`、`settings::panel`、`overlay`（`annotate`/`wndproc`/`mod`）、`pin::wndproc`
- 相关：PRD `SYS-2`、ADR 0017（快捷键清空）

## 背景

用户希望把截图 / 贴图 / 工具切换键绑到鼠标侧键或中键。`RegisterHotKey` 只接受键盘虚拟键码，
没有任何系统 API 能把鼠标键注册成全局热键。

## 决策

1. **Raw Input，不用低级鼠标钩子**。向隐藏消息窗口注册一台设备
   （`RAWINPUTDEVICE{ 0x01, 0x02, RIDEV_INPUTSINK }`），只收 `WM_INPUT`。
   否决 `WH_MOUSE_LL` 的理由：钩子每次鼠标移动都要跨进程回调到 airsnip 线程，
   回调超时 1 秒会被系统静默卸载；而 Raw Input 只投递消息、不阻塞输入链，
   且无鼠标输入时没有任何消息（待机 0% CPU 红线不变）。
   代价：移动鼠标时每个输入批次一条 `WM_INPUT`（含一次约 48 字节的 `GetRawInputData`）；
   **该项尚未实测**，补测方法与待填数据见 `docs/perf-m5.md`。
2. **虚拟键码沿用 `winuser.h` 的鼠标编号**：`VK_MBUTTON = 0x04`、`VK_XBUTTON1 = 0x05`、
   `VK_XBUTTON2 = 0x06`。这三个值在键盘虚拟键码区间之外，配置结构（`u32`）与旧
   `config.toml` 都不需要改动。
3. **只支持中键与两个侧键**。左右键不参与：左键要用来点键帽，右键要用来取消捕获 / 弹菜单，
   绑上去会让面板自身交互失效。
4. **不做鼠标+修饰键组合**。`handle_mouse_hotkey` 只比对虚拟键码；捕获鼠标键时强制
   `modifiers = 0`，避免显示成「Ctrl+侧键1」却实际不要求 Ctrl。
5. **`register_one` 跳过鼠标键**，不走 `RegisterHotKey`（否则弹「错误码 87」）。
6. **覆盖层与贴图窗口内直接处理鼠标消息**：这些窗口有焦点，`WM_MBUTTONDOWN` /
   `WM_XBUTTONDOWN` 直接分派给长截图键 / 工具切换键 / 边框键，不需要 Raw Input。
7. **无修饰键警告不因鼠标键触发**：鼠标键不抢占其他程序的键盘输入。
8. **覆盖层运行期间不响应全局鼠标热键**：Raw Input 照样投递到主窗口，不挡的话
   按鼠标截图键会在覆盖层上再起一份（每份 = 一份全屏位图）；贴图键同属 PIN-1 的
   「截图期间撤销全局贴图热键」。`start_capture` 另用 `OVERLAY_RUNNING` 做入口互斥，
   把键盘热键的同类重入一并堵住。

## 影响

- **不抢占**：Raw Input 只是旁听，其他程序照常收到侧键 / 中键。这与 `RegisterHotKey`
  的独占语义不同 —— 对截图工具是优点（不会让浏览器失去「中键开新标签页」）。
- **全屏独占游戏**下 `WM_INPUT` 可能不投递；`RIDEV_INPUTSINK` 已在系统层注册，
  正常窗口化应用与桌面上不受影响。
- 注册失败（极少）只影响鼠标热键，静默降级，`AIRSNIP_DEBUG` 下打印一行日志。

## 未采纳方案

- **`SetWindowsHookExW(WH_MOUSE_LL)`**：兼容性最好（PowerToys/AutoHotkey 的做法），
  但常驻钩子每次鼠标移动都要回调进程，与「常驻 ≈0 开销」的产品定位冲突。
- **`RIDEV_NOLEGACY`**：会同时屏蔽其他程序的鼠标消息，等于变相抢占。
- **只做覆盖层内生效**：实现最简单，但截图 / 贴图这两个最需要鼠标键的全局热键反而绑不上。
