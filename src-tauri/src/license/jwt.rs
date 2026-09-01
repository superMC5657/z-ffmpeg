//! 授权令牌（Ed25519 签名的 JWT）离线验签。
//!
//! 令牌由软糖铺服务端签发（`alg: EdDSA`），客户端用编译进二进制的等级公钥
//! 验签 + 校验 `exp` / `deviceId` / `app`，通过即认为授权有效（离线宽限期
//! 以最近一次成功在线验证签发的 `exp` 为界）。

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;

#[derive(Debug)]
pub enum JwtError {
    /// 令牌格式不正确（不是合法 JWT）
    Malformed,
    /// 签名无效（被篡改，或应用公钥已更换）
    InvalidSignature,
    /// 令牌已过期
    Expired,
    /// 令牌与当前设备 / 应用 / 等级不匹配
    Mismatch(String),
    /// 客户端未配置公钥
    NoKey,
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwtError::Malformed => write!(f, "令牌格式不正确"),
            JwtError::InvalidSignature => write!(f, "令牌签名无效"),
            JwtError::Expired => write!(f, "授权已过期"),
            JwtError::Mismatch(m) => write!(f, "令牌不匹配: {m}"),
            JwtError::NoKey => write!(f, "未配置验签公钥"),
        }
    }
}

/// JWT payload 中客户端关心的 claims 子集
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub sub: Option<String>,
    pub code: Option<String>,
    pub app: Option<String>,
    pub level: Option<String>,
    pub level_label: Option<String>,
    pub device_id: Option<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    pub iat: Option<i64>,
    pub exp: Option<i64>,
}

/// 解析配置里的公钥：支持 PEM（SPKI DER，取末尾 32 字节）或 raw 32 字节的
/// base64（标准或 URL safe、可有可无 padding）。
pub fn parse_public_key(config: &str) -> Option<[u8; 32]> {
    let stripped: String = config
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with("-----"))
        .collect();

    let bytes = STANDARD
        .decode(stripped.trim_end_matches('='))
        .or_else(|_| URL_SAFE_NO_PAD.decode(stripped.trim_end_matches('=')))
        .or_else(|_| STANDARD.decode(&stripped))
        .ok()?;

    match bytes.len() {
        32 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Some(key)
        }
        // Ed25519 SPKI DER 固定 44 字节：12 字节算法头 + 32 字节公钥
        44 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes[12..]);
            Some(key)
        }
        _ => None,
    }
}

