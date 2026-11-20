// SPDX-License-Identifier: GPL-3.0-only
//! 条带拼接（LNG-4）：用「上一帧底部/顶部条带」在「新帧」中做行哈希匹配，求纵向像素偏移，
//! 定位重叠区并去除重复。纯函数、无窗口依赖，便于单测覆盖多种滚动场景。
//!
//! 术语：`offset` > 0 = 内容上移（向下滚动，LNG-2）；< 0 = 内容下移（向上滚动，LNG-10）。
//! 固定页眉/页脚（LNG-5）由调用方跨帧检测后传入，定位时从模板与新增区间中排除。
//!
//! **容错（issue #157 / #196）**：整行一个哈希要求像素完全一致，视口里只要有轮播图 / GIF /
//! 闪烁光标 / 进度条，这些行就整行不匹配；阈值又硬，动画占内容区一成以上就必然定位失败。
//! 现在每行切成 8 段各算哈希，**8 段里有 5 段一致就算这行相同**；计分只数「与上一行不同」
//! 的有信息量行（空白 / 纯色行不灌水）；所有候选位移都算分取最高（平手取更小位移），
//! 不再「第一个达标就停」。
//!
//! 帧签名可缓存（`Signs`）：调用方只需每拍算新帧，上一帧的签名直接复用。

/// 每行切成的分段数。动画只占行的一部分宽度时，其余分段仍可匹配。
const SEGS: usize = 8;
/// 一行里至少多少段一致才算「这行相同」。
const SEG_MIN_EQ: usize = 5;
/// 候选位移至少要有这么多「有信息量行」参与计分，否则不可信。
const MIN_INFO_ROWS: u32 = 6;
/// 分数差在此范围内视为平手，平手取更小位移（更保守）。
const TIE_EPS: f32 = 0.02;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// 行签名：分段哈希 + 是否「有信息量」。
#[derive(Clone, Copy)]
struct RowSig {
    seg: [u64; SEGS],
    /// 与上一行签名不同才算有信息量（空白 / 纯色 / 重复行不计分，避免灌水）。
    info: bool,
}

fn seg_hash(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    // 每 2 个像素抽 1 个：240px 的分段仍有 120 个采样点，足够区分行，成本减半。
    for c in bytes.chunks(8).step_by(2) {
        let v = match *c {
            [c0, c1, c2, c3, c4, c5, c6, c7] => u64::from_le_bytes([c0, c1, c2, c3, c4, c5, c6, c7]),
            ref tail => tail
                .iter()
                .enumerate()
                .fold(0u64, |v, (i, &b)| v | (b as u64) << (8 * i)),
        };
        h = (h ^ v).wrapping_mul(FNV_PRIME);
    }
    h
}

/// 计算每行签名（长度 = 行数）。
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

/// 两行是否「基本一致」：足够多的分段相同即可（容忍局部动画）。
#[inline]
fn sig_match(a: &RowSig, b: &RowSig) -> bool {
    let mut eq = 0;
    for k in 0..SEGS {
        if a.seg[k] == b.seg[k] {
            eq += 1;
        }
    }
    eq >= SEG_MIN_EQ
}

/// 一帧的行签名。调用方可缓存上一帧的签名，每拍只算新帧。
#[derive(Default)]
pub struct Signs {
    sigs: Vec<RowSig>,
}

impl Signs {
    pub fn new(frame: &[u8], w: u32, h: u32) -> Self {
        Self {
            sigs: row_sigs(frame, w, h),
        }
    }

    /// 诊断：与另一帧「同位置行不一致」的占比。
    /// 1.0 = 逐行都变了，0.0 = 画面完全没变（用它区分「真没动」与「抓到了旧帧」）。
    pub fn changed_ratio(&self, other: &Signs, h: u32) -> f32 {
        let h = h as usize;
        if self.sigs.len() < h || other.sigs.len() < h || h == 0 {
            return 0.0;
        }
        let diff = (0..h).filter(|&i| !sig_match(&self.sigs[i], &other.sigs[i])).count();
        diff as f32 / h as f32
    }

