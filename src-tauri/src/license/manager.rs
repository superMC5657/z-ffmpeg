//! 授权状态机：激活 / 在线续验 / 注销 + 离线验签降级（见接入指南 3.6 节流程）。
//!
//! - 本地凭证 `license.json`（code + license JWT + email），启动加载并离线验签，
//!   验签失败 = 免费版（凭证保留，激活对话框可回显 code/email）；
//! - 在线请求失败不判定授权失效，按离线宽限期（最近一次在线验证的 `exp`）降级；
//! - 收到 401 类失效错误（CDK_REVOKED / DEVICE_NOT_ACTIVATED）立即删除凭证降级免费版；
//! - debug 构建恒为已解锁，开发调试不被门控干扰。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;

use super::client::{self, ApiError};
use super::config::SoftCandyConfig;
use super::device;
use super::jwt::{self, Claims, JwtError};

#[derive(Debug)]
pub enum LicenseFlowError {
    /// 网络失败 / 超时 / 响应异常——不判定授权失效，走离线降级
    Network(String),
    /// 服务端返回的结构化错误（error code）
    Api(ApiError),
}

impl std::fmt::Display for LicenseFlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseFlowError::Network(m) => write!(f, "{m}"),
            LicenseFlowError::Api(e) => write!(f, "{e}"),
        }
    }
}

/// 本地保存的授权凭证
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredLicense {
    code: String,
    license: String,
    email: String,
}

/// Pro 状态明细（内部）
#[derive(Debug, Clone)]
struct ProInfo {
    code: String,
    email: String,
    level_label: Option<String>,
    expires_at: String,
    /// 令牌到期时间（Unix 秒），is_pro 快速判断用
    exp: i64,
    features: Vec<String>,
    /// 是否处于离线宽限期（最近一次在线验证失败 / 尚未在线验证）
    offline: bool,
}

#[derive(Debug, Clone)]
enum LicenseState {
    Free,
    Pro(ProInfo),
}

/// 前端展示用的授权状态（camelCase，对齐 src/types/index.ts 的 LicenseStatus）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    pub pro: bool,
    pub level_label: Option<String>,
    pub email: Option<String>,
    /// 令牌到期时间（RFC3339）
    pub expires_at: Option<String>,
    pub features: Vec<String>,
    /// 是否处于离线宽限期
    pub offline: bool,
    /// 已绑定的激活码（激活对话框回显）
    pub code: Option<String>,
    /// 购买页链接（激活对话框「购买激活码」入口），未配置为 None
    pub buy_url: Option<String>,
}

pub struct LicenseManager {
    config: SoftCandyConfig,
    data_dir: PathBuf,
    device_id: String,
    state: RwLock<LicenseState>,
}

