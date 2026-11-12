#!/usr/bin/env sh
# SPDX-License-Identifier: GPL-3.0-only
#
# AirSnip Linux 前端安装脚本（ADR 0034）。
#
# 只做两件事：构建 release、把二进制装到 ~/.local/bin/airsnip。
# **不碰** 你的合成器配置：热键绑定需要你按自己的桌面写（见下方提示），
# 脚本不去猜配置格式与文件位置。
#
# 用法：scripts/install-linux.sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

cargo build -p airsnip-linux --release
install -Dm755 target/release/airsnip "$HOME/.local/bin/airsnip"

echo "已安装：$HOME/.local/bin/airsnip"
case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "提示：$HOME/.local/bin 不在 PATH 里，绑定热键时请用绝对路径。" ;;
esac
cat <<'EOF'

绑定热键（示例，按你的合成器写）：

niri（~/.config/niri/config.kdl 或其中的 binds 文件）：
    binds {
        Mod+F1 { spawn "/home/you/.local/bin/airsnip" "snip"; }
    }

sway / i3：
    bindsym $mod+F1 exec ~/.local/bin/airsnip snip

Hyprland（~/.config/hypr/hyprland.conf）：
    bind = SUPER, F1, exec, ~/.local/bin/airsnip snip

不要用程序自己注册全局热键的写法：Wayland 没有该协议，热键必须由合成器绑定。
EOF