    /// 诊断：有信息量行数（分数分母，太小说明页面几乎没内容）。
    pub fn info_rows(&self, h: u32) -> u32 {
        let h = (h as usize).min(self.sigs.len());
        self.sigs[..h].iter().filter(|s| s.info).count() as u32
    }
}

/// 条带匹配参数。
#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// 判定「重叠区一致」的最低得分（有信息量行中匹配的占比，0.0~1.0）。
    pub min_ratio: f32,
    /// 固定页眉 / 页脚的最小高度（像素），低于此值不认定为固定区域。
    pub min_fixed: u32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            min_ratio: 0.6,
            min_fixed: 8,
        }
    }
}

/// 拼接结果。
#[derive(Debug, Clone, Copy)]
pub struct Match {
    /// 新帧相对上一帧的滚动像素数：>0 内容上移（向下滚动），<0 内容下移（向上滚动），0 无位移。
    pub offset: i32,
    /// 新帧中应拼入画布的行区间 `[start, end)`（全天坐标，已排除固定区域与重叠部分）。
    pub start: u32,
    /// 行区间结束（不含）。
    pub end: u32,
    /// 定位是否可信（匹配行占比达标）。
    pub confident: bool,
}

/// 在两帧间定位滚动偏移：对 `s=0` 与两个方向的所有候选位移计分，取得分最高者
/// （分数平手取更小位移）。
///
/// 相比「第一个达标就停」，局部动画只会拉低所有候选的分数，不会让真实位移落选；
/// 只给有信息量行计分，又避免大片空白把错误位移顶上来。
///
/// `fixed_top` / `fixed_bottom` 为已确认的固定页眉/页脚高度，匹配只在内容区进行。
pub fn locate(prev: &Signs, new: &Signs, h: u32, fixed_top: u32, fixed_bottom: u32, params: &Params) -> Match {
    let (ps, ns): (&[RowSig], &[RowSig]) = (&prev.sigs, &new.sigs);
    // 帧尺寸对不上（未 seed / 尺寸变了）：不做定位，交给调用方当作失败。
    if ps.len() < h as usize || ns.len() < h as usize {
        return no_match(h);
    }
    let ct = fixed_top;
    let content_h = h.saturating_sub(fixed_top + fixed_bottom);
    // 内容区太小则不做拼接（避免零碎结果）。
    if content_h < 8 {
        return no_match(h);
    }
    // 至少保留约 1/5 内容区作重叠区，允许用户滚得快一点。
    let min_overlap = (content_h / 5).max(8);
    let max_shift = content_h.saturating_sub(min_overlap).max(1);
    // 有信息量行的前缀和：每个候选位移都能 O(1) 拿到分母。
    let pre = info_prefix(ps);

    // best = (得分, 位移, 是否向上)；need = 还在被接受的分数下限（用于提前放弃候选）。
    let mut best: Option<(f32, u32, bool)> = None;
    let mut need = params.min_ratio;
    // `s=0` 取**真实分数**（need=0 关掉剪枝）：只把「有信息量行不够」当作 Page 空白，
    // 不能把「s=0 得分低」也当成空白页 —— 否则大面积背景的页面（论坛帖、白底文档）
    // 会在这条分支里被判成「没滚」，后面的候选位移一个都不会试（issue #196 复现）。
    match score(ps, ns, &pre, ct, content_h, 0, false, 0.0) {
        Some(sc) => best = Some((sc, 0, false)),
        None => {
            // 有信息量行不足（空白页 / 纯色画面）：退回全行比对，只用来判断「是否静止」。
            let eq = (0..content_h)
                .filter(|i| sig_match(&ps[(ct + i) as usize], &ns[(ct + i) as usize]))
                .count() as u32;
            if eq as f32 / content_h as f32 >= params.min_ratio {
                return still(h);
            }
        }
    }
    for up in [false, true] {
        for s in 1..=max_shift {
            let Some(sc) = score(ps, ns, &pre, ct, content_h, s, up, need) else {
                continue;
            };
            if best.is_none_or(|(b, _, _)| sc > b + TIE_EPS) {
                best = Some((sc, s, up));
                need = (sc + TIE_EPS).min(1.0);
            }
        }
    }

    match best {
        Some((sc, _, _)) if sc < params.min_ratio => no_match(h),
        Some((_, 0, _)) => still(h),
        Some((_, s, up)) => {
            if up {
                up_match(ct, s)
            } else {
                down_match(ct, content_h, s)
            }
        }
        None => no_match(h),
    }
}

