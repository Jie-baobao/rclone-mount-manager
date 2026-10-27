//! 导出文件的口令加密。
//!
//! 为什么需要它：导出的 JSON 里带着 `password_obscured`（rclone 的混淆串，可逆），
//! 随手发到群里/传到网盘就等于把 NAS 口令一起给出去了。这里提供一个可选的口令保护：
//!
//! - 密钥派生：PBKDF2-HMAC-SHA256，16 字节随机盐，120000 轮
//! - 加密：AES-256-GCM（带认证，口令错或文件被改都会在解密时失败，而不是给出乱码）
//! - 载荷：仍然是「导出 JSON 的文本」，解密后交给原来的导入逻辑处理，格式不变
//!
//! 信封长这样（`encrypted: true` 是导入时的判断依据）：
//! ```json
//! {
//!   "app": "rclone-mount-manager", "format": 2, "encrypted": true,
//!   "kdf": "pbkdf2-sha256", "iterations": 120000,
//!   "salt": "base64", "nonce": "base64", "ciphertext": "base64"
//! }
//! ```

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;

const ITERATIONS: u32 = 120_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

fn derive_key(password: &str, salt: &[u8], iterations: u32) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut key);
    key
}

/// 把明文（完整的导出 JSON 文本）加密成信封 JSON 字符串
pub fn encrypt(plain: &str, password: &str) -> Result<String, String> {
    if password.is_empty() {
        return Err("口令不能为空".into());
    }
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

    let key = derive_key(password, &salt, ITERATIONS);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("加密初始化失败：{e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plain.as_bytes())
        .map_err(|_| "加密失败".to_string())?;

    let doc = serde_json::json!({
        "app": "rclone-mount-manager",
        "format": 2,
        "encrypted": true,
        "kdf": "pbkdf2-sha256",
        "iterations": ITERATIONS,
        "salt": B64.encode(salt),
        "nonce": B64.encode(nonce_bytes),
        "ciphertext": B64.encode(ciphertext),
    });
    serde_json::to_string_pretty(&doc).map_err(|e| format!("序列化失败：{e}"))
}

/// 判断一份文本是不是加密信封（导入时需要据此提示用户输入口令）
pub fn is_encrypted(doc: &serde_json::Value) -> bool {
    doc.get("encrypted").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// 解密信封，返回原始明文 JSON 文本。口令错会返回明确的中文提示。
pub fn decrypt(doc: &serde_json::Value, password: &str) -> Result<String, String> {
    let field = |name: &str| -> Result<String, String> {
        doc.get(name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("加密文件缺少 {name} 字段"))
    };
    let salt = B64
        .decode(field("salt")?)
        .map_err(|_| "加密文件的 salt 不是合法 base64".to_string())?;
    let nonce_bytes = B64
        .decode(field("nonce")?)
        .map_err(|_| "加密文件的 nonce 不是合法 base64".to_string())?;
    let ciphertext = B64
        .decode(field("ciphertext")?)
        .map_err(|_| "加密文件的内容不是合法 base64".to_string())?;
    let iterations = doc
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(ITERATIONS as u64) as u32;
    if nonce_bytes.len() != NONCE_LEN {
        return Err("加密文件的 nonce 长度不对，可能不是本应用导出的文件".into());
    }

    let key = derive_key(password, &salt, iterations);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("解密初始化失败：{e}"))?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_slice())
        .map_err(|_| "口令不对，或文件已被修改".to_string())?;
    String::from_utf8(plain).map_err(|_| "解密结果不是有效的文本".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_wrong_password() {
        let plain = r#"{"app":"rclone-mount-manager","connections":[]}"#;
        let env = encrypt(plain, "hunter2").unwrap();
        let doc: serde_json::Value = serde_json::from_str(&env).unwrap();
        assert!(is_encrypted(&doc));
        assert_eq!(decrypt(&doc, "hunter2").unwrap(), plain);
        assert!(decrypt(&doc, "wrong").is_err());
    }
}
