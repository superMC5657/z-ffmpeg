//! 软糖铺授权接口 HTTP 客户端：激活 / 续验 / 注销。
//! 统一 10s 超时（契约约定，避免对话框永久卡死）；调用方需在阻塞线程执行。

use serde_json::{json, Value};

use super::config::HTTP_TIMEOUT_SECS;

/// 服务端返回的授权响应（activate 成功）
#[derive(Debug, Clone)]
pub struct ActivateResponse {
    pub license: String,
    pub expires_at: String,
    pub level_label: Option<String>,
}

/// 服务端返回的续验响应（verify 成功）
#[derive(Debug, Clone)]
pub struct VerifyResponse {
    pub license: String,
    pub expires_at: String,
}

/// 带错误码的接口错误（用于映射中文文案）
#[derive(Debug, Clone)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", error_message(&self.code, &self.message))
    }
}

/// 错误码 → 用户可读的中文文案（契约错误码分支，未识别的回退服务端 message）
pub fn error_message(code: &str, fallback: &str) -> String {
    let msg = match code {
        "INVALID_REQUEST" => "请求格式错误",
        "INVALID_CDK" => "激活码格式不正确（应为 XXXX-XXXX-XXXX-XXXX）",
        "INVALID_DEVICE_ID" => "设备标识无效",
        "INVALID_EMAIL" => "邮箱格式不正确",
        "INVALID_LICENSE" => "本地授权令牌无效",
        "CDK_NOT_FOUND" => "激活码不存在，请检查输入",
        "APP_MISMATCH" => "激活码不属于当前应用",
        "CDK_UNAVAILABLE" => "激活码已失效（可能已撤销、退款或订单未支付）",
        "LEVEL_MISMATCH" => "激活码与授权等级不一致",
        "LEVEL_NOT_FOUND" => "该应用未配置此授权等级",
        "LEVEL_DISABLED" => "该授权等级已停用，暂时无法激活",
        "EMAIL_MISMATCH" => "激活码与购买邮箱不匹配，请确认邮箱输入",
        "LIMIT_EXCEEDED" => "激活码的设备名额已满，请先在其他设备上注销激活，或联系客服解绑",
        "LICENSE_MISMATCH" => "授权与当前设备不匹配，请重新激活",
        "INVALID_SIGNATURE" => "授权签名无效，请重新激活",
        "CDK_REVOKED" => "激活码已失效（撤销 / 退款 / 订单未支付）",
        "DEVICE_NOT_ACTIVATED" => "本设备的激活已被注销或解绑，请重新激活",
        "APP_NOT_FOUND" => "产品不存在或已下架",
        "UNAUTHORIZED" => "鉴权失败",
        "RATE_LIMITED" => "请求过于频繁，请稍后再试",
        _ => fallback,
    };
    if msg.is_empty() { fallback.to_string() } else { msg.to_string() }
}

fn client() -> reqwest::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
}

/// POST JSON 并解析响应体。HTTP 4xx/5xx 时把 {"error","message"} 转成 ApiError；
/// 网络失败 / 超时返回 Err(网络原因) —— 调用方据此走离线降级。
fn post_json(url: &str, body: Value) -> Result<Value, super::manager::LicenseFlowError> {
    let resp = client()
        .map_err(|e| super::manager::LicenseFlowError::Network(e.to_string()))?
        .post(url)
        .json(&body)
        .send()
        .map_err(|e| super::manager::LicenseFlowError::Network(e.to_string()))?;

    let status = resp.status();
    let value: Value = resp
        .json()
        .map_err(|e| super::manager::LicenseFlowError::Network(format!("响应解析失败: {e}")))?;

    if status.is_success() {
        Ok(value)
    } else {
        let code = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Err(super::manager::LicenseFlowError::Api(ApiError { code, message }))
    }
}

/// POST /activate —— 激活码绑定当前设备，获取授权令牌。
/// 同 code + deviceId 重复激活幂等（返回 200 新令牌）。
pub fn activate(
    url: &str,
    level: &str,
    code: &str,
    device_id: &str,
    email: &str,
) -> Result<ActivateResponse, super::manager::LicenseFlowError> {
    let value = post_json(
        url,
        json!({
            "code": code,
            "deviceId": device_id,
            "email": email,
            "level": level,
        }),
    )?;
    Ok(ActivateResponse {
        license: value
            .get("license")
            .and_then(|v| v.as_str())
            .ok_or(super::manager::LicenseFlowError::Network("响应缺少 license 字段".into()))?
            .to_string(),
        expires_at: value
            .get("expiresAt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        level_label: value
            .get("levelLabel")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

/// POST /verify —— 在线续验，服务端签发新令牌延长离线宽限期。
pub fn verify(url: &str, device_id: &str, license: &str, email: &str) -> Result<VerifyResponse, super::manager::LicenseFlowError> {
    let value = post_json(
        url,
        json!({
            "deviceId": device_id,
            "license": license,
            "email": email,
        }),
    )?;
    if value.get("valid").and_then(|v| v.as_bool()) != Some(true) {
        return Err(super::manager::LicenseFlowError::Network(
            "服务端返回 valid != true".into(),
        ));
    }
    Ok(VerifyResponse {
        license: value
            .get("license")
            .and_then(|v| v.as_str())
            .ok_or(super::manager::LicenseFlowError::Network("响应缺少 license 字段".into()))?
            .to_string(),
        expires_at: value
            .get("expiresAt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// POST /deactivate —— 解绑设备、释放名额。幂等：已解绑同样返回 200。
pub fn deactivate(url: &str, code: &str, device_id: &str, email: &str) -> Result<(), super::manager::LicenseFlowError> {
    let value = post_json(
        url,
        json!({
            "code": code,
            "deviceId": device_id,
            "email": email,
        }),
    )?;
    if value.get("unbound").and_then(|v| v.as_bool()) != Some(true) {
        return Err(super::manager::LicenseFlowError::Network(
            "服务端返回 unbound != true".into(),
        ));
    }
    Ok(())
}
