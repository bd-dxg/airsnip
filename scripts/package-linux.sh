#!/usr/bin/env sh
# SPDX-License-Identifier: GPL-3.0-only
#
# 打 Linux 发布包（ADR 0034）：构建 release → 校验 → 输出
#   dist/airsnip-v<版本>-linux-x86_64.tar.gz
#   dist/SHA256SUMS（含 tgz 与裸二进制的摘要）
#
# 与 Windows 的 scripts/package.ps1 对应，但只做 Linux 该做的事：
#   - 静态链接 MSVC CRT 之类的检查不适用（Linux 侧依赖 glibc，构建机决定性）
#   - 自动更新清单（latest.json）只在 Windows 侧发布，Linux 不做
#
# 用法：scripts/package-linux.sh [--skip-build]
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

skip_build=0
[ "${1:-}" = "--skip-build" ] && skip_build=1

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/airsnip-linux/Cargo.toml | head -1)
[ -n "$version" ] || { echo "读不到版本号（crates/airsnip-linux/Cargo.toml）" >&2; exit 1; }

name="airsnip-v${version}-linux-x86_64"
dist="$root/dist"
bin="$root/target/release/airsnip"

if [ "$skip_build" -eq 0 ]; then
    echo "==> 构建 release（airsnip-linux）"
    cargo build -p airsnip-linux --release
fi
[ -f "$bin" ] || { echo "没找到 $bin（先跑一次不带 --skip-build 的构建）" >&2; exit 1; }

echo "==> 校验二进制"
size_kb=$(du -k "$bin" | cut -f1)
echo "    体积：${size_kb} KB"
# 体积红线：Windows 版目标 < 2 MB；Linux 版含 cosmic-text/wayland 略大，按 6 MB 守门，
# 超了说明有依赖被意外静态链接进来（例如字体或图像库）。
if [ "$size_kb" -gt 6144 ]; then
    echo "  ✗ 体积超标（> 6 MB）" >&2
    exit 1
fi
# 冒烟：子命令能跑、帮助能打印（同时验证不依赖未安装的动态库）
"$bin" --help >/dev/null
echo "    --help 正常"
# 动态依赖只允许 glibc / libgcc / libm 这类系统库
echo "    ldd："
ldd "$bin" | sed 's/^/      /'

echo "==> 组装 $name"
stage="$dist/$name"
rm -rf "$stage"
mkdir -p "$stage"
install -m755 "$bin" "$stage/airsnip"
for f in README.md LICENSE THIRD-PARTY.md; do
    [ -f "$f" ] && cp "$f" "$stage/"
done
cp scripts/install-linux.sh "$stage/install-linux.sh"
chmod +x "$stage/install-linux.sh"

echo "==> 打包"
tgz="$dist/$name.tar.gz"
rm -f "$tgz"
tar -C "$dist" -czf "$tgz" "$name"
cp "$bin" "$dist/airsnip-linux-x86_64"

echo "==> 摘要"
(cd "$dist" && sha256sum "$(basename "$tgz")" airsnip-linux-x86_64 > SHA256SUMS && cat SHA256SUMS)

echo
echo "完成："
echo "  $tgz（$(du -k "$tgz" | cut -f1) KB）"
echo "  $dist/airsnip-linux-x86_64（裸二进制）"
echo "  $dist/SHA256SUMS"
