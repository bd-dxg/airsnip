// SPDX-License-Identifier: GPL-3.0-only
//! 区域抓帧（LNG-2/3，issue #128）：长截图逐帧采集视口。
//!
//! HDR 输出走**复用**的抓帧会话（`hdr::HdrRegionSession`），纯 SDR 走每帧 BitBlt。
//! 复用是为了避免 HDR 下每帧重建 DXGI —— 那会在 NVIDIA 驱动上累积内存泄漏。
//! 单次全屏抓屏（`capture_virtual_screen`）不受影响，仍是现建现用。
//!
//! HDR 后端通过 trait 注入，便于在无 HDR 环境下用 fake 覆盖状态机（见测试）。

use super::hdr::{self, GrabError};

/// HDR 抓帧后端：真实实现是 `hdr::HdrRegionSession`，测试注入 fake。
trait HdrGrabber {
    /// `need_frame` 为 true 时必须给出新帧；否则桌面无变化可返回 `Ok(None)`。
    fn grab(&mut self, need_frame: bool) -> Result<Option<Vec<u8>>, GrabError>;
}

impl HdrGrabber for hdr::HdrRegionSession {
    fn grab(&mut self, need_frame: bool) -> Result<Option<Vec<u8>>, GrabError> {
        hdr::HdrRegionSession::grab(self, need_frame)
    }
}

/// 建立 HDR 会话；`None` = 该区域不在 HDR 输出上（回退 BitBlt）。
type OpenHdr = Box<dyn Fn(i32, i32, u32, u32) -> Option<Result<Box<dyn HdrGrabber>, String>>>;
/// SDR 抓帧。
type BitBltFn = Box<dyn Fn(i32, i32, u32, u32) -> Result<Vec<u8>, String>>;

/// 区域抓帧器：跨帧复用 HDR 抓帧会话；SDR 时回退 BitBlt。
pub struct RegionCapture {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    /// HDR 会话；`None` 表示纯 SDR，走 BitBlt。
    hdr: Option<Box<dyn HdrGrabber>>,
    /// 建 / 重建 HDR 会话（测试注入 fake）。
    open: OpenHdr,
    /// SDR 抓帧（测试注入 fake）。
    bitblt: BitBltFn,
    /// 最近一帧（桌面无变化时沿用，让拼接看到「静止」）。
    last: Vec<u8>,
    /// 是否已取到过至少一帧（决定 HDR 会话能否把「无变化」当静止）。
    have_frame: bool,
}

impl RegionCapture {
    /// 建立区域抓帧器。HDR 会话建立失败时返回 `Err`（长截图会取消并提示）。
    pub fn new(x: i32, y: i32, w: u32, h: u32) -> Result<Self, String> {
        Self::with_backends(
            x,
            y,
            w,
            h,
            Box::new(|x, y, w, h| {
                hdr::HdrRegionSession::open(x, y, w, h).map(|r| r.map(|s| Box::new(s) as Box<dyn HdrGrabber>))
            }),
            Box::new(|x, y, w, h| Ok(super::capture_region_bitblt(x, y, w, h, false)?.rgba)),
        )
    }

    /// 用指定后端构造（测试注入 fake）。
    fn with_backends(x: i32, y: i32, w: u32, h: u32, open: OpenHdr, bitblt: BitBltFn) -> Result<Self, String> {
        let hdr = match open(x, y, w, h) {
            None => None, // 纯 SDR
            Some(Ok(s)) => Some(s),
            Some(Err(e)) => return Err(e),
        };
        Ok(Self {
            x,
            y,
            w,
            h,
            hdr,
            open,
            bitblt,
            last: Vec::new(),
            have_frame: false,
        })
    }