/// 有信息量行的前缀和（长度 = 行数 + 1）。
fn info_prefix(sigs: &[RowSig]) -> Vec<u32> {
    let mut pre = Vec::with_capacity(sigs.len() + 1);
    pre.push(0);
    for s in sigs {
        let last = pre[pre.len() - 1];
        pre.push(last + u32::from(s.info));
    }
    pre
}

/// 对位移 `s` 计分：返回「有信息量行中匹配的占比」。
/// 有信息量行太少、或剩余行全匹配也达不到 `need` 时返回 `None`（不可信）。
#[allow(clippy::too_many_arguments)]
fn score(ps: &[RowSig], ns: &[RowSig], pre: &[u32], ct: u32, ch: u32, s: u32, up: bool, need: f32) -> Option<f32> {
    let overlap = ch - s;
    // 向下滚：prev[ct+s+i] vs new[ct+i]；向上滚：prev[ct+i] vs new[ct+s+i]。
    let (a0, b0) = if up { (ct, ct + s) } else { (ct + s, ct) };
    // 分母 = 窗口内有信息量的行数，O(1) 取前缀和（逐候选重扫会让静止帧白烧一遍全屏）。
    let total = pre[(a0 + overlap) as usize] - pre[a0 as usize];
    if total < MIN_INFO_ROWS {
        return None;
    }
    let denom = total as f32;
    let mut matched = 0u32;
    let mut seen = 0u32;
    for i in 0..overlap {
        let a = &ps[(a0 + i) as usize];
        if !a.info {
            continue;
        }
        seen += 1;
        if sig_match(a, &ns[(b0 + i) as usize]) {
            matched += 1;
        } else if ((matched + (total - seen)) as f32) < need * denom {
            // 剩余行全匹配也达不到要求：提前放弃这个候选位移。
            return None;
        }
    }
    Some(matched as f32 / denom)
}

fn no_match(h: u32) -> Match {
    Match {
        offset: 0,
        start: h,
        end: h,
        confident: false,
    }
}

fn still(h: u32) -> Match {
    Match {
        offset: 0,
        start: h,
        end: h,
        confident: true,
    }
}

fn down_match(ct: u32, content_h: u32, s: u32) -> Match {
    Match {
        offset: s as i32,
        start: ct + content_h - s,
        end: ct + content_h,
        confident: true,
    }
}

fn up_match(ct: u32, s: u32) -> Match {
    Match {
        offset: -(s as i32),
        start: ct,
        end: ct + s,
        confident: true,
    }
}

/// 遮掉的列至少多宽（像素）：再窄就是噪点，不值得擦。
const MIN_STATIC_W: u32 = 8;

