// SPDX-License-Identifier: GPL-3.0-only
//! HDR 色调映射（CAP-1，issue #75）：scRGB FP16 → 8bit sRGB。
//!
//! Windows 开启 HDR 后，桌面合成在 scRGB（线性，1.0 == 80 nits）空间，HDR 高光可达数千 nits。
//! 传统 GDI `BitBlt` 只能拿 8bit 降级帧，高光被压平截断（实测整屏 23% 像素爆成纯白）。
//! 本模块把 DDA 抓回的 FP16 帧按显示器 SDR 白点归一化后编码回 sRGB，
//! SDR 内容精确还原到 [0,1]，HDR 高光平滑压到白（不硬截断）。
//!
//! 映射策略经实机 A/B 验证（见 ADR 0024）：`÷SDR白点 + clip` 与屏幕观感最接近，
//! 与 OBS、ShareX-HDR 的输出一致。

/// fp16 → f32 的查表长度（half 位模式全部 65536 种）。
const HALF_LUT_LEN: usize = 65536;
/// sRGB OETF 查表精度。
const SRGB_LUT_LEN: usize = 1024;

/// 单个 fp16 位模式转 f32（NaN/Inf 归零，避免污染后续运算）。
fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1F) as i32;
    let mant = (h & 0x3FF) as f32;
    if exp == 0 {
        sign * mant * 2f32.powi(-24)
    } else if exp == 31 {
        0.0 // Inf / NaN
    } else {
        sign * (1.0 + mant / 1024.0) * 2f32.powi(exp - 15)
    }
}

/// linear [0,1] → sRGB 8bit 的查表。
pub fn srgb_lut() -> Vec<u8> {
    (0..SRGB_LUT_LEN)
        .map(|i| {
            let v = i as f32 / (SRGB_LUT_LEN - 1) as f32;
            let s = if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0 + 0.5).clamp(0.0, 255.0) as u8
        })
        .collect()
}

/// sRGB 编码单值（供测试与少量调用）。
pub fn srgb_encode(v: f32, lut: &[u8]) -> u8 {
    let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    lut[(v * (SRGB_LUT_LEN - 1) as f32 + 0.5) as usize]
}

/// scRGB 归一化倍数：scRGB 的 1.0 对应 80 nits，除以该倍数后 SDR 白点恰好落在 1.0。
pub fn scrgb_scale(sdr_white_nits: f32) -> f32 {
    (sdr_white_nits / 80.0).max(0.01)
}

/// 合成「fp16 位模式 → 最终 8bit」查表。
///
/// 把「拆位 → 除白点 → clamp → sRGB 编码」四步压成一次查表，
/// 逐像素只剩 3 次数组访问（64 KB 表，可驻 L2）。
pub fn encode_lut(sdr_white_nits: f32) -> Vec<u8> {
    let scale = scrgb_scale(sdr_white_nits);
    let srgb = srgb_lut();
    (0..HALF_LUT_LEN)
        .map(|raw| srgb_encode(half_to_f32(raw as u16) / scale, &srgb))
        .collect()
}

/// FP16 scRGB 帧 → RGBA8。
///
/// `row_pitch` 是源每行字节数（D3D11 staging 纹理常带对齐填充）；
/// `sdr_white_nits` 取自 `DISPLAYCONFIG_SDR_WHITE_LEVEL`。
pub fn tonemap(src: &[u8], row_pitch: usize, w: u32, h: u32, sdr_white_nits: f32) -> Vec<u8> {
    let enc = encode_lut(sdr_white_nits);
    tonemap_with(src, row_pitch, w, h, &enc)
}