/// 验签并校验 claims。`expected_device` 为当前机器码；`expected_app` / `expected_level`
/// 为配置的应用 slug 与授权等级；`now` 由调用方传入（便于测试）。
pub fn verify_license_token(
    token: &str,
    public_key: &[u8; 32],
    expected_device: &str,
    expected_app: &str,
    expected_level: &str,
    now: i64,
) -> Result<Claims, JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Malformed);
    }
    let (header_b64, payload_b64, sig_b64) = (parts[0], parts[1], parts[2]);

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| JwtError::Malformed)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| JwtError::Malformed)?;

    let signing_input = format!("{header_b64}.{payload_b64}");
    let verifying_key = VerifyingKey::from_bytes(public_key).map_err(|_| JwtError::NoKey)?;
    verifying_key
        .verify_strict(signing_input.as_bytes(), &signature)
        .map_err(|_| JwtError::InvalidSignature)?;

    let payload_json = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| JwtError::Malformed)?;
    let claims: Claims =
        serde_json::from_slice(&payload_json).map_err(|_| JwtError::Malformed)?;

    // exp 校验：缺失视为已过期（服务端签发必含 exp）
    let exp = claims.exp.ok_or(JwtError::Expired)?;
    if now >= exp {
        return Err(JwtError::Expired);
    }

    // deviceId 必须与当前机器一致（离线校验的核心）
    match &claims.device_id {
        Some(id) if id == expected_device => {}
        Some(_) => return Err(JwtError::Mismatch("设备不一致".into())),
        None => return Err(JwtError::Mismatch("令牌未绑定设备".into())),
    }

    // 应用 / 等级一致性（claims 缺失时宽容，服务端签发必含）
    if let Some(app) = &claims.app {
        if app != expected_app {
            return Err(JwtError::Mismatch(format!("应用不一致: {app}")));
        }
    }
    if let Some(level) = &claims.level {
        if level != expected_level {
            return Err(JwtError::Mismatch(format!("等级不一致: {level}")));
        }
    }

    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn make_keypair() -> (SigningKey, [u8; 32]) {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let public = signing.verifying_key().to_bytes();
        (signing, public)
    }

    /// 用测试密钥签一个与线上格式一致的 JWT（EdDSA / base64url 三段）
    fn sign_jwt(signing: &SigningKey, claims: &serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
        let signing_input = format!("{header}.{payload}");
        let sig = signing.sign(signing_input.as_bytes());
        format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes()))
    }

    fn claims_json(device: &str, exp: i64) -> serde_json::Value {
        serde_json::json!({
            "iss": "soft-candy",
            "sub": "SDX4-K9TP-2M7Q-W3HZ",
            "app": "z-ffmpeg",
            "level": "pro",
            "levelLabel": "专业版",
            "deviceId": device,
            "email": "buyer@example.com",
            "features": ["pro"],
            "iat": 1750000000,
            "exp": exp,
        })
    }

    #[test]
    fn parse_raw_and_spki_public_key() {
        let (_, public) = make_keypair();
        // raw 32 字节 base64
        let raw = STANDARD.encode(public);
        assert_eq!(parse_public_key(&raw).as_ref(), Some(&public));
        // SPKI DER（12 字节头 + 32 字节公钥）
        let mut der = vec![0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
        der.extend_from_slice(&public);
        let pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
            STANDARD.encode(&der)
        );
        assert_eq!(parse_public_key(&pem).as_ref(), Some(&public));
        // 长度不对 → None
        assert_eq!(parse_public_key("AAAA"), None);
    }

    #[test]
    fn valid_token_passes() {
        let (signing, public) = make_keypair();
        let token = sign_jwt(&signing, &claims_json("device-0001", 2000));
        let claims = verify_license_token(&token, &public, "device-0001", "z-ffmpeg", "pro", 1000).unwrap();
        assert_eq!(claims.email.as_deref(), Some("buyer@example.com"));
        assert_eq!(claims.level.as_deref(), Some("pro"));
    }

    #[test]
    fn expired_token_rejected() {
        let (signing, public) = make_keypair();
        let token = sign_jwt(&signing, &claims_json("device-0001", 1000));
        assert!(matches!(
            verify_license_token(&token, &public, "device-0001", "z-ffmpeg", "pro", 1000),
            Err(JwtError::Expired)
        ));
    }

    #[test]
    fn wrong_device_rejected() {
        let (signing, public) = make_keypair();
        let token = sign_jwt(&signing, &claims_json("device-0001", 2000));
        assert!(matches!(
            verify_license_token(&token, &public, "device-0002", "z-ffmpeg", "pro", 1000),
            Err(JwtError::Mismatch(_))
        ));
    }

    #[test]
    fn tampered_token_rejected() {
        let (signing, public) = make_keypair();
        let mut token = sign_jwt(&signing, &claims_json("device-0001", 2000));
        // 篡改 payload 最后一个字符（仍保持合法 base64url 的字符集内翻转）
        let last = token.pop().unwrap();
        token.push(if last == 'A' { 'B' } else { 'A' });
        assert!(matches!(
            verify_license_token(&token, &public, "device-0001", "z-ffmpeg", "pro", 1000),
            Err(JwtError::InvalidSignature) | Err(JwtError::Malformed)
        ));
    }

    #[test]
    fn malformed_token_rejected() {
        let (_, public) = make_keypair();
        assert!(matches!(
            verify_license_token("not-a-jwt", &public, "d", "z-ffmpeg", "pro", 0),
            Err(JwtError::Malformed)
        ));
    }
}