/// 检测「滚动时不动」的列（左右两侧的粘滞侧栏、滚动条）：返回（起列, 止列, 填充色）。
///
/// 与 `fixed_regions` 同思路，只是方向换成列：页面滚了、这一列却逐行跟上一帧一模一样，
/// 就说明它是 `position: fixed` 的侧栏。遮掉后粘滞内容只在结果顶部出现一次，
/// 而不是每拼一段就重复一遍（论坛的话题侧栏、时间轴、聊天窗的左右栏都是这个形态）。
///
/// **纯色列不遮**：白底正文里的白列、侧栏的空白边材在后面的段落里可能有真实内容
/// （宽图片、表格），遮了会误删。
///
/// `top` / `bottom` 为已确认的固定页眉/页脚高度，检测只在其间进行。
pub fn static_columns(prev: &[u8], new: &[u8], w: u32, h: u32, top: u32, bottom: u32) -> Vec<(u32, u32, [u8; 4])> {
    let stride = w as usize * 4;
    let y0 = top as usize;
    let y1 = h.saturating_sub(bottom) as usize;
    if w == 0 || y1 <= y0 || prev.len() < stride * h as usize || new.len() < stride * h as usize {
        return Vec::new();
    }
    let rows = (y1 - y0) as u32;
    // 允许极少量行变化（侧栏里的瞬时提示、光标），但不能多到把正文也算进来：
    // 真滚过的正文列是「逐行都不一样」，粘滞列是「一行都不变」，中间地带按噪声处理。
    let tol = (rows / 50).max(2);
    // 逐行扫描（缓存友好）：累计每列有多少行变了，并记住这列是否有颜色变化。
    // 「纯色列」不能遮 —— 白底正文里的白列、侧栏的空白边材都不该被当成粘滞内容，
    // 而且它们往往在后面的段落里才出现真实内容（图片、宽表格），遮了会误删。
    let mut changed = vec![0u32; w as usize];
    let mut varied = vec![false; w as usize];
    let mut seed = vec![[0u8; 4]; w as usize];
    for y in y0..y1 {
        let a = &prev[y * stride..(y + 1) * stride];
        let b = &new[y * stride..(y + 1) * stride];
        for (i, ch) in changed.iter_mut().enumerate() {
            let o = i * 4;
            if a[o..o + 4] != b[o..o + 4] {
                *ch += 1;
            }
            if !varied[i] {
                let px = [b[o], b[o + 1], b[o + 2], b[o + 3]];
                if y == y0 {
                    seed[i] = px;
                } else if px != seed[i] {
                    varied[i] = true;
                }
            }
        }
    }
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut run: Option<u32> = None;
    for (x, ch) in changed.iter().enumerate() {
        let x = x as u32;
        if *ch <= tol && varied[x as usize] {
            run.get_or_insert(x);
        } else if let Some(a) = run.take()
            && x - a >= MIN_STATIC_W
        {
            out.push((a, x));
        }
    }
    if let Some(a) = run
        && w - a >= MIN_STATIC_W
    {
        out.push((a, w));
    }
    // 遮得太多说明这页不是「侧栏 + 正文」结构（或检测不准），宁可不遮。
    let masked: u32 = out.iter().map(|(a, b)| b - a).sum();
    if masked * 2 > w {
        return Vec::new();
    }
    out.into_iter()
        .map(|(a, b)| (a, b, run_color(new, w, y0, y1, a, b)))
        .collect()
}

/// 该列区间的代表色（出现次数最多的颜色，最多跟踪 8 种）—— 用它填平，下面几屏才像
/// 「侧栏还在、只是空了」；颜色太杂（图片 / 渐变侧栏）时借右侧相邻列的像素。
fn run_color(data: &[u8], w: u32, y0: usize, y1: usize, a: u32, b: u32) -> [u8; 4] {
    const TRACK: usize = 8;
    let stride = w as usize * 4;
    let mut colors = [[0u8; 4]; TRACK];
    let mut counts = [0u32; TRACK];
    let mut n = 0usize;
    for y in y0..y1 {
        let row = &data[y * stride..(y + 1) * stride];
        for x in a..b {
            let o = x as usize * 4;
            let c = [row[o], row[o + 1], row[o + 2], row[o + 3]];
            match colors[..n].iter().position(|k| *k == c) {
                Some(i) => counts[i] += 1,
                None if n < TRACK => {
                    colors[n] = c;
                    counts[n] = 1;
                    n += 1;
                }
                None => return neighbor_color(data, w, y0, a, b),
            }
        }
    }
    if n == 0 {
        return neighbor_color(data, w, y0, a, b);
    }
    let mut best = 0;
    for i in 1..n {
        if counts[i] > counts[best] {
            best = i;
        }
    }
    colors[best]
}

fn neighbor_color(data: &[u8], w: u32, y: usize, a: u32, b: u32) -> [u8; 4] {
    let x = if b < w { b } else { a.saturating_sub(1) };
    let o = y * w as usize * 4 + x as usize * 4;
    match data.get(o..o + 4) {
        Some(px) => [px[0], px[1], px[2], px[3]],
        None => [0, 0, 0, 255],
    }
}