/// 用预算好的编码表做映射（批量转换时避免重复建表）。
pub fn tonemap_with(src: &[u8], row_pitch: usize, w: u32, h: u32, enc: &[u8]) -> Vec<u8> {
    let w_us = w as usize;
    let rows = h as usize;
    let mut out = vec![0u8; w_us * rows * 4];

    std::thread::scope(|scope| {
        let workers = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(4)
            .clamp(1, 8);
        let chunk = rows.div_ceil(workers).max(1);
        let out_addr = out.as_mut_ptr() as usize;
        let src_addr = src.as_ptr() as usize;
        for start in (0..rows).step_by(chunk) {
            let end = (start + chunk).min(rows);
            let enc = enc;
            scope.spawn(move || {
                for y in start..end {
                    // 行起点按 pitch 偏移，长度只取有效像素（pitch 尾部是填充）
                    let row =
                        unsafe { std::slice::from_raw_parts((src_addr as *const u8).add(y * row_pitch), w_us * 8) };
                    let dst_row = unsafe { (out_addr as *mut u8).add(y * w_us * 4) };
                    for x in 0..w_us {
                        let px = &row[x * 8..x * 8 + 8];
                        let r = enc[u16::from_le_bytes([px[0], px[1]]) as usize];
                        let g = enc[u16::from_le_bytes([px[2], px[3]]) as usize];
                        let b = enc[u16::from_le_bytes([px[4], px[5]]) as usize];
                        unsafe {
                            let dst = dst_row.add(x * 4);
                            dst.write(r);
                            dst.add(1).write(g);
                            dst.add(2).write(b);
                            dst.add(3).write(255);
                        }
                    }
                }
            });
        }
    });
    out
}

