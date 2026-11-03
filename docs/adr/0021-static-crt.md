# ADR 0021：静态链接 MSVC 运行时

- 状态：已采纳（v0.8.0 开发期）
- 影响范围：新增 `.cargo/config.toml`、`scripts/package.ps1`（打包守门）、`README.md`、`AGENTS.md`
- 相关：ADR 0003（管理员权限 manifest）、ADR 0018（自动更新）

## 背景

用户反馈：解压双击 `airsnip.exe` 后弹系统错误框——

```
airsnip.exe - 系统错误
由于找不到 VCRUNTIME140.dll，无法继续执行代码。重新安装程序可能会解决此问题。
```

根因不是用户机器缺环境，而是**发布包缺依赖**：Rust 的 `x86_64-pc-windows-msvc` 目标默认动态链接
VC 运行时（`/MD`），产出的 exe 导入 `VCRUNTIME140.dll` 与 6 个 `api-ms-win-crt-*.dll`。
开发机装了 Visual Studio（`C:\Windows\System32\vcruntime140.dll`），所以本地一直跑得起来；
干净的 Windows 机器没装 VC++ Redistributable，就在加载器阶段直接失败。

这与项目定位直接冲突：README 承诺「解压即用、不装运行时、绿色单文件」。

## 决策

**静态链接 CRT**：新增 `.cargo/config.toml`

```toml
[target.x86_64-pc-windows-msvc]
rustflags = ["-C", "target-feature=+crt-static"]
```

### 实测数据

| 项 | 动态 CRT | 静态 CRT |
| --- | --- | --- |
| exe 体积 | 1,264,640 B（1.21 MB） | 1,393,152 B（1.33 MB） |
| 导入表里的 CRT | `VCRUNTIME140.dll` + 6 个 `api-ms-win-crt-*` | 无 |
| `cargo test` | — | 228 passed / 0 failed（3 项 ignored 不变） |

体积增加 128 KB（+10%），仍远低于 PRD 的 5 MB 红线；导入表只剩 `kernel32 / user32 / gdi32 /
imm32 / shell32 / ole32 / oleaut32 / combase / advapi32 / dwmapi / dwrite / winhttp / ntdll` 与两个
`api-ms-win-core-*`（`synch` / `shcore-scaling`）——全部是 Windows 10/11 自带组件。

### 打包守门

`scripts/package.ps1` 新增 `Get-PeImportedDlls`：直接解析 PE 导入表（不依赖 `dumpbin`，
没有 VS 的机器也能跑），发现 `VCRUNTIME* / MSVCP* / CONCRT* / api-ms-win-crt* / ucrtbase / msvcrt`
就 `throw`。理由是这类回归**静默且只在别人的机器上暴露**：本地构建永远成功，只有用户下载后双击才发现。

## 未采纳方案

- **随包分发 `vc_redist.x64.exe`**：约 25 MB，是 exe 本体的 19 倍；还要管理员安装、可能要求重启，
  与「1 MB 绿色单文件」的卖点正面冲突。
- **只在 README 写「需先安装 VC++ 运行库」**：绝大多数用户不会读，反馈会持续出现；
  而且用户认知里「装个截图工具还得装运行库」本身就是缺陷。
- **改用 GNU 工具链（`x86_64-pc-windows-gnu`）**：同样能静态链接运行时，但会连带换掉链接器与
  部分系统库绑定行为，与 windows-rs 的 `raw-dylib` / 导入库选择耦合，风险远大于收益。

## 影响

- **dev 构建同样静态链接**：`cargo build` / `cargo test` 都走同一份 rustflags。测试已验证无差异。
  build script 与 proc-macro 由 rustc 自动按动态链接处理，不受影响。
- **`cargo deny check`**：不涉及新依赖，配置无需改动。
- **自动更新（ADR 0018）**：替换的是 exe 本体，静态链接后包更大 128 KB，流程不变。
- **不要在 `.cargo/config.toml` 之外改用 `RUSTFLAGS` 环境变量**：环境变量会覆盖 config 里的
  `target.*.rustflags`，若该变量里忘了带 `+crt-static`，会悄悄退回动态链接——此时靠打包守门拦截。
- 若将来加入依赖 C 代码的 crate（`cc` 编译），需确认其同样支持 `+crt-static`；`cargo build --release`
  能通过即说明没问题。
