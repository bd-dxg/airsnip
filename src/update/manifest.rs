// SPDX-License-Identifier: GPL-3.0-only
//! 更新清单（SYS-7）：`latest.json` 的结构与签名校验。
//!
//! 防篡改靠**签名**而不是 hash：清单与安装包放在同一个 Release 里，能改包的人也能顺手
//! 改清单里的 sha256，两者一起改就校验不出来了。因此发布机用离线私钥对
//! `payload()` 产出的规范化文本签名，程序内置公钥验签，验签不过直接拒绝。
//!
//! 签名同时绑定版本号（防降级/拼接：把「新版本号 + 旧包」拼一起会验签失败）。

use ed25519_dalek::{Signature, VerifyingKey};

/// 签名载荷的格式版本前缀：将来改格式时一并改它，旧客户端会直接判定为无效。
const PAYLOAD_TAG: &str = "airsnip-update-v1";

/// 内置更新公钥（ed25519，32 字节，十六进制）。
///
/// 对应私钥**不在仓库里**，由发布机保存在 `%USERPROFILE%\.airsnip\update.key`。
/// 发版前用 `cargo run --example update-keygen` 生成一对新密钥并替换此处常量。
pub const UPDATE_PUBKEY_HEX: &str = "690b09dd387fc4de0162752046703d95581a4d4e305a91b54cc947ed39863901";

/// 更新清单。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Manifest {
    /// 新版本号（如 `0.7.0`）。
    pub version: String,
    /// 发布时间（ISO 8601，仅展示用，不参与验签语义）。
    #[serde(default)]
    pub pub_date: String,
    /// 更新说明（可选，展示在关于页）。
    #[serde(default)]
    pub notes: String,
    /// 安装包下载地址（GitHub 规范地址，加速前缀由客户端按可用镜像套用）。
    pub url: String,
    /// 安装包 SHA-256（小写十六进制）。
    pub sha256: String,
    /// 对 `payload()` 的 ed25519 签名（十六进制，128 字符）。
    pub signature: String,
}

impl Manifest {
    /// 被签名的规范化文本：标签 + 版本 + 摘要，一行一项。
    /// 版本参与签名，篡改版本号必须重新签名（否则防降级失效）。
    pub fn payload(&self) -> String {
        format!("{PAYLOAD_TAG}\n{}\n{}", self.version, self.sha256.to_ascii_lowercase())
    }

    /// 验签：公钥写死在程序里，清单来自网络也不影响安全性。
    pub fn verify(&self) -> Result<(), String> {
        self.verify_with(UPDATE_PUBKEY_HEX)
    }

    /// 指定公钥验签（单测用，便于注入测试密钥而不碰发布密钥）。
    pub fn verify_with(&self, pubkey_hex: &str) -> Result<(), String> {
        let key = decode_hex(pubkey_hex).ok_or("内置更新公钥格式非法")?;
        let key: [u8; 32] = key.as_slice().try_into().map_err(|_| "内置更新公钥长度非法")?;
        let vk = VerifyingKey::from_bytes(&key).map_err(|e| format!("更新公钥无效：{e}"))?;
        let sig = decode_hex(&self.signature).ok_or("清单签名不是合法十六进制")?;
        let sig: [u8; 64] = sig.as_slice().try_into().map_err(|_| "清单签名长度应为 64 字节")?;
        let sig = Signature::from_bytes(&sig);
        vk.verify_strict(self.payload().as_bytes(), &sig)
            .map_err(|_| "更新清单验签失败（可能被篡改，或公钥与本版本不匹配）".to_string())
    }
}

/// 十六进制 → 字节。非法字符或奇数长度返回 None。
pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi as u8) << 4 | lo as u8);
    }
    Some(out)
}

/// 字节 → 小写十六进制。
pub fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            version: "0.7.0".into(),
            pub_date: "2026-11-01T10:00:00Z".into(),
            notes: String::new(),
            url: "https://github.com/bd-dxg/airsnip/releases/download/v0.7.0/airsnip.exe".into(),
            sha256: "aBcD".into(),
            signature: String::new(),
        }
    }

    #[test]
    fn payload_binds_version_and_digest() {
        let m = manifest();
        assert_eq!(
            m.payload(),
            "airsnip-update-v1\n0.7.0\nabcd",
            "摘要应统一小写，版本必须进载荷"
        );
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(decode_hex("0"), None, "奇数长度应拒绝");
        assert_eq!(decode_hex("zz"), None, "非法字符应拒绝");
        assert_eq!(encode_hex(&[0x00, 0xff, 0x10]), "00ff10");
    }

    /// 验签：正确签名通过；改动版本、摘要或签名本身都会失败（防篡改 + 防降级拼接）。
    #[test]
    fn signature_detects_tampering() {
        use ed25519_dalek::{Signer, SigningKey};

        // 固定测试密钥（非发布密钥）：只用于验证逻辑，不涉及发布安全
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pub_hex = encode_hex(&sk.verifying_key().to_bytes());

        let mut m = manifest();
        m.signature = encode_hex(&sk.sign(m.payload().as_bytes()).to_bytes());
        assert!(m.verify_with(&pub_hex).is_ok(), "正确签名应通过");

        // 改版本号（把新版本号配旧包）
        let mut tampered = m.clone();
        tampered.version = "0.6.0".into();
        assert!(tampered.verify_with(&pub_hex).is_err(), "改版本号应验签失败");

        // 改摘要（换包）
        let mut tampered = m.clone();
        tampered.sha256 = "deadbeef".into();
        assert!(tampered.verify_with(&pub_hex).is_err(), "改摘要应验签失败");

        // 换签名
        let mut tampered = m.clone();
        tampered.signature = encode_hex(&[0u8; 64]);
        assert!(tampered.verify_with(&pub_hex).is_err(), "换签名应验签失败");
    }

    /// 内置公钥必须是合法的 32 字节 ed25519 公钥（占位全 0 会被这里拦住）。
    #[test]
    fn builtin_pubkey_is_valid() {
        let bytes = decode_hex(UPDATE_PUBKEY_HEX).expect("公钥应为十六进制");
        assert_eq!(bytes.len(), 32, "公钥应为 32 字节");
        assert!(
            VerifyingKey::from_bytes(&bytes.try_into().unwrap()).is_ok(),
            "公钥应能解析"
        );
    }

    /// 端到端：发布脚本生成的 `dist/latest.json` 必须能用内置公钥验签。
    /// 这验证了「update-keygen 生成私钥 → update-sign 签名 → 客户端内置公钥」闭环。
    /// 手动跑：先 `pwsh -File scripts/package.ps1` + `pwsh -File scripts/gen-latest-json.ps1`
    #[test]
    #[ignore = "需要先构建产物并生成最新清单"]
    fn verifies_generated_manifest() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist/latest.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()));
        let m: Manifest = serde_json::from_str(&text).expect("清单应为合法 JSON");
        m.verify()
            .unwrap_or_else(|e| panic!("发布脚本生成的清单未通过内置公钥验签：{e}"));
        assert!(
            crate::update::version::Version::parse(&m.version).is_some(),
            "清单版本号应可解析：{}",
            m.version
        );
    }
}
