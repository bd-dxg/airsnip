// SPDX-License-Identifier: GPL-3.0-only
//! HDR 显示器状态探测（CAP-1，issue #75）：读取高级色彩（HDR）是否启用与 SDR 白点。
//!
//! 抓屏走 GDI `BitBlt` 时在 HDR 桌面上会拿到被压平的 8bit 帧（整屏约 23% 像素爆成纯白），
//! 因此需要先判断目标显示器是否处于 HDR 模式，再决定是否走 DXGI 抓屏 + 色调映射。

use std::mem::size_of;

use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SDR_WHITE_LEVEL, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};

/// scRGB 的参考白点：1.0 对应 80 nits。
const SCRGB_REFERENCE_NITS: f32 = 80.0;
/// `SDRWhiteLevel` 的单位是 1/1000 个参考白点。
const SDR_WHITE_STEP: f32 = SCRGB_REFERENCE_NITS / 1000.0;

/// 单个显示器的色彩状态。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayColorState {
    /// 是否处于高级色彩（HDR / scRGB）模式。
    pub hdr_enabled: bool,
    /// SDR 内容被合成到的亮度（nits）。HDR 未启用时为 80。
    pub sdr_white_nits: f32,
}

impl Default for DisplayColorState {
    fn default() -> Self {
        Self {
            hdr_enabled: false,
            sdr_white_nits: SCRGB_REFERENCE_NITS,
        }
    }
}

/// 读取全部启用中的显示器色彩状态，按 `DXGI_OUTPUT_DESC.DeviceName` 索引。
///
/// `DXGI_OUTPUT_DESC1.ColorSpace` 与 `DISPLAYCONFIG` 的 `AdvancedColorEnabled`
/// 并不总是等价（尤其多屏混合 DPI/HDR），这里以 DisplayConfig 为准，
/// 调用方再与 `ColorSpace` 取或。
pub fn active_display_states() -> Vec<(String, DisplayColorState)> {
    unsafe {
        let mut npaths = 0u32;
        let mut nmodes = 0u32;
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut npaths, &mut nmodes).is_err() {
            return Vec::new();
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); npaths as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nmodes as usize];
        let err = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut npaths,
            paths.as_mut_ptr(),
            &mut nmodes,
            modes.as_mut_ptr(),
            None,
        );
        if err.is_err() {
            return Vec::new();
        }

        let mut out = Vec::new();
        for path in paths.iter().take(npaths as usize) {
            let Some(name) = source_device_name(path) else { continue };
            let state = read_target_state(path);
            out.push((name, state));
        }
        out
    }
}

/// 取 path 对应的 GDI 设备名（如 `\\.\DISPLAY1`），可与 DXGI 的 `DeviceName` 直接比较。
fn source_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    use windows::Win32::Devices::Display::{
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    };
    unsafe {
        let mut req = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        req.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        req.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        req.header.adapterId = path.sourceInfo.adapterId;
        req.header.id = path.sourceInfo.id;
        if DisplayConfigGetDeviceInfo(&mut req.header) != 0 {
            return None;
        }
        let name = req.viewGdiDeviceName;
        let len = name.iter().position(|c| *c == 0).unwrap_or(name.len());
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&name[..len]))
    }
}

/// 读单个 target 的高级色彩启用状态与 SDR 白点。
fn read_target_state(path: &DISPLAYCONFIG_PATH_INFO) -> DisplayColorState {
    unsafe {
        let mut info = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
        info.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO;
        info.header.size = size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
        info.header.adapterId = path.targetInfo.adapterId;
        info.header.id = path.targetInfo.id;
        if DisplayConfigGetDeviceInfo(&mut info.header) != 0 {
            return DisplayColorState::default();
        }
        // bit0 = 硬件支持高级色彩，bit1 = 当前已启用
        if info.Anonymous.value & 0b10 == 0 {
            return DisplayColorState::default();
        }

        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
        white.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL;
        white.header.size = size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32;
        white.header.adapterId = path.targetInfo.adapterId;
        white.header.id = path.targetInfo.id;
        let sdr_white_nits = if DisplayConfigGetDeviceInfo(&mut white.header) == 0 && white.SDRWhiteLevel > 0 {
            white.SDRWhiteLevel as f32 * SDR_WHITE_STEP
        } else {
            SCRGB_REFERENCE_NITS
        };

        DisplayColorState {
            hdr_enabled: true,
            sdr_white_nits,
        }
    }
}

/// 按 GDI 设备名查状态；查不到时返回默认（视为 SDR，走 BitBlt 快路径）。
pub fn state_for_device(states: &[(String, DisplayColorState)], device_name: &str) -> DisplayColorState {
    states
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(device_name))
        .map(|(_, s)| *s)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdr_white_step_matches_windows_unit() {
        // 1000 个单位 == 80 nits 参考白
        assert!((1000.0 * SDR_WHITE_STEP - 80.0).abs() < 1e-3);
        // 3500 个单位 == 280 nits（实测机器上的取值）
        assert!((3500.0 * SDR_WHITE_STEP - 280.0).abs() < 1e-3);
    }

    #[test]
    fn default_state_treats_display_as_sdr() {
        let s = DisplayColorState::default();
        assert!(!s.hdr_enabled);
        assert_eq!(s.sdr_white_nits, 80.0);
    }

    #[test]
    fn lookup_is_case_insensitive_and_falls_back() {
        let states = vec![(
            "\\\\.\\DISPLAY1".to_string(),
            DisplayColorState {
                hdr_enabled: true,
                sdr_white_nits: 280.0,
            },
        )];
        assert!(state_for_device(&states, "\\\\.\\display1").hdr_enabled);
        // 未登记的显示器按 SDR 处理
        let other = state_for_device(&states, "\\\\.\\DISPLAY2");
        assert!(!other.hdr_enabled);
    }

    /// 本机查询不应 panic；无 HDR 环境时返回空或全 SDR 均属正常。
    #[test]
    fn active_display_query_is_safe() {
        let states = active_display_states();
        for (name, s) in &states {
            assert!(!name.is_empty());
            assert!(s.sdr_white_nits >= 80.0, "SDR 白点不应低于参考白：{s:?}");
        }
    }
}
