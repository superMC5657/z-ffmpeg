pub mod encode;
pub mod queue;
pub mod preset;
pub mod system;
pub mod history;
pub mod vmaf;
pub mod license;
pub mod analytics;

use crate::encoder::codec::EncodeConfig;
use crate::error::AppResult;
use crate::license::LicenseManager;

/// 编码配置的门控检查（硬件加速与高级参数透传已开放给免费版）
pub fn ensure_config_allowed(
    _license: &LicenseManager,
    _config: &EncodeConfig,
) -> AppResult<()> {
    Ok(())
}
