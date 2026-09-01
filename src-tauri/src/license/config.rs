//! 软糖铺接入配置：编译进二进制的 `tauri.conf.json → plugins.softcandy` 区块，
//! 启动时解析一次（对齐 image-viewer 参考实现的配置方式）。
//!
//! 字段缺失即视为未配置（契约约定）：
//! - `apiBase` 为空 → 客户端完全不联网（激活/续验/注销/埋点均跳过）；
//! - 等级公钥缺失 → 离线验签必失败（等于免费版）；
//! - `analyticsToken` 为空 → 埋点上报不带 Authorization。

use std::collections::HashMap;

use serde::Deserialize;

/// 免费版允许的最大并发编码任务数
pub const FREE_MAX_CONCURRENT: usize = 2;

/// 在线请求统一超时（契约约定 10s，避免对话框永久卡死）
pub const HTTP_TIMEOUT_SECS: u64 = 10;

/// `tauri.conf.json → plugins.softcandy` 的结构
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SoftCandyConfig {
    /// 软糖铺 API Base URL；为空则客户端完全不联网
    pub api_base: String,
    /// 购买页链接，激活对话框展示「购买激活码」入口；空则不展示
    pub buy_url: String,
    /// 产品标识（slug），接口路径里的 `{slug}` 用它替换
    pub product: String,
    /// 授权等级名
    pub license_level: String,
    /// 授权等级名 → Ed25519 公钥（PEM 或 base64），离线验签用
    pub license_public_keys: HashMap<String, String>,
    /// 本地许可证文件名（存放在应用数据目录下）
    pub license_file_name: String,
    /// 激活接口路径（`{slug}` 占位可选）
    pub activate_path: String,
    /// 续验接口路径
    pub verify_path: String,
    /// 注销接口路径
    pub deactivate_path: String,
    /// 埋点接口路径
    pub analytics_path: String,
    /// 埋点上报 token；为空则不带 Authorization
    pub analytics_token: String,
}

impl Default for SoftCandyConfig {
    fn default() -> Self {
        SoftCandyConfig {
            api_base: String::new(),
            buy_url: String::new(),
            product: String::new(),
            license_level: "pro".into(),
            license_public_keys: HashMap::new(),
            license_file_name: "license.json".into(),
            // 路径给契约默认值：只配 apiBase + product 也能工作
            activate_path: "/api/v1/apps/{slug}/activate".into(),
            verify_path: "/api/v1/apps/{slug}/verify".into(),
            deactivate_path: "/api/v1/apps/{slug}/deactivate".into(),
            analytics_path: "/api/v1/apps/{slug}/analytics".into(),
            analytics_token: String::new(),
        }
    }
}

impl SoftCandyConfig {
    /// 从 JSON 值解析（测试用；主流程走 `from_tauri`）
    #[cfg(test)]
    fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap()
    }

    /// 从 Tauri 运行时配置解析 `plugins.softcandy`；区块缺失 = 默认配置（不联网）。
    pub fn from_tauri(config: &tauri::Config) -> Self {
        let Some(value) = config.plugins.0.get("softcandy") else {
            log::info!("tauri.conf.json 未配置 plugins.softcandy，授权/埋点均不联网");
            return Self::default();
        };
        match serde_json::from_value::<SoftCandyConfig>(value.clone()) {
            Ok(cfg) => {
                log::info!(
                    "软糖铺配置加载完成: product={}, apiBase={}, level={}",
                    cfg.product,
                    cfg.api_base,
                    cfg.license_level
                );
                cfg
            }
            Err(e) => {
                log::error!("plugins.softcandy 配置解析失败，按未配置处理: {e}");
                Self::default()
            }
        }
    }

    /// 是否联网（apiBase 为空 = 跳过所有在线请求）
    pub fn online_enabled(&self) -> bool {
        !self.api_base.is_empty()
    }

    /// 当前授权等级的公钥（缺失 → 离线验签必失败）
    pub fn level_public_key(&self) -> Option<&str> {
        self.license_public_keys
            .get(&self.license_level)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
    }

    fn url(&self, path_template: &str) -> String {
        let path = path_template.replace("{slug}", &self.product);
        format!("{}{}", self.api_base, path)
    }

    pub fn activate_url(&self) -> String {
        self.url(&self.activate_path)
    }

    pub fn verify_url(&self) -> String {
        self.url(&self.verify_path)
    }

    pub fn deactivate_url(&self) -> String {
        self.url(&self.deactivate_path)
    }

    pub fn analytics_url(&self) -> String {
        self.url(&self.analytics_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from_json(json: &str) -> SoftCandyConfig {
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        SoftCandyConfig::from_value(&value)
    }

    #[test]
    fn parses_camel_case_fields() {
        let cfg = config_from_json(
            r#"{
                "apiBase": "http://127.0.0.1:8080",
                "buyUrl": "http://localhost:5173/buy/z-ffmpeg",
                "product": "z-ffmpeg",
                "licenseLevel": "pro",
                "licensePublicKeys": { "pro": "HHvVDoB7i1gMlCH7PreE2h2lovqa+taR6mb756xpmyE=" },
                "licenseFileName": "license.json",
                "activatePath": "/api/v1/apps/z-ffmpeg/activate",
                "verifyPath": "/api/v1/apps/z-ffmpeg/verify",
                "deactivatePath": "/api/v1/apps/z-ffmpeg/deactivate",
                "analyticsPath": "/api/v1/apps/z-ffmpeg/analytics",
                "analyticsToken": "98f6e02b"
            }"#,
        );
        assert_eq!(cfg.api_base, "http://127.0.0.1:8080");
        assert_eq!(cfg.product, "z-ffmpeg");
        assert_eq!(cfg.license_level, "pro");
        assert!(cfg.level_public_key().is_some());
        assert!(cfg.online_enabled());
        assert_eq!(
            cfg.activate_url(),
            "http://127.0.0.1:8080/api/v1/apps/z-ffmpeg/activate"
        );
        assert_eq!(
            cfg.analytics_url(),
            "http://127.0.0.1:8080/api/v1/apps/z-ffmpeg/analytics"
        );
    }

    #[test]
    fn slug_template_path_falls_back_to_product() {
        let cfg = config_from_json(
            r#"{
                "apiBase": "http://localhost:8080",
                "product": "my-app"
            }"#,
        );
        // 未显式配置路径 → 用默认模板 + {slug} 替换
        assert_eq!(
            cfg.verify_url(),
            "http://localhost:8080/api/v1/apps/my-app/verify"
        );
    }

    #[test]
    fn missing_key_means_no_key() {
        let cfg = config_from_json(r#"{ "product": "z-ffmpeg" }"#);
        assert_eq!(cfg.level_public_key(), None);
    }

    #[test]
    fn empty_api_base_means_offline() {
        let cfg = config_from_json(r#"{ "product": "z-ffmpeg" }"#);
        assert!(!cfg.online_enabled());
    }

    #[test]
    fn unknown_fields_and_missing_section_fall_back_to_defaults() {
        let cfg = config_from_json(r#"{ "somethingElse": 1 }"#);
        assert_eq!(cfg.license_level, "pro");
        assert_eq!(cfg.license_file_name, "license.json");
        assert_eq!(cfg.analytics_path, "/api/v1/apps/{slug}/analytics");
    }
}
