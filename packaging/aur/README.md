# AUR 发布（airsnip）

`PKGBUILD` 走**源码构建**：从 GitHub tag 源码包编译 `crates/airsnip-linux`，
装到 `/usr/bin/airsnip`（含 `LICENSE`、`THIRD-PARTY.md`、`README.md`）。

> 前提：只支持**包含 Linux 前端的版本**（≥ v0.12.0）。v0.11.0 及更早的源码包里
> 没有 `crates/airsnip-linux`，`build()` 会失败。

## 本地验证（未发版时）

tag 还不存在时，把 `source` 换成分支归档即可（GitHub 会把分支名拼进目录名，
所以 `cd` 也要跟着改）：

```sh
cp packaging/aur/airsnip/PKGBUILD /tmp/airsnip-aur/ && cd /tmp/airsnip-aur
sed -i 's|refs/tags/v$pkgver.tar.gz|refs/heads/feat/linux-port.tar.gz|' PKGBUILD
sed -i "s|cd \"\$pkgname-\$pkgver\"|cd \"\$srcdir\"/airsnip-feat-linux-port|g" PKGBUILD
makepkg -f --skipinteg          # 只验证 build()/package()，跳过摘要校验
pacman -Qlp airsnip-*.pkg.tar.zst
```

正式发版后，直接用仓库里的 PKGBUILD 跑 `makepkg -f` 即可（摘要已经填好）。

## 发版时更新 PKGBUILD

1. 打 tag（`git tag v0.12.0 && git push origin v0.12.0`）
2. 改 `pkgver=0.12.0`
3. 重算摘要：`updpkgsums`（或 `sha256sum` 后手填）
4. 重生成元信息：`makepkg --printsrcinfo > .SRCINFO`
5. `makepkg -f` 本地打一次包确认能装：`sudo pacman -U airsnip-0.12.0-1-x86_64.pkg.tar.zst`

## 首次发布到 AUR

AUR 是一个独立 git 仓库，**push 需要 AUR 账号 + 已登记的 SSH 公钥**
（<https://aur.archlinux.org/> → My Account → SSH Public Key）。

```sh
# 1. 克隆 AUR 仓库（首次是空仓库）
git clone ssh://aur@aur.archlinux.org/airsnip.git
cd airsnip

# 2. 放入 PKGBUILD 与 .SRCINFO（AUR 只认这两个文件）
cp /path/to/airsnip/packaging/aur/airsnip/PKGBUILD .
cp /path/to/airsnip/packaging/aur/airsnip/.SRCINFO .

# 3. 提交并推送
git add PKGBUILD .SRCINFO
git commit -m "airsnip 0.12.0-1"
git push origin master
```

之后每次发版重复「改 pkgver → updpkgsums → printsrcinfo → 提交推送」。

用户安装：

```sh
yay -S airsnip      # 或 paru -S airsnip
```

## 为什么没有 `-bin` 包

`-bin` 包要指向一个**已发布的预编译产物**。本仓库的 Linux 产物现在是
`scripts/package-linux.sh` 产出的 `airsnip-v<版本>-linux-x86_64.tar.gz`，
目前只作为 CI artifact 上传，还没有挂到 GitHub Release 上。
等发布流程把该 tgz 挂到 Release 后，可以再加 `airsnip-bin`：

```sh
source=("$pkgname-$pkgver.tar.gz::https://github.com/bd-dxg/airsnip/releases/download/v$pkgver/airsnip-v$pkgver-linux-x86_64.tar.gz")
```
