// SPDX-License-Identifier: GPL-3.0-only
//! HTTP 客户端（SYS-7）：用系统自带 WinHTTP，不引入 reqwest。
//!
//! 理由：reqwest 会带进 tokio 运行时，与「待机 0% CPU、常驻内存 ≤ 15MB」红线冲突，
//! 而这里只需要拿一份几百字节的 JSON 和一个安装包。
//!
//! 只支持 `https://`（更新链路全部是 https；不支持明文以免被降级劫持）。

use std::ffi::c_void;

use windows::Win32::Networking::WinHttp::*;
use windows::core::{PCWSTR, w};

/// 连接超时（毫秒）：镜像站挂掉时要快速失败，不能卡住检查流程。
const CONNECT_TIMEOUT_MS: i32 = 5_000;
/// 读取超时（毫秒）：单次 recv 的等待上限。
const RECEIVE_TIMEOUT_MS: i32 = 10_000;
/// 单次读取块大小（64 KB）。
const CHUNK: usize = 64 * 1024;

/// 会话/连接/请求句柄的 RAII 包装：出错路径上也要关闭，避免泄漏。
struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// 变宽字符串（WinHTTP 只接受 UTF-16）。
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// `https://host/path` → (host, path)。其它协议或格式返回 None。
fn split_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    if host.is_empty() {
        return None;
    }
    // 去掉端口（WinHttpConnect 需要单独传端口，这里不支持自定义端口）
    let host = host.split(':').next().unwrap_or(host).to_string();
    Some((host, path))
}

/// GET 请求，逐块回调 `on_chunk`（参数：数据块 / 已接收字节 / 响应总长度）。返回总字节数。
///
/// `on_chunk` 返回 Err 时中止（用于磁盘写入失败或校验失败）。
pub fn get<F>(url: &str, mut on_chunk: F) -> Result<usize, String>
where
    F: FnMut(&[u8], usize, Option<usize>) -> Result<(), String>,
{
    let (host, path) = split_url(url).ok_or_else(|| format!("不支持的下载地址：{url}"))?;
    let agent = wide("airsnip-updater");
    let host_w = wide(&host);
    let path_w = wide(&path);

    unsafe {
        let session = Handle(WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return Err("WinHttpOpen 失败".into());
        }
        let _ = set_timeout(session.0, WINHTTP_OPTION_CONNECT_TIMEOUT, CONNECT_TIMEOUT_MS);
        let _ = set_timeout(session.0, WINHTTP_OPTION_RECEIVE_TIMEOUT, RECEIVE_TIMEOUT_MS);

        let connect = Handle(WinHttpConnect(
            session.0,
            PCWSTR(host_w.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ));
        if connect.0.is_null() {
            return Err(format!("无法连接 {host}"));
        }
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err("WinHttpOpenRequest 失败".into());
        }
        // 不禁用重定向：GitHub 的 `releases/latest/download/...` 会 302 到带 tag 的地址。
        // WinHTTP 默认跟随跳转（最多 10 次），且不接受 HTTPS → HTTP 降级，足够安全。
        // 部分镜像对没有 Accept 的请求返回 403
        // 注意：这里不能带结尾的 0，wrapper 用 slice.len() 当长度传（多一个 0 会 ERROR_INVALID_PARAMETER）
        let headers: Vec<u16> = "Accept: */*\r\n".encode_utf16().collect();
        let ok = WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).is_ok();
        if !ok {
            let err = windows::Win32::Foundation::GetLastError();
            return Err(format!("请求 {host} 失败（错误码 {}）", err.0));
        }
        if WinHttpReceiveResponse(request.0, std::ptr::null_mut()).is_err() {
            let err = windows::Win32::Foundation::GetLastError();
            return Err(format!("{host} 未响应（错误码 {}）", err.0));
        }
        let status = status_code(request.0)?;
        if !(200..300).contains(&status) {
            return Err(format!("{host} 返回 HTTP {status}"));
        }
        let total = content_length(request.0);

        let mut received = 0usize;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let mut read = 0u32;
            if WinHttpReadData(request.0, buf.as_mut_ptr() as *mut c_void, CHUNK as u32, &mut read).is_err() {
                return Err("读取响应失败".into());
            }
            if read == 0 {
                break;
            }
            let n = read as usize;
            received += n;
            on_chunk(&buf[..n], received, total)?;
        }
        Ok(received)
    }
}

/// 设置超时选项。
unsafe fn set_timeout(handle: *mut c_void, option: u32, ms: i32) -> bool {
    let bytes = ms.to_le_bytes();
    unsafe { WinHttpSetOption(Some(handle), option, Some(&bytes)).is_ok() }
}

/// 读响应状态码。
fn status_code(request: *mut c_void) -> Result<u32, String> {
    let mut code = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut code as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(|e| format!("读取状态码失败：{e}"))?;
    }
    Ok(code)
}

/// 读响应总长度（`Content-Length`）；镜像未给或分块传输时返回 None。
fn content_length(request: *mut c_void) -> Option<usize> {
    let mut len = 0u64;
    let mut size = std::mem::size_of::<u64>() as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut len as *mut u64 as *mut c_void),
            &mut size,
            std::ptr::null_mut(),
        )
        .is_ok()
    };
    (ok && len > 0).then_some(len as usize)
}

/// 便捷封装：GET 到内存（清单才几百字节，直接收下）。
pub fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    get(url, |chunk, _, _| {
        out.extend_from_slice(chunk);
        // 清单不该超过 64 KB，异常响应直接拒绝（防被塞垃圾数据打爆内存）
        if out.len() > 64 * 1024 {
            return Err("响应过大，已中止".into());
        }
        Ok(())
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_https_urls() {
        assert_eq!(
            split_url("https://github.com/a/b.exe"),
            Some(("github.com".into(), "/a/b.exe".into()))
        );
        assert_eq!(
            split_url("https://example.com"),
            Some(("example.com".into(), "/".into()))
        );
        assert_eq!(split_url("http://example.com/x"), None, "只允许 https");
        assert_eq!(split_url("https:///x"), None, "缺 host 应拒绝");
    }

    #[test]
    fn strips_port_from_host() {
        let (host, path) = split_url("https://example.com:8443/a").expect("应能解析");
        assert_eq!(host, "example.com");
        assert_eq!(path, "/a");
    }

    /// 真实网络请求（默认忽略）：验证 WinHTTP 封装能跑通 TLS + 自动跟随 302。
    /// 手动跑：`cargo test update::http -- --ignored`
    #[test]
    #[ignore = "需要网络"]
    fn fetches_real_url_following_redirects() {
        // 该地址会 302 到具体提交，同时验证了重定向跟随
        let url = "https://raw.githubusercontent.com/google/material-design-icons/master/symbols/web/delete/materialsymbolsoutlined/delete_24px.svg";
        let bytes = get_bytes(url).expect("应能下载 SVG");
        assert!(bytes.len() > 100, "响应体过小：{}", bytes.len());
        assert!(String::from_utf8_lossy(&bytes).contains("<svg"));
    }
}