/// BGRA8 帧 → RGBA8（非 HDR 输出时 DDA 仍可能返回 8bit）。
pub fn bgra_to_rgba(src: &[u8], row_pitch: usize, w: u32, h: u32) -> Vec<u8> {
    let w_us = w as usize;
    let rows = h as usize;
    let mut out = vec![0u8; w_us * rows * 4];
    for y in 0..rows {
        let row = &src[y * row_pitch..y * row_pitch + w_us * 4];
        for x in 0..w_us {
            let px = &row[x * 4..x * 4 + 4];
            let dst = &mut out[(y * w_us + x) * 4..][..4];
            dst.copy_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_conversion_matches_known_values() {
        // 1.0 / 0.5 / 2.0 / 0 的 IEEE 754 half 位模式
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0x4000), 2.0);
        assert_eq!(half_to_f32(0x0000), 0.0);
        // Inf / NaN 归零，避免污染映射
        assert_eq!(half_to_f32(0x7C00), 0.0);
        assert_eq!(half_to_f32(0x7E00), 0.0);
        // 负值保留符号
        assert_eq!(half_to_f32(0xBC00), -1.0);
    }

    #[test]
    fn srgb_lut_hits_endpoints() {
        let lut = srgb_lut();
        assert_eq!(lut[0], 0);
        assert_eq!(lut[SRGB_LUT_LEN - 1], 255);
        // 线性 0.5 对应的 sRGB 值约 188
        let mid = lut[SRGB_LUT_LEN / 2];
        assert!((186..=190).contains(&mid), "线性 0.5 → sRGB {mid}，预期约 188");
    }

    #[test]
    fn srgb_encode_clamps_out_of_range() {
        let lut = srgb_lut();
        assert_eq!(srgb_encode(-1.0, &lut), 0);
        assert_eq!(srgb_encode(5.0, &lut), 255);
        assert_eq!(srgb_encode(f32::NAN, &lut), 0);
    }

    #[test]
    fn scrgb_scale_uses_80_nit_reference() {
        // 80 nits 是 scRGB 的 1.0 基准，倍率恰为 1
        assert!((scrgb_scale(80.0) - 1.0).abs() < 1e-6);
        // 280 nits 白点 → 3.5 倍
        assert!((scrgb_scale(280.0) - 3.5).abs() < 1e-6);
        // 异常输入不会产生除零
        assert!(scrgb_scale(0.0) > 0.0);
    }

    /// SDR 白点内容必须精确落在 [0,1]，不能被 HDR 映射改动。
    #[test]
    fn sdr_white_maps_to_full_white() {
        // scRGB 值 = 3.5 对应 SDR 白点（280 nits）
        let scale = scrgb_scale(280.0);
        let px = (scale as f32).to_half_bits();
        let mut src = Vec::new();
        for _ in 0..3 {
            src.extend_from_slice(&px.to_le_bytes());
        }
        src.extend_from_slice(&0u16.to_le_bytes()); // alpha
        let out = tonemap(&src, 8, 1, 1, 280.0);
        assert_eq!(&out[..3], &[255, 255, 255], "SDR 白应编码为纯白");
        assert_eq!(out[3], 255);
    }

    /// 超过 SDR 白点的 HDR 高光压到白，不溢出。
    #[test]
    fn hdr_highlight_clips_to_white() {
        let scale = scrgb_scale(280.0);
        // 8 倍 SDR 白点（约 2240 nits）
        let px = ((scale * 8.0) as f32).to_half_bits();
        let mut src = Vec::new();
        for _ in 0..3 {
            src.extend_from_slice(&px.to_le_bytes());
        }
        src.extend_from_slice(&0u16.to_le_bytes());
        let out = tonemap(&src, 8, 1, 1, 280.0);
        assert_eq!(&out[..3], &[255, 255, 255]);
    }

    /// 黑色保持为黑。
    #[test]
    fn black_stays_black() {
        let src = vec![0u8; 8];
        let out = tonemap(&src, 8, 1, 1, 280.0);
        assert_eq!(&out[..3], &[0, 0, 0]);
        assert_eq!(out[3], 255);
    }

    /// 行 pitch 带填充时不能读串行（填充字节不进输出）。
    #[test]
    fn row_pitch_padding_is_skipped() {
        let scale = scrgb_scale(280.0);
        let white = (scale as f32).to_half_bits();
        // 每行 2 像素有效（16 字节），pitch 24 字节（8 字节填充填 0xFF）
        let mut src = Vec::new();
        for _ in 0..2 {
            for _ in 0..2 {
                for _ in 0..3 {
                    src.extend_from_slice(&white.to_le_bytes());
                }
                src.extend_from_slice(&0u16.to_le_bytes());
            }
            src.extend_from_slice(&[0xFF; 8]);
        }
        let out = tonemap(&src, 24, 2, 2, 280.0);
        assert_eq!(out.len(), 2 * 2 * 4);
        for (i, px) in out.chunks_exact(4).enumerate() {
            assert_eq!(&px[..3], &[255, 255, 255], "第 {i} 个像素被 padding 污染");
        }
    }

    #[test]
    fn bgra_conversion_swaps_and_forces_opaque() {
        let src = vec![1u8, 2, 3, 0, 4, 5, 6, 0];
        let out = bgra_to_rgba(&src, 8, 2, 1);
        assert_eq!(out, vec![3u8, 2, 1, 255, 6, 5, 4, 255]);
    }

    /// f32 → half 的测试辅助（只覆盖测试用到的非负有限值）。
    trait ToHalfBits {
        fn to_half_bits(self) -> u16;
    }

    impl ToHalfBits for f32 {
        fn to_half_bits(self) -> u16 {
            let v = self;
            if v <= 0.0 || !v.is_finite() {
                return 0;
            }
            let bits = v.to_bits();
            let exp = ((bits >> 23) & 0xFF) as i32 - 127;
            let mant = bits & 0x7F_FFFF;
            if exp > 15 {
                return 0x7C00; // 溢出为 Inf（调用方会归零）
            }
            if exp < -14 {
                // 次正规数
                let shift = (-14 - exp) as u32;
                let m = (mant | 0x80_0000) >> (13 + shift);
                return m as u16;
            }
            (((exp + 15) as u32) << 10 | (mant >> 13)) as u16
        }
    }
}
