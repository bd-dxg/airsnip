// SPDX-License-Identifier: GPL-3.0-only
//! 为更新清单生成签名（SYS-7）。发布脚本 `scripts/gen-latest-json.ps1` 调用它。
//!
//! 用法：
//! ```text
//! cargo run --example update-sign -- --version 0.7.0 --sha256 <安装包 sha256>
//! ```
//!
//! 私钥默认读 `%USERPROFILE%\.airsnip\update.key`，可用 `--key <路径>` 指定。
//! 输出即清单的 `signature` 字段（十六进制），签名载荷与客户端 `Manifest::payload()`
//! 完全一致（标签 + 版本 + 小写摘要），确保版本号被签名绑定、无法篡改。

use std::path::PathBuf;

use ed25519_dalek::{Signer, SigningKey};

const PAYLOAD_TAG: &str = "airsnip-update-v1";

fn main() {
    let mut version = String::new();
    let mut sha256 = String::new();
    let mut key_arg: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--version" => version = args.next().unwrap_or_default(),
            "--sha256" => sha256 = args.next().unwrap_or_default(),
            "--key" => key_arg = args.next().map(PathBuf::from),
            other => {
                eprintln!("未知参数：{other}");
                std::process::exit(2);
            }
        }
    }
    if version.is_empty() || sha256.is_empty() {
        eprintln!("用法：update-sign --version <版本> --sha256 <摘要> [--key <私钥路径>]");
        std::process::exit(2);
    }

    let key_path = key_arg.unwrap_or_else(|| {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".airsnip")
            .join("update.key")
    });
    let Ok(secret_hex) = std::fs::read_to_string(&key_path) else {
        eprintln!("读不到私钥：{}（先跑 update-keygen）", key_path.display());
        std::process::exit(1);
    };
    let Some(secret) = decode_hex(secret_hex.trim()) else {
        eprintln!("私钥格式非法（应为 64 位十六进制）");
        std::process::exit(1);
    };
    let Ok(seed) = <[u8; 32]>::try_from(secret.as_slice()) else {
        eprintln!("私钥长度应为 32 字节");
        std::process::exit(1);
    };

    let payload = format!("{PAYLOAD_TAG}\n{version}\n{}", sha256.to_ascii_lowercase());
    let sig = SigningKey::from_bytes(&seed).sign(payload.as_bytes());
    println!("{}", hex(&sig.to_bytes()));
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in s.as_bytes().chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi as u8) << 4 | lo as u8);
    }
    Some(out)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
