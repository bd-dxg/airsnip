# 第三方组件声明

AirSnip 自身以 **GPL-3.0-only** 发布。以下是随程序一同分发的第三方开源组件（共 143 个 crate，均由 crates.io 获取）。

所有组件均使用宽松许可证，与 GPL-3.0-only 兼容（无 GPL-2.0-only 等不兼容条款）。清单包含运行期与构建期依赖（如 `embed-resource`），也包含 Linux 前端（`crates/airsnip-linux`）在 Linux 上构建时引入的依赖（Wayland 客户端、cosmic-text 等）；Windows 发布包不包含这些 crate。

自动校验：仓库根目录的 `deny.toml` + `cargo deny check`（许可证 / 安全公告 / 来源 / 重复版本），CI 与发布前均需通过。

## 按许可证分组

### (MIT OR Apache-2.0) AND IJG

- jpeg-encoder 0.7.1

### (MIT OR Apache-2.0) AND Unicode-3.0

- unicode-ident 1.0.26

### 0BSD OR MIT OR Apache-2.0

- adler2 2.0.1

### Apache-2.0

- unicode-linebreak 0.1.5

### Apache-2.0 OR GPL-2.0-only

- self_cell 1.3.0

### Apache-2.0 OR MIT

- base64ct 1.8.3
- const-oid 0.9.6
- der 0.7.10
- ed25519 2.2.3
- equivalent 1.0.2
- indexmap 2.14.2
- linebender_resource_handle 0.1.1
- pkcs8 0.10.2
- rustc-hash 2.1.3
- signature 2.2.0
- spki 0.7.3
- swash 0.2.10
- yazi 0.2.2
- zeno 0.3.3
- zeroize 1.9.1

### Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT

- linux-raw-sys 0.12.1
- rustix 1.1.6
- wasi 0.11.1+wasi-snapshot-preview1

### BSD-2-Clause

- arrayref 0.3.9

### BSD-3-Clause

- curve25519-dalek 4.1.3
- ed25519-dalek 2.2.0
- subtle 2.6.1
- tiny-skia 0.12.0
- tiny-skia-path 0.12.0

### MIT

- core_maths 0.1.1
- embed-resource 3.0.11
- fontconfig-parser 0.5.8
- fontdb 0.23.0
- generic-array 0.14.7
- harfrust 0.5.2
- libm 0.2.16
- quick-xml 0.41.0
- simd-adler32 0.3.10
- strict-num 0.1.1
- vswhom 0.1.0
- vswhom-sys 0.1.3
- wayland-backend 0.3.17
- wayland-client 0.31.15
- wayland-protocols 0.32.13
- wayland-protocols-wlr 0.3.12
- wayland-scanner 0.31.11
- wayland-sys 0.31.11
- winnow 1.0.4
- winreg 0.55.0
- zmij 1.0.23

### MIT OR Apache-2.0

