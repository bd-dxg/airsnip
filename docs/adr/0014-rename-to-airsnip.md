# 14. 项目与软件改名为 AirSnip（轻截）

- **Status**: Accepted
- **Context**: 原名 `jietu` 是「截图」的拼音，对中文用户直白，但作为项目与品牌名有两个问题：一是英文用户无法从拼写读出功能，GitHub 上的可发现性差；二是它只是功能描述，不承载产品主张。本项目真正的差异化是「极轻」——1 MB 单文件、待机 0% CPU / 1.2 MB 内存、热键 0.12 秒出画面，而 `jietu` 一个字都说不出这些。备选 `airsnip` 的 `snip` 精准对应截图动作、`air` 对应「轻、无感、常驻不占资源」，与定位一致；且 GitHub 上无同名项目占用（仅有 3 个 0 star 的小仓库）。改名时机在 v0.5.0 之后、用户量仍小，成本随用户数线性增长，越晚越贵。
- **Decision**:
  1. **展示名 `AirSnip`，中文副名「轻截」**：界面（托盘提示、设置窗口标题、关于页）、README、Releases 标题统一写作 `AirSnip 轻截` 或 `AirSnip`；中文语境不使用小写 `airsnip`。
  2. **技术标识全小写 `airsnip`**：crate 名 / exe 名 / zip 名（`airsnip-v{版本}-win-x64.zip`）、GitHub 仓库名、配置目录、任务计划名、窗口类名、单实例 mutex、日志前缀、文件名模板前缀。
  3. **具体映射**（旧 → 新）：
     - `%APPDATA%\jietu` → `%APPDATA%\airsnip`
     - 任务计划名 `jietu` → `airsnip`（`src/app/autostart.rs`）
     - `Local\jietu.single.instance` → `Local\airsnip.single.instance`
     - 窗口类 `jietu.hidden` / `.overlay` / `.settings` / `.pin` / `.popmenu` / `.menu` / `.longshot[.border|.preview]` → `airsnip.*`
     - 默认保存目录 `图片\jietu` → `图片\airsnip`；文件名模板 `jietu-YYYYMMDD-HHMMSS` → `airsnip-...`
     - 环境变量 `JIETU_TIMING` / `JIETU_DEBUG` → `AIRSNIP_TIMING` / `AIRSNIP_DEBUG`
     - 调试日志 `%TEMP%\jietu-longshot.log` → `airsnip-longshot.log`
     - `app.manifest` / `app.dev.manifest` 的 `assemblyIdentity name`
  4. **不做存量迁移**（明确取舍）：不读取旧配置目录、不清理旧任务计划、不搬移旧保存目录。理由是本项目当前用户极少，迁移代码的长期维护成本高于一次性手动处理；代价见 Consequences。
  5. **仓库重命名**：`bd-dxg/jietu` → `bd-dxg/airsnip`（GitHub 对旧 URL 自动重定向，`app/open.rs` 的项目地址与第三方声明地址随之更新）。
- **Consequences**:
  - 更容易：品牌与产品定位一致；英文用户可凭名字判断用途；`airsnip-core`、`AirSnip CLI` 等后续延展命名自然。
  - 更难／代价（**存量 v0.5.0 用户需手动处理**）：
    - 旧配置不继承——新版首次启动按默认值重建 `%APPDATA%\airsnip\config.toml`，热键、保存目录、字体等自定义设置需重设一次；旧 `%APPDATA%\jietu` 可手动删除。
    - **旧任务计划 `jietu` 必须手动删除**（`schtasks /delete /tn jietu /f`，需管理员）。若不删，开机时会同时拉起旧版与新版两份程序。
    - **单实例 mutex 已改名**：旧版与新版互不排斥，可同时运行并抢占同一组热键。升级后请先退出旧版（或直接删除旧 exe），不要新旧并存。
    - 旧截图仍留在 `图片\jietu`，新版默认保存到 `图片\airsnip`，历史文件不会自动搬家。
  - **陷阱**：`scripts/perf-capture.ps1` 通过窗口类名（`airsnip.hidden` / `airsnip.overlay`）定位窗口、通过 `[airsnip]` 日志前缀解析耗时，改名后脚本与程序必须同版本使用，否则采集会失败。
