// SPDX-License-Identifier: GPL-3.0-only
//! 生成更新签名密钥对（SYS-7）。
//!
//! 用法：`cargo run --example update-keygen`
//!
//! - 私钥写到 `%USERPROFILE%\.airsnip\update.key`（**不在仓库里**，不要提交、不要外传）；
//! - 公钥打印到标准输出，把它填进 `src/update/manifest.rs` 的 `UPDATE_PUBKEY_HEX`。
//!
//! 已存在私钥时默认不覆盖（防止误删导致已发布版本永远无法更新）；
//! 确实要换密钥时加 `--force`，并注意旧版本的客户端将无法再自动更新。

use std::path::PathBuf;

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;

fn main() {
    let force = std::env::args().any(|a| a == "--force");
    let dir = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".airsnip");
    let key_path = dir.join("update.key");

    if key_path.exists() && !force {
        eprintln!("私钥已存在：{}（换密钥请加 --force）", key_path.display());
        std::process::exit(1);
    }

    let key = SigningKey::generate(&mut OsRng);
    let pub_hex = hex(&key.verifying_key().to_bytes());
    let secret_hex = hex(&key.to_bytes());

    if std::fs::create_dir_all(&dir).is_err() || std::fs::write(&key_path, &secret_hex).is_err() {
        eprintln!("写入私钥失败：{}", key_path.display());
        std::process::exit(1);
    }

    println!("私钥：{}", key_path.display());
    println!("公钥：{pub_hex}");
    println!();
    println!("下一步：把上面的公钥填进 src/update/manifest.rs 的 UPDATE_PUBKEY_HEX");
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 保持 `VerifyingKey` 可见性（避免未使用导入警告）。
#[allow(dead_code)]
fn _assert_type(_: VerifyingKey) {}