- arrayvec 0.7.8
- bitflags 2.13.2
- block-buffer 0.10.4
- cc 1.6.0
- cfg-if 1.0.5
- cosmic-text 0.19.0
- cpufeatures 0.2.17
- crc32fast 1.5.2
- crypto-common 0.1.7
- digest 0.10.7
- errno 0.3.14
- fdeflate 0.3.7
- find-msvc-tools 0.1.14
- flate2 1.1.10
- font-types 0.11.3
- font-types 0.12.7
- getrandom 0.2.17
- hashbrown 0.17.1
- image-webp 0.2.4
- itoa 1.0.18
- libc 0.2.190
- log 0.4.34
- memmap2 0.9.12
- once_cell 1.21.4
- pkg-config 0.3.34
- png 0.18.1
- proc-macro2 1.0.107
- quote 1.0.47
- rand_core 0.6.4
- read-fonts 0.37.0
- read-fonts 0.41.0
- roxmltree 0.20.0
- rustc_version 0.4.1
- semver 1.0.28
- serde 1.0.229
- serde_core 1.0.229
- serde_derive 1.0.229
- serde_json 1.0.154
- serde_spanned 1.1.1
- sha2 0.10.9
- shlex 2.0.1
- skrifa 0.40.0
- skrifa 0.44.0
- smallvec 1.16.5
- smol_str 0.3.7
- syn 2.0.119
- syn 3.0.6
- sys-locale 0.3.2
- toml 1.1.6+spec-1.1.0
- toml_datetime 1.1.1+spec-1.1.0
- toml_parser 1.1.3+spec-1.1.0
- toml_writer 1.1.2+spec-1.1.0
- ttf-parser 0.25.1
- typenum 1.20.1
- unicode-bidi 0.3.18
- unicode-script 0.5.9
- unicode-segmentation 1.13.3
- windows 0.62.2
- windows-collections 0.3.2
- windows-core 0.62.2
- windows-future 0.3.2
- windows-implement 0.60.2
- windows-interface 0.59.3
- windows-link 0.2.1
- windows-numerics 0.3.1
- windows-result 0.4.1
- windows-strings 0.5.1
- windows-sys 0.59.0
- windows-targets 0.52.6
- windows-threading 0.2.1
- windows_aarch64_gnullvm 0.52.6
- windows_aarch64_msvc 0.52.6
- windows_i686_gnu 0.52.6
- windows_i686_gnullvm 0.52.6
- windows_i686_msvc 0.52.6
- windows_x86_64_gnu 0.52.6
- windows_x86_64_gnullvm 0.52.6
- windows_x86_64_msvc 0.52.6

### MIT OR Apache-2.0 OR BSD-1-Clause

- fiat-crypto 0.2.9

### MIT OR Zlib OR Apache-2.0

- miniz_oxide 0.8.9
- miniz_oxide 0.9.1

### MIT/Apache-2.0

- curve25519-dalek-derive 0.1.1
- downcast-rs 1.2.1
- quick-error 2.0.1
- rangemap 1.8.0
- version_check 0.9.5

### Unlicense OR MIT

- byteorder-lite 0.1.0
- memchr 2.8.3

### Zlib

- slotmap 1.1.1
- zlib-rs 0.6.8

### Zlib OR Apache-2.0 OR MIT

- bytemuck 1.25.2
- bytemuck_derive 1.12.1
- tinyvec 1.13.3

## 许可证全文

- MIT: https://opensource.org/license/mit
- Apache-2.0: https://www.apache.org/licenses/LICENSE-2.0
- BSD-3-Clause: https://opensource.org/license/bsd-3-clause
- BSD-2-Clause: https://opensource.org/license/bsd-2-clause
- Zlib: https://opensource.org/license/zlib
- 0BSD: https://opensource.org/license/0bsd
- Unlicense: https://unlicense.org/
- Unicode-3.0: https://www.unicode.org/license.txt
- IJG: https://www.ijg.org/
- ISC: https://opensource.org/license/isc-license-txt
## 图标与美术资源

界面图标位于 `assets/icons/ui/`，统一取自 **Material Symbols**（Apache-2.0）；应用图标与 README 头图为项目自绘。两类资源均与 GPL-3.0-only 兼容。

### Material Symbols（Apache-2.0）

- 来源：https://github.com/google/material-design-icons（`symbols/web/*/materialsymbolsoutlined`）
- 许可证：Apache-2.0，https://www.apache.org/licenses/LICENSE-2.0
- 使用范围：`assets/icons/ui/` 下全部 21 个 SVG（`toolbar/*` / `status/*` / `panel/*`）。仅去掉 `width`/`height`、给 `<path>` 补 `fill="currentColor"`，图形未做其他修改。

### 本项目自绘

- 范围：应用图标 `assets/icons/app.ico` / `logo.svg` / `logo-small.svg` / `logo-512.png`，以及 README 头图 `assets/hero.webp`。
- 许可：与项目本体一致，GPL-3.0-only。