    /// 抓取当前区域帧，返回最近一帧像素（长度 = `w*h*4`）。
    pub fn grab(&mut self) -> Result<&[u8], String> {
        if self.hdr.is_none() {
            self.last = (self.bitblt)(self.x, self.y, self.w, self.h)?;
            return Ok(&self.last);
        }
        // 取出会话本体：重建前必须先释放旧 duplication（同一输出同时只能有一个会话）。
        let mut session = self.hdr.take().expect("hdr 非空");
        match session.grab(!self.have_frame) {
            Ok(Some(rgba)) => {
                self.last = rgba;
                self.have_frame = true;
                self.hdr = Some(session);
            }
            // 桌面无变化：沿用上一帧（拼接会看到位移 0 → 静止）。
            Ok(None) => self.hdr = Some(session),
            Err(GrabError::Lost) => {
                // 会话失效（分辨率切换 / 全屏 / 锁屏）：重建一次再继续。
                drop(session); // 先释放旧会话，否则新会话建不起来
                self.have_frame = false;
                let Some(rebuilt) = (self.open)(self.x, self.y, self.w, self.h) else {
                    // 已不再有 HDR 输出：本帧回退 BitBlt。
                    self.last = (self.bitblt)(self.x, self.y, self.w, self.h)?;
                    return Ok(&self.last);
                };
                let mut next = rebuilt?;
                // 重建后立刻补一帧，避免本拍无帧而中止长截图。
                match next.grab(true) {
                    Ok(Some(rgba)) => {
                        self.last = rgba;
                        self.have_frame = true;
                    }
                    Ok(None) => {}
                    Err(GrabError::Lost) => return Err("HDR 会话重建后再次失效".into()),
                    Err(GrabError::Other(e)) => return Err(e),
                }
                self.hdr = Some(next);
            }
            Err(GrabError::Other(e)) => return Err(e),
        }
        if self.last.is_empty() {
            return Err("HDR 抓屏会话未取到首帧".into());
        }
        Ok(&self.last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    const W: u32 = 4;
    const H: u32 = 4;

    fn buf(v: u8) -> Vec<u8> {
        vec![v; (W * H * 4) as usize]
    }

    /// fake HDR 会话：按脚本依次返回抓帧结果（用尽后返回 `Ok(None)`）。
    struct FakeHdr(VecDeque<Result<Option<Vec<u8>>, GrabError>>);
    impl HdrGrabber for FakeHdr {
        fn grab(&mut self, _need_frame: bool) -> Result<Option<Vec<u8>>, GrabError> {
            self.0.pop_front().unwrap_or(Ok(None))
        }
    }
    fn fake(script: Vec<Result<Option<Vec<u8>>, GrabError>>) -> Box<dyn HdrGrabber> {
        Box::new(FakeHdr(script.into()))
    }

    /// 按脚本依次返回 open 结果（用尽后返回 `None` = 无 HDR）。
    fn scripted_open(items: Vec<Option<Result<Box<dyn HdrGrabber>, String>>>) -> OpenHdr {
        let q = Rc::new(RefCell::new(VecDeque::from(items)));
        Box::new(move |_, _, _, _| q.borrow_mut().pop_front().unwrap_or(None))
    }

    fn const_bitblt(v: u8) -> BitBltFn {
        Box::new(move |_, _, w, h| Ok(vec![v; (w * h * 4) as usize]))
    }

    fn build(opens: Vec<Option<Result<Box<dyn HdrGrabber>, String>>>) -> Result<RegionCapture, String> {
        RegionCapture::with_backends(W as i32, H as i32, W, H, scripted_open(opens), const_bitblt(9))
    }

    /// 新帧 → 无变化沿用上一帧 → 下一新帧。
    #[test]
    fn hdr_new_frame_then_unchanged_reuses_last() {
        let mut rc = build(vec![Some(Ok(fake(vec![Ok(Some(buf(1))), Ok(None), Ok(Some(buf(2)))])))]).unwrap();
        assert_eq!(rc.grab().unwrap(), &buf(1)[..]);
        assert_eq!(rc.grab().unwrap(), &buf(1)[..]);
        assert_eq!(rc.grab().unwrap(), &buf(2)[..]);
    }

    /// 会话失效 → 重建 → 立即补一帧。
    #[test]
    fn hdr_lost_rebuilds_and_refetches() {
        let mut rc = build(vec![
            Some(Ok(fake(vec![Ok(Some(buf(1))), Err(GrabError::Lost)]))),
            Some(Ok(fake(vec![Ok(Some(buf(2)))]))),
        ])
        .unwrap();
        assert_eq!(rc.grab().unwrap(), &buf(1)[..]);
        assert_eq!(rc.grab().unwrap(), &buf(2)[..], "重建后应立即取到新帧");
    }

    /// 会话失效且不再有 HDR 输出 → 回退 BitBlt。
    #[test]
    fn hdr_lost_without_hdr_falls_back_to_bitblt() {
        let mut rc = build(vec![Some(Ok(fake(vec![Err(GrabError::Lost)]))), None]).unwrap();
        assert_eq!(rc.grab().unwrap(), &buf(9)[..]);
    }

    /// 纯 SDR：每拍都走 BitBlt。
    #[test]
    fn sdr_uses_bitblt_every_tick() {
        let mut rc = build(vec![None]).unwrap();
        assert_eq!(rc.grab().unwrap(), &buf(9)[..]);
        assert_eq!(rc.grab().unwrap(), &buf(9)[..]);
    }

    /// 首帧取不到有效画面 → 报错（不静默返回空帧）。
    #[test]
    fn hdr_first_frame_unavailable_is_error() {
        let mut rc = build(vec![Some(Ok(fake(vec![Ok(None)])))]).unwrap();
        assert!(rc.grab().is_err());
    }

    /// HDR 会话建立失败 → `new` 直接返回错误。
    #[test]
    fn hdr_open_error_propagates() {
        assert!(build(vec![Some(Err("DuplicateOutput1 占用".into()))]).is_err());
    }
}
