// SPDX-License-Identifier: GPL-3.0-only
//! 长截图帧对位移分析（诊断工具，非产品代码）。
//!
//! 用途：`longshot/stitch.rs` 判 `confident=false` 时，把落盘的 `-prev.png` / `-new.png`
//! 拿出来算一遍「每个候选位移的匹配率」，从而区分两种情况：
//!
//! - 有一个位移得分接近 1.0 → 算法没选到它（定位逻辑 bug）；
//! - 所有位移得分都很低 → 帧内容本身对不上（亚像素滚动 / 抓帧来源不同 / 画面被叠加）。
//!
//! 签名口径与 `stitch.rs` 保持一致（8 段 FNV、5 段一致算同行、info = 与上一行不同），
//! 否则算出来的数字没有对照意义。
//!
//! ```powershell
//! cargo run --example longshot-shift -- "$env:TEMP\airsnip\airsnip-longshot-failed-1-prev.png" `
//!     "$env:TEMP\airsnip\airsnip-longshot-failed-1-new.png"
//! ```

use std::fs::File;

const SEGS: usize = 8;
const SEG_MIN_EQ: usize = 5;
const MIN_INFO_ROWS: u32 = 6;

#[derive(Clone, Copy)]
struct RowSig {
    seg: [u64; SEGS],
    info: bool,
}

fn seg_hash(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for c in bytes.chunks(8).step_by(2) {
        let v = match *c {
            [c0, c1, c2, c3, c4, c5, c6, c7] => u64::from_le_bytes([c0, c1, c2, c3, c4, c5, c6, c7]),
            ref tail => tail
                .iter()
                .enumerate()
                .fold(0u64, |v, (i, &b)| v | (b as u64) << (8 * i)),
        };
        h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn row_sigs(frame: &[u8], w: u32, h: u32) -> Vec<RowSig> {
    let px = w as usize;
    let stride = px * 4;
    let mut out: Vec<RowSig> = Vec::with_capacity(h as usize);
    for y in 0..h as usize {
        let row = &frame[y * stride..(y + 1) * stride];
        let mut seg = [0u64; SEGS];
        for (k, s) in seg.iter_mut().enumerate() {
            let a = k * px / SEGS * 4;
            let b = (k + 1) * px / SEGS * 4;
            *s = seg_hash(&row[a..b]);
        }
        let info = out.last().is_none_or(|p: &RowSig| p.seg != seg);
        out.push(RowSig { seg, info });
    }
    out
}

fn sig_match(a: &RowSig, b: &RowSig) -> bool {
    (0..SEGS).filter(|&k| a.seg[k] == b.seg[k]).count() >= SEG_MIN_EQ
}

/// 与 `stitch::score` 同口径：分母只算 prev 侧「有信息量」的行。
fn score(ps: &[RowSig], ns: &[RowSig], h: u32, s: u32, up: bool) -> Option<f32> {
    let overlap = h - s;
    let (a0, b0) = if up { (0, s) } else { (s, 0) };
    let total = (a0..a0 + overlap).filter(|&i| ps[i as usize].info).count() as u32;
    if total < MIN_INFO_ROWS {
        return None;
    }
    let matched = (0..overlap)
        .filter(|&i| ps[(a0 + i) as usize].info && sig_match(&ps[(a0 + i) as usize], &ns[(b0 + i) as usize]))
        .count() as u32;
    Some(matched as f32 / total as f32)
}

fn load(path: &str) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).expect("打开 PNG 失败")));
    let mut reader = decoder.read_info().expect("解析 PNG 失败");
    let mut buf = vec![0; reader.output_buffer_size().expect("PNG 缓冲区大小未知")];
    let info = reader.next_frame(&mut buf).expect("解码 PNG 失败");
    assert_eq!(info.color_type, png::ColorType::Rgba, "只支持 RGBA PNG");
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("用法: longshot-shift <prev.png> <new.png>");
        std::process::exit(2);
    }
    let (prev, pw, ph) = load(&args[0]);
    let (new, nw, nh) = load(&args[1]);
    println!("prev={pw}x{ph}  new={nw}x{nh}");
    if pw != nw || ph != nh {
        println!("尺寸不同，算法在 `locate` 里会直接判 no_match。");
        return;
    }
    let (w, h) = (pw, ph);
    let ps = row_sigs(&prev, w, h);
    let ns = row_sigs(&new, w, h);

    let info_rows = ps.iter().filter(|s| s.info).count();
    println!("prev 有信息量行 {info_rows}/{h}");

    // 同位置不匹配的行里，像素到底差多少：小幅度（混合 / 抖动）还是大幅度（内容不同）。
    let mut diffs: Vec<f32> = Vec::new();
    for y in 0..h as usize {
        if sig_match(&ps[y], &ns[y]) {
            continue;
        }
        let row = y * w as usize * 4;
        let n = w as usize * 4;
        let a = &prev[row..row + n];
        let b = &new[row..row + n];
        let sum: u32 = a
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0.iter())
            .map(|(x, y)| {
                (u32::from(x[0].abs_diff(y[0])) + u32::from(x[1].abs_diff(y[1])) + u32::from(x[2].abs_diff(y[2]))) / 3
            })
            .sum();
        diffs.push(sum as f32 / w as f32);
    }
    if !diffs.is_empty() {
        diffs.sort_by(|a, b| a.total_cmp(b));
        let p = |q: f32| diffs[((diffs.len() as f32 - 1.0) * q) as usize];
        println!(
            "不匹配行 {} 行，每像素平均差 p50={:.1} p90={:.1} max={:.1}（0-255）",
            diffs.len(),
            p(0.5),
            p(0.9),
            diffs[diffs.len() - 1]
        );
    }

    let mut rows: Vec<(f32, u32, bool)> = Vec::new();
    println!(
        "s=0（同位置）匹配率: {:.3}",
        score(&ps, &ns, h, 0, false).unwrap_or(-1.0)
    );
    for up in [false, true] {
        for s in 1..h {
            if let Some(sc) = score(&ps, &ns, h, s, up) {
                rows.push((sc, s, up));
            }
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\n完全一致行配对（8 段全同，抽样每 40 行）：");
    for y in (0..h as usize).step_by(40) {
        let hit = (0..h as usize).find(|&y2| ps[y].seg == ns[y2].seg);
        match hit {
            Some(y2) => println!("  prev y={y:<4} → new y={y2:<4} （偏移 {:+}）", y2 as i64 - y as i64),
            None => println!("  prev y={y:<4} → 无完全一致行"),
        }
    }
    println!("\n得分最高的候选位移：");
    for (sc, s, up) in rows.iter().take(10) {
        let dir = if *up { "上滚" } else { "下滚" };
        // 同时给出该位移下「所有行」的一致占比，用于和日志里的 changed= 对照。
        let overlap = h - s;
        let all_same = (0..overlap)
            .filter(|&i| {
                let (a, b) = if *up { (i, i + s) } else { (i + s, i) };
                sig_match(&ps[a as usize], &ns[b as usize])
            })
            .count();
        println!(
            "{dir} s={s:<4} 匹配率={sc:.3}  全行一致率={:.1}%",
            all_same as f32 / overlap as f32 * 100.0
        );
    }
}
