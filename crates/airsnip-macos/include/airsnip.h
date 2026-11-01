// SPDX-License-Identifier: GPL-3.0-only
// AirSnip 共享核心 C ABI（实现：crates/airsnip-macos/src/）。
//
// 约定（docs/macos-port.md §3.2）：
// - 返回 int32_t 状态码：AIRSNIP_OK 成功；失败时 airsnip_last_error() 给出说明
//   （本线程最近一次失败，UTF-8，指针在本线程下次失败前有效，调用方不释放）。
// - Rust 内部 panic 不会跨越本接口，统一转为 AIRSNIP_ERR_PANIC（无返回值的函数则忽略该次操作）。
// - 图像为 RGBA8 预乘；传入的缓冲只在调用期间借用，Rust 会复制所需数据。
// - 字符串入参为 NUL 结尾 UTF-8。
// - 覆盖层（AsOverlay）的所有函数只在主线程调用。

#ifndef AIRSNIP_H
#define AIRSNIP_H

#include <stddef.h>
#include <stdint.h>

#define AIRSNIP_OK 0
#define AIRSNIP_ERR_FAILED (-1)
#define AIRSNIP_ERR_PANIC (-2)

/// 图像缓冲（借用）：data 至少 stride * height 字节，每行前 width * 4 字节有效。
typedef struct {
    const uint8_t *data;
    uint32_t width;
    uint32_t height;
    uint32_t stride;
} AsImage;

/// 核心版本号（静态字符串）。
const char *airsnip_version(void);

/// 本线程最近一次失败的说明（无错误时为空串）。
const char *airsnip_last_error(void);

/// 按扩展名（png / jpg / jpeg / webp）编码并写入 path。
int32_t airsnip_save_image(const AsImage *img, const char *path);

/// 截图热键：kVK_* 键码与 Carbon 修饰位（controlKey 0x1000 / optionKey 0x0800 / shiftKey 0x0200 / cmdKey 0x0100）。
void airsnip_config_shot_hotkey(uint32_t *key, uint32_t *mods);

// ---------------------------------------------------------------- 覆盖层

/// 光标形状（airsnip_core::overlay::Shape 的判别值）。
enum {
    AIRSNIP_CURSOR_ARROW = 0,
    AIRSNIP_CURSOR_CROSS = 1,
    AIRSNIP_CURSOR_HAND = 2,
    AIRSNIP_CURSOR_MOVE = 3,
    AIRSNIP_CURSOR_NS = 4,
    AIRSNIP_CURSOR_WE = 5,
    AIRSNIP_CURSOR_NWSE = 6,
    AIRSNIP_CURSOR_NESW = 7,
    AIRSNIP_CURSOR_RADIUS = 8,
};

/// 修饰键位（airsnip_overlay_set_mods / _wheel / _key 的 mods 参数）。
#define AIRSNIP_MOD_SHIFT 1u
#define AIRSNIP_MOD_SPACE 2u
#define AIRSNIP_MOD_CMD 4u

/// 宿主回调表。带 ctx 的回调在主线程、在 airsnip_overlay_* 调用内同步触发；
/// clipboard_png / notify 不带 ctx，可能在后台线程调用，实现方需自行回主线程。
/// 任何回调都可为 NULL（视为不支持）。
typedef struct {
    void *ctx;
    /// 上屏：覆盖层像素坐标的矩形需要重绘（从 airsnip_overlay_display 取像素）。
    void (*present)(void *ctx, int32_t x, int32_t y, int32_t w, int32_t h);
    void (*set_cursor)(void *ctx, int32_t shape);
    /// 结束覆盖层：只能安排异步释放，不得在回调内调用 airsnip_overlay_free。
    void (*close)(void *ctx);
    void (*alert)(void *ctx, const char *msg);
    /// 贴图（PIN-1）：img 只在回调期间有效；返回非 0 表示成功。
    int32_t (*pin)(void *ctx, const AsImage *img, int32_t x, int32_t y);
    /// 另存为对话框：选定路径写入 out（NUL 结尾）并返回非 0；取消返回 0。dir 可为 NULL。
    int32_t (*pick_save_file)(void *ctx, const char *dir, const char *name, const char *ext, char *out, size_t out_len);
    /// 文本输入期间开启输入法（EDT-4）。
    void (*set_ime)(void *ctx, int32_t enabled);
    /// 复制完成：PNG 数据只在回调期间有效（后台线程）。
    void (*clipboard_png)(const uint8_t *png, size_t len, uint32_t width, uint32_t height);
    /// 后台失败提示（复制 / 保存）。
    void (*notify)(const char *msg);
} AsHost;

typedef struct AsOverlay AsOverlay;

/// 创建覆盖层：screen 为该显示器像素的 RGBA 预乘图；backing_scale 为 backingScaleFactor；
/// dark 为系统当前是否深色外观。失败返回 NULL。host->ctx 须在覆盖层释放前保持有效。
AsOverlay *airsnip_overlay_new(const AsImage *screen, float backing_scale, int32_t dark, const AsHost *host);
/// 释放覆盖层（同时记住本次的工具、保存格式与目录）。
void airsnip_overlay_free(AsOverlay *ov);
/// 借用上屏缓冲（RGBA 预乘），在下一次调用该覆盖层的函数前有效。
AsImage airsnip_overlay_display(AsOverlay *ov);

void airsnip_overlay_set_mods(AsOverlay *ov, uint32_t mods, int32_t x, int32_t y);
void airsnip_overlay_mouse_down(AsOverlay *ov, int32_t x, int32_t y);
void airsnip_overlay_mouse_move(AsOverlay *ov, int32_t x, int32_t y);
void airsnip_overlay_mouse_up(AsOverlay *ov);
void airsnip_overlay_double_click(AsOverlay *ov, int32_t x, int32_t y);
void airsnip_overlay_cancel(AsOverlay *ov);
/// 滚轮：steps 为带符号格数（触控板连续滚动由调用方归并成格）。
void airsnip_overlay_wheel(AsOverlay *ov, int32_t x, int32_t y, int32_t steps, uint32_t mods);
/// 按键（kVK_* 键码）：返回非 0 表示已处理。
int32_t airsnip_overlay_key(AsOverlay *ov, uint32_t code, uint32_t mods);
/// 是否正在文本输入（此时空格是字符，不是「按住拖动选区」）。
int32_t airsnip_overlay_editing_text(AsOverlay *ov);
int32_t airsnip_overlay_has_selection(AsOverlay *ov);
/// 关闭后的结果：0 完成、1 取消、2 长截图（viewport 写入 x, y, w, h，可为 NULL）。
int32_t airsnip_overlay_result(AsOverlay *ov, int32_t *viewport);

#endif
