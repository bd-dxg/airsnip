# ADR 0018：自动更新（检查 / 加速下载 / 签名校验 / 自动替换）

- 状态：已采纳（v0.7.0 开发期，对应看板 #59）
- 影响范围：`update`（新模块）、`app`（启动检查与退出）、`settings`（关于页）、发布脚本
- 相关：PRD `SYS-7`、ADR 0003（管理员权限与任务计划）、ADR 0017（快捷键清空）

## 背景

用户只能手动去 Release 下载覆盖。需求是：静默检查新版本、国内网络可用的下载线路、
发版产物可校验防篡改。约束不变：待机 CPU 0%、常驻内存 ≤ 15MB、不阻塞消息循环。

## 决策

### 1. HTTP 用系统 WinHTTP，不引入 reqwest

`cargo` 里没有 HTTP 客户端。reqwest 会带进 tokio 运行时，与「待机 0% CPU、常驻 ≤ 15MB」
冲突，而这里只需要几百字节 JSON + 一个 ~1MB 的 exe。WinHTTP 是系统组件，零额外体积、
自动走系统代理；代价是一段 unsafe 调用与手写 URL 拆分（只支持 `https://`）。

### 2. 清单扁平结构，签名绑定版本

`latest.json` 字段：`version` / `pub_date` / `notes` / `url` / `sha256` / `signature`。
只支持单平台（windows-x86_64），不照抄 tauri 的 `platforms` 嵌套。

签名载荷固定为三行文本：

```
airsnip-update-v1
<version>
<sha256 小写>
```

理由：**版本号必须进签名**，否则能把「新版本号 + 旧安装包」拼在一起绕过降级检查。
客户端 `Manifest::payload()` 与发布脚本 `update-sign` 必须逐字一致，两边各有测试/自检守着。

### 3. 验签优先于一切字段

`manifest.verify()` 在读取版本、URL、摘要**之前**执行。不验签就相信清单里的任何字段，
等于把「换包」的权力交给了能改清单的人。hash 只防传输损坏，不防篡改（清单与包同源，
能改包的人也能顺手改 hash），所以不用 hash 做安全判断。

### 4. 克隆加速线路的策略与 issue 建议不同

issue 建议「清单里的 url 直接写带加速前缀的地址」。这里改成：

- 清单与安装包都按同一顺序试地址：`gh-proxy.org` → `cdn.gh-proxy.org` → `v4.gh-proxy.org` → GitHub 直连。
- 清单里存**GitHub 规范地址**，加速前缀由客户端套用（`endpoints::candidates`）。

好处是镜像换域名不需要重新发版；代价是客户端多一个字符串拼接。清单里若写了完整镜像
地址也能照常工作（非 GitHub 地址不加前缀）。

### 5. 更新包用裸 exe，不用 zip

自动更新只需要替换 exe。zip 会引入解压依赖（新 crate）与「解压到哪」的问题。
发布时 `package.ps1` 同时产出两个资产：`airsnip-v{ver}-win-x64.zip`（手动下载，含
LICENSE / THIRD-PARTY / README）与 `airsnip-v{ver}-win-x64.exe`（自动更新）。
清单指向后者。

### 6. 替换 exe 用独立 cmd 脚本

Windows 下运行中的 exe 不能被覆盖。流程：下载到 `%TEMP%` → 校验和比对 → 写一个 `.cmd`
→ 启动它 → 主进程退出 → 脚本轮询父进程消失 → `copy /y` 覆盖 → 启动新 exe → 自删。

- 程序始终以管理员运行，脚本继承提权，无需再弹 UAC。
- `copy` 失败时仍启动旧程序，避免「更新完打不开」；代价是用户下次启动还是旧版。
- 用 `CommandExt::creation_flags(CREATE_NO_WINDOW)` 起脚本，不弹黑窗。

### 7. 触发时机与性能

启动后 3 秒一次性 `SetTimer` 触发检查，回调里立刻 `KillTimer`——不引入常驻轮询线程，
待机仍是 0% CPU。检查失败静默（国内网络下镜像挂掉是常态），只在用户手动点击时把原因
显示在关于页。发现新版本时弹一次托盘气泡，不弹模态框。

### 8. 密钥管理

- 私钥：`%USERPROFILE%\.airsnip\update.key`（64 位十六进制 seed），**不在仓库里**。
- 公钥：编译进 `src/update/manifest.rs` 的 `UPDATE_PUBKEY_HEX`。
- 生成：`cargo run --example update-keygen`（已存在时需 `--force` 才覆盖，防止误删导致
  旧客户端永远无法更新）。
- 签名：`cargo run --example update-sign -- --version <ver> --sha256 <摘要>`。
- **更换密钥等于切断旧版本的自动更新能力**（旧客户端内置的是旧公钥）。

### 9. 依赖与开关

新增 `ed25519-dalek`（Apache-2.0/MIT）、`sha2`（MIT/Apache-2.0）、`serde_json`（MIT/Apache-2.0），
均与 GPL-3.0-only 兼容，已在 `deny.toml` 白名单内并记入 `THIRD-PARTY.md`。
配置项：`auto_check_update`（默认开，关于页可关）、`skipped_version`（预留：跳过此版本）。

## 影响

- 常驻内存新增约 0（不下载时只有几个静态状态），下载期间按块 64KB 复用缓冲。
- 下载不设整条请求总超时（慢网也能下完），只设连接 5s / 读取 10s。
- 若用户把 exe 放在无写权限目录，替换会失败并退回旧版；不额外弹窗（气泡已发过）。

## 未采纳方案

- **只校验 SHA-256**：清单与包同源，两者一起改就没意义。
- **Windows CNG 验签**：零新依赖，但需手写 key blob 解析，代码量与出错面都更大。
- **zip + 内置解压**：多一个解压 crate 与临时目录清理问题，收益仅是「多更新几个附属文件」。