/// 检测帧顶部 / 底部的「固定区域」高度（LNG-5 页眉 / 页脚）：
/// 与上一帧逐行比较，返回（顶部静止行数, 底部静止行数）。
/// 静止行可能是页面固定头部 / 底部工具栏；拆分时据此避免重复拼入。
/// 逐行用 `sig_match` 比较，页眉里有小动画（logo 旋转、时间跳动）也能识别。
pub fn fixed_regions(prev: &Signs, new: &Signs, h: u32, params: &Params) -> (u32, u32) {
    let (ps, ns): (&[RowSig], &[RowSig]) = (&prev.sigs, &new.sigs);
    let h = h as usize;
    if ps.len() < h || ns.len() < h {
        return (0, 0);
    }
    let mut top = 0usize;
    while top < h && sig_match(&ps[top], &ns[top]) {
        top += 1;
    }
    if (top as u32) < params.min_fixed {
        top = 0;
    }
    let mut bottom = 0usize;
    while bottom < h && sig_match(&ps[h - 1 - bottom], &ns[h - 1 - bottom]) {
        bottom += 1;
    }
    // 顶部与底部不允许重叠。
    if bottom + top > h {
        bottom = h - top;
    }
    if (bottom as u32) < params.min_fixed {
        bottom = 0;
    }
    (top as u32, bottom as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 逐帧字节接口（测试用）：内部先把两帧算成签名。
    fn locate(prev: &[u8], new: &[u8], w: u32, h: u32, fixed_top: u32, fixed_bottom: u32, params: &Params) -> Match {
        let (p, n) = (Signs::new(prev, w, h), Signs::new(new, w, h));
        super::locate(&p, &n, h, fixed_top, fixed_bottom, params)
    }

    fn fixed_regions(prev: &[u8], new: &[u8], w: u32, h: u32, params: &Params) -> (u32, u32) {
        let (p, n) = (Signs::new(prev, w, h), Signs::new(new, w, h));
        super::fixed_regions(&p, &n, h, params)
    }

    /// 造一页：每行按行号着色（R=低8位、G=高8位、B=横向渐变保证行内变化），行间可区分。
    fn make_page(w: u32, page_h: u32) -> Vec<u8> {
        let mut data = vec![0u8; (w * page_h * 4) as usize];
        for y in 0..page_h {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                data[o] = (y & 0xff) as u8;
                data[o + 1] = ((y >> 8) & 0xff) as u8;
                data[o + 2] = ((y * 7 + x * 13) & 0xff) as u8;
                data[o + 3] = 255;
            }
        }
        data
    }

    /// 截取页面的视口窗口 [y0, y0+h)。
    fn viewport(page: &[u8], w: u32, y0: u32, h: u32) -> Vec<u8> {
        let stride = w as usize * 4;
        page[y0 as usize * stride..(y0 + h) as usize * stride].to_vec()
    }

    fn params() -> Params {
        Params {
            min_ratio: 0.6,
            min_fixed: 8,
        }
    }

    #[test]
    fn locate_down_scroll_finds_offset() {
        let w = 64;
        let h = 60u32;
        let page = make_page(w, 400);
        let prev = viewport(&page, w, 0, h);
        let new = viewport(&page, w, 10, h);
        let m = locate(&prev, &new, w, h, 0, 0, &params());
        assert!(m.confident);
        assert_eq!(m.offset, 10);
        // 新增内容为 new 底部 [50,60) = page[60..70)
        assert_eq!(m.start, 50);
        assert_eq!(m.end, 60);
    }

    #[test]
    fn locate_up_scroll_finds_offset() {
        let w = 64;
        let h = 60u32;
        let page = make_page(w, 400);
        let prev = viewport(&page, w, 60, h);
        let new = viewport(&page, w, 50, h);
        let m = locate(&prev, &new, w, h, 0, 0, &params());
        assert!(m.confident);
        assert_eq!(m.offset, -10);
        // 新增内容为 new 顶部 [0,10) = page[50..60)
        assert_eq!(m.start, 0);
        assert_eq!(m.end, 10);
    }

    #[test]
    fn locate_no_move_returns_zero() {
        let w = 64;
        let h = 60u32;
        let page = make_page(w, 200);
        let f = viewport(&page, w, 20, h);
        let m = locate(&f, &f, w, h, 0, 0, &params());
        assert!(m.confident);
        assert_eq!(m.offset, 0);
        assert_eq!(m.start, h);
        assert_eq!(m.end, h);
    }

    /// 纯色画面（全帧同样）：没有「有信息量行」，仍应判定为静止而不是失败。
    #[test]
    fn locate_uniform_page_is_still() {
        let (w, h) = (64u32, 60u32);
        let f = vec![240u8; (w * h * 4) as usize];
        let m = locate(&f, &f, w, h, 0, 0, &params());
        assert!(m.confident, "纯色画面不该报定位失败");
        assert_eq!(m.offset, 0);
    }

    /// 局部动画（issue #157）：行的一部分在变、外加整行横幅动画，定位仍应成功。
    #[test]
    fn locate_tolerates_animated_region() {
        let (w, h) = (64u32, 100u32);
        let page = make_page(w, 500);
        let prev = viewport(&page, w, 0, h);
        let mut new = viewport(&page, w, 15, h);
        // 行 20..70 的左侧 8..32 列在变（只影响 3/8 的分段）
        for y in 20..70u32 {
            for x in 8..32u32 {
                let o = ((y * w + x) * 4) as usize;
                new[o] ^= 0x55;
                new[o + 1] ^= 0x33;
            }
        }
        // 再加 10 行整行都在变（模拟横幅动画）
        for y in 80..90u32 {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                new[o] ^= 0x77;
            }
        }
        let m = locate(&prev, &new, w, h, 0, 0, &params());
        assert!(m.confident, "局部动画不应导致定位失败");
        assert_eq!(m.offset, 15);
    }

    /// 滚动快一些（超过内容区一半、但仍在 1/5 重叠区内）也要能定位。
    #[test]
    fn locate_allows_fast_scroll() {
        let (w, h) = (64u32, 100u32);
        let page = make_page(w, 500);
        let prev = viewport(&page, w, 0, h);
        let new = viewport(&page, w, 70, h);
        let m = locate(&prev, &new, w, h, 0, 0, &params());
        assert!(m.confident);
        assert_eq!(m.offset, 70);
    }

    /// 大面积背景的页面（论坛帖 / 白底文档）真实滚动了，不能因为「s=0 得分低」
    /// 而走「空白页」兼底分支被判成静止（issue #196 实测原因）。
    #[test]
    fn locate_scrolls_page_with_large_blank_background() {
        let (w, h) = (64u32, 120u32);
        let page_h = 400u32;
        // 每 4 行 = 3 行纯白背景 + 1 行「文字」，行内容按索引区分
        let mut page = vec![250u8; (w * page_h * 4) as usize];
        for y in (3..page_h).step_by(4) {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                page[o] = (y & 0xff) as u8;
                page[o + 1] = ((y >> 8) & 0xff) as u8;
                page[o + 2] = (x * 9) as u8;
            }
        }
        let prev = viewport(&page, w, 0, h);
        // 向下滚 60 行（4 的倍数，背景行仍能对齐）
        let new = viewport(&page, w, 60, h);
        let m = locate(&prev, &new, w, h, 0, 0, &params());
        assert!(m.confident, "大面积背景页面的真实滚动不应判失败");
        assert_eq!(m.offset, 60, "应找出真实位移，而不是判「没动」");
    }

    /// 粘滞侧栏：页面滚了、左侧几列却逐行没变（且有字）→ 应被识别为可遮掉的静态列。
    #[test]
    fn static_columns_finds_sticky_sidebar() {
        let (w, h) = (64u32, 100u32);
        let page = make_page(w, 260);
        let rail = 16u32;
        let frame = |off: u32| -> Vec<u8> {
            let mut f = viewport(&page, w, off, h);
            for y in 0..h {
                for x in 0..rail {
                    let o = ((y * w + x) * 4) as usize;
                    // 背景 + 隔几行一行「文字」，模拟侧栏的菜单项
                    let v = if y % 10 == 0 && (2..12).contains(&x) { 20 } else { 200 };
                    f[o] = v;
                    f[o + 1] = v;
                    f[o + 2] = v;
                    f[o + 3] = 255;
                }
            }
            f
        };
        let masks = static_columns(&frame(0), &frame(60), w, h, 0, 0);
        assert_eq!(masks.len(), 1, "应只识别出一段粘滞列，实际 {masks:?}");
        assert_eq!((masks[0].0, masks[0].1), (2, 12), "只遮有文字的列");
    }

    /// 纯色列（白底正文里的白列、侧栏空白边材）不遮：后面段落里可能有真实内容。
    #[test]
    fn static_columns_ignores_uniform_columns() {
        let (w, h) = (64u32, 100u32);
        let page = make_page(w, 260);
        let frame = |off: u32| -> Vec<u8> {
            let mut f = viewport(&page, w, off, h);
            for y in 0..h {
                for x in 0..16u32 {
                    let o = ((y * w + x) * 4) as usize;
                    f[o] = 200;
                    f[o + 1] = 200;
                    f[o + 2] = 200;
                    f[o + 3] = 255;
                }
            }
            f
        };
        assert!(
            static_columns(&frame(0), &frame(60), w, h, 0, 0).is_empty(),
            "纯色列不该被当成粘滞内容"
        );
    }

    /// 完全静止的一对帧（根本没滚）：不能把整屏当成「静态列」遮掉。
    #[test]
    fn static_columns_ignores_unscrolled_pair() {
        let (w, h) = (64u32, 100u32);
        let page = make_page(w, 140);
        let f = viewport(&page, w, 0, h);
        assert!(static_columns(&f, &f, w, h, 0, 0).is_empty(), "整帧没变时不应遮任何列");
    }

    #[test]
    fn fixed_regions_detects_header_footer() {
        let w = 64;
        let h = 80u32;
        // 页面：10 行固定页眉(值=900)、60 行内容、10 行固定页脚(值=901)
        let mut page = vec![0u8; (w * h * 4) as usize];
        for y in 0..10 {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                page[o] = 200;
                page[o + 1] = 201;
                page[o + 2] = (x * 5) as u8;
                page[o + 3] = 255;
            }
        }
        for y in 10..70 {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                page[o] = (y) as u8;
                page[o + 1] = (y >> 8) as u8;
                page[o + 2] = (x * 3) as u8;
                page[o + 3] = 255;
            }
        }
        for y in 70..80 {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                page[o] = 202;
                page[o + 1] = 203;
                page[o + 2] = (x * 7) as u8;
                page[o + 3] = 255;
            }
        }
        // 第二帧整体下移 5 行（但页眉/页脚保留在原位）
        let mut frame2 = page.clone();
        for y in 10..70 {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                frame2[o] = (y + 5) as u8;
                frame2[o + 1] = ((y + 5) >> 8) as u8;
                frame2[o + 2] = (x * 3) as u8;
                frame2[o + 3] = 255;
            }
        }
        let (t, b) = fixed_regions(&page, &frame2, w, h, &params());
        assert_eq!(t, 10);
        assert_eq!(b, 10);
    }

    /// 页眉里有一小块动画（logo 旋转）：不应因此认为页眉不固定。
    #[test]
    fn fixed_regions_tolerates_header_animation() {
        let w = 64;
        let h = 80u32;
        let hdr = 10u32;
        // 帧内容：页眉恒定（行内横向渐变）、内容行按「内容行号」着色。
        let frame = |shift: u32| -> Vec<u8> {
            let mut data = vec![0u8; (w * h * 4) as usize];
            for y in 0..h {
                for x in 0..w {
                    let o = ((y * w + x) * 4) as usize;
                    if y < hdr {
                        data[o] = 200;
                        data[o + 1] = 201;
                        data[o + 2] = (x * 5) as u8;
                    } else {
                        let g = y - hdr + shift;
                        data[o] = (g & 0xff) as u8;
                        data[o + 1] = (g >> 8) as u8;
                        data[o + 2] = ((x * 3 + g * 7) & 0xff) as u8;
                    }
                    data[o + 3] = 255;
                }
            }
            data
        };
        let frame1 = frame(0);
        let mut frame2 = frame(5);
        // 页眉（行 0..10）左侧一小块在变：只影响 1 个分段
        for y in 2..8u32 {
            for x in 0..8u32 {
                let o = ((y * w + x) * 4) as usize;
                frame2[o] ^= 0xFF;
            }
        }
        let (t, _) = fixed_regions(&frame1, &frame2, w, h, &params());
        assert_eq!(t, hdr, "页眉里的小动画不应让页眉检测失效");
    }
}