impl LicenseManager {
    pub fn new(config: SoftCandyConfig, data_dir: &Path) -> Self {
        let device_id = device::device_id();
        let manager = LicenseManager {
            config,
            data_dir: data_dir.to_path_buf(),
            device_id,
            state: RwLock::new(LicenseState::Free),
        };
        manager.restore_from_disk();
        manager
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn config(&self) -> &SoftCandyConfig {
        &self.config
    }

    fn license_path(&self) -> PathBuf {
        self.data_dir.join(&self.config.license_file_name)
    }

    fn load_stored(&self) -> Option<StoredLicense> {
        let text = std::fs::read_to_string(self.license_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save_stored(&self, stored: &StoredLicense) {
        if let Err(e) = std::fs::write(
            self.license_path(),
            serde_json::to_string_pretty(stored).unwrap_or_default(),
        ) {
            log::error!("保存 license.json 失败: {e}");
        }
    }

    fn delete_stored(&self) {
        let _ = std::fs::remove_file(self.license_path());
    }

    /// 启动时离线验签本地令牌，通过则直接进入 Pro（离线宽限期）
    fn restore_from_disk(&self) {
        let Some(stored) = self.load_stored() else {
            return;
        };
        match self.offline_verify(&stored.license) {
            Ok(claims) => {
                log::info!("本地授权验签通过（离线模式），等级: {:?}", claims.level);
                *self.state.write() = LicenseState::Pro(ProInfo {
                    code: stored.code,
                    email: claims.email.unwrap_or(stored.email),
                    level_label: claims.level_label,
                    expires_at: exp_to_rfc3339(claims.exp.unwrap_or(0)),
                    exp: claims.exp.unwrap_or(0),
                    features: claims.features,
                    offline: true,
                });
            }
            Err(e) => {
                log::info!("本地授权验签未通过，按免费版处理: {e}");
            }
        }
    }

    /// 用编译进二进制的公钥离线验签令牌（签名 + exp + deviceId + app/level）
    fn offline_verify(&self, token: &str) -> Result<Claims, JwtError> {
        let key_hex = self
            .config
            .level_public_key()
            .ok_or(JwtError::NoKey)?;
        let key = jwt::parse_public_key(key_hex).ok_or(JwtError::NoKey)?;
        jwt::verify_license_token(
            token,
            &key,
            &self.device_id,
            &self.config.product,
            &self.config.license_level,
            chrono::Utc::now().timestamp(),
        )
    }

    fn claims_to_pro_info(claims: &Claims, stored: &StoredLicense, offline: bool) -> ProInfo {
        ProInfo {
            code: claims.code.clone().unwrap_or_else(|| stored.code.clone()),
            email: claims.email.clone().unwrap_or_else(|| stored.email.clone()),
            level_label: claims.level_label.clone(),
            expires_at: exp_to_rfc3339(claims.exp.unwrap_or(0)),
            exp: claims.exp.unwrap_or(0),
            features: claims.features.clone(),
            offline,
        }
    }

    // ============================================================
    // 状态查询（门控入口）
    // ============================================================

    pub fn status(&self) -> LicenseStatus {
        let buy_url = if self.config.buy_url.is_empty() {
            None
        } else {
            Some(self.config.buy_url.clone())
        };
        let status = match &*self.state.read() {
            LicenseState::Free => LicenseStatus {
                pro: false,
                level_label: None,
                email: None,
                expires_at: None,
                features: vec![],
                offline: false,
                code: self.load_stored().map(|s| s.code),
                buy_url,
            },
            LicenseState::Pro(p) => LicenseStatus {
                pro: true,
                level_label: p.level_label.clone(),
                email: Some(p.email.clone()),
                expires_at: Some(p.expires_at.clone()),
                features: p.features.clone(),
                offline: p.offline,
                code: Some(p.code.clone()),
                buy_url,
            },
        };
        // debug 构建恒为已解锁（与 is_pro 一致），开发调试不被门控干扰
        if cfg!(debug_assertions) && !status.pro {
            return LicenseStatus {
                pro: true,
                level_label: Some("专业版（开发模式）".into()),
                expires_at: None,
                offline: false,
                features: vec!["pro".into()],
                ..status
            };
        }
        status
    }

    /// Pro 门控判断。debug 构建恒为 true（开发调试不被门控干扰）。
    pub fn is_pro(&self) -> bool {
        #[cfg(debug_assertions)]
        return true;

        #[cfg(not(debug_assertions))]
        match &*self.state.read() {
            LicenseState::Pro(p) => chrono::Utc::now().timestamp() < p.exp,
            LicenseState::Free => false,
        }
    }

    /// 门控助手：命令层拦截用，未授权返回用户可读错误（带功能名）。
    pub fn ensure_pro(&self, feature: &str) -> Result<(), crate::error::AppError> {
        if self.is_pro() {
            Ok(())
        } else {
            Err(crate::error::AppError::Internal(format!(
                "{feature}为 Pro 版功能，请在设置中激活授权后使用"
            )))
        }
    }

    // ============================================================
    // 激活 / 续验 / 注销（调用方需放在阻塞线程执行）
    // ============================================================

    /// 激活：把激活码绑定到当前设备。重复激活幂等（覆盖本地令牌）。
    pub fn activate(&self, code: &str, email: &str) -> Result<LicenseStatus, LicenseFlowError> {
        let code = code.trim().to_uppercase();
        let email = email.trim().to_string();
        if !is_valid_code_format(&code) {
            return Err(LicenseFlowError::Api(ApiError {
                code: "INVALID_CDK".into(),
                message: "激活码格式不正确".into(),
            }));
        }
        if !is_valid_email(&email) {
            return Err(LicenseFlowError::Api(ApiError {
                code: "INVALID_EMAIL".into(),
                message: "邮箱格式不正确".into(),
            }));
        }

        let resp = client::activate(
            &self.config.activate_url(),
            &self.config.license_level,
            &code,
            &self.device_id,
            &email,
        )?;

        let stored = StoredLicense {
            code,
            license: resp.license.clone(),
            email,
        };
        // 激活成功后本地验签一次：公钥不匹配等问题当场暴露
        let claims = self.offline_verify(&stored.license).map_err(|e| {
            log::error!("激活返回的令牌本地验签失败: {e}");
            LicenseFlowError::Network(format!("激活返回的令牌验签失败: {e}"))
        })?;

        *self.state.write() =
            LicenseState::Pro(Self::claims_to_pro_info(&claims, &stored, false));
        self.save_stored(&stored);
        log::info!("激活成功，令牌到期: {}", resp.expires_at);

        Ok(self.status())
    }

    /// 在线续验：成功必须用返回的新令牌覆盖本地保存，否则下次验证会因令牌过期失败。
    ///
    /// 返回值语义：
    /// - `Ok(true)`：续验成功，令牌已覆盖；
    /// - `Ok(false)`：网络失败——不判定授权失效，继续按离线宽限期使用；
    /// - `Err(e)`：授权已失效（401 类），凭证已删除、已降级免费版。
    pub fn verify_online(&self) -> Result<bool, LicenseFlowError> {
        // API_BASE 未配置时不联网
        if !self.config.online_enabled() {
            return Ok(false);
        }
        let Some(stored) = self.load_stored() else {
            return Ok(false);
        };

        match client::verify(&self.config.verify_url(), &self.device_id, &stored.license, &stored.email) {
            Ok(resp) => {
                let new_stored = StoredLicense {
                    code: stored.code,
                    license: resp.license,
                    email: stored.email,
                };
                match self.offline_verify(&new_stored.license) {
                    Ok(claims) => {
                        *self.state.write() =
                            LicenseState::Pro(Self::claims_to_pro_info(&claims, &new_stored, false));
                        self.save_stored(&new_stored);
                        log::info!("在线续验成功，令牌到期: {}", resp.expires_at);
                        Ok(true)
                    }
                    Err(e) => {
                        log::error!("续验返回的新令牌本地验签失败: {e}");
                        Err(LicenseFlowError::Network(format!("新令牌验签失败: {e}")))
                    }
                }
            }
            Err(LicenseFlowError::Network(m)) => {
                log::warn!("在线续验网络失败，按离线宽限期继续: {m}");
                Ok(false)
            }
            Err(LicenseFlowError::Api(api)) => {
                if matches!(api.code.as_str(), "CDK_REVOKED" | "DEVICE_NOT_ACTIVATED" | "INVALID_SIGNATURE") {
                    log::warn!("授权已失效（{}），删除本地凭证并降级免费版", api.code);
                    *self.state.write() = LicenseState::Free;
                    self.delete_stored();
                    Err(LicenseFlowError::Api(api))
                } else {
                    // EMAIL_MISMATCH 等其他错误：旧版本令牌场景，保留凭证等待重新激活
                    log::warn!("在线续验返回 {}，保留本地凭证", api.code);
                    Ok(false)
                }
            }
        }
    }

    /// 注销激活：解除设备绑定、释放名额，成功后必须删除本地令牌并停用专业功能。
    pub fn deactivate(&self) -> Result<LicenseStatus, LicenseFlowError> {
        let Some(stored) = self.load_stored() else {
            // 本来就没有凭证，幂等成功
            *self.state.write() = LicenseState::Free;
            return Ok(self.status());
        };

        client::deactivate(&self.config.deactivate_url(), &stored.code, &self.device_id, &stored.email)?;
        *self.state.write() = LicenseState::Free;
        self.delete_stored();
        log::info!("注销激活成功，已释放设备名额");

        Ok(self.status())
    }

    /// 启动时异步在线续验一次 + 每 24 小时周期续验。
    /// 网络失败不影响已付费用户（离线宽限期策略）。
    pub fn spawn_periodic_verify(self: &Arc<Self>) {
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            // 等应用启动完成再首次续验
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            loop {
                if manager.config.online_enabled() {
                    let m = manager.clone();
                    let result = tauri::async_runtime::spawn_blocking(move || m.verify_online()).await;
                    match result {
                        Ok(Ok(true)) => log::info!("周期续验: 令牌已更新"),
                        Ok(Ok(false)) => log::info!("周期续验: 网络不可用或跳过，保持离线宽限期"),
                        Ok(Err(e)) => log::warn!("周期续验: 授权失效: {e}"),
                        Err(e) => log::error!("周期续验任务异常: {e}"),
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
            }
        });
    }
}

/// 激活码格式：4×4 大写字母数字（服务端会再校验，客户端先做格式预检）
fn is_valid_code_format(code: &str) -> bool {
    let parts: Vec<&str> = code.split('-').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            p.len() == 4 && p.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

fn is_valid_email(email: &str) -> bool {
    // 宽松校验：非空、含 @、@ 后有域名——真正的校验在服务端
    let Some((_, domain)) = email.split_once('@') else {
        return false;
    };
    !email.starts_with('@') && domain.contains('.') && !domain.starts_with('.')
}

fn exp_to_rfc3339(exp: i64) -> String {
    chrono::DateTime::from_timestamp(exp, 0)
        .map(|d| d.with_timezone(&chrono::Local).to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_format_validation() {
        assert!(is_valid_code_format("SDX4-K9TP-2M7Q-W3HZ"));
        assert!(is_valid_code_format("ABCD-1234-ABCD-1234"));
        assert!(!is_valid_code_format("SDX4-K9TP-2M7Q"));
        assert!(!is_valid_code_format("SDX4-K9TP2M7Q-W3HZ"));
        assert!(!is_valid_code_format("SDX-K9TP-2M7Q-W3HZ"));
        assert!(!is_valid_code_format(""));
    }

    #[test]
    fn email_loose_validation() {
        assert!(is_valid_email("buyer@example.com"));
        assert!(is_valid_email("a.b@c.io"));
        assert!(!is_valid_email("buyer@example"));
        assert!(!is_valid_email("@example.com"));
        assert!(!is_valid_email("no-at-sign"));
        assert!(!is_valid_email(""));
    }
}
