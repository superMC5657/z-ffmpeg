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

/// 编码配置的 Pro 门控：硬件加速、additional_params 高级参数透传。
/// 所有编码入口（start_encode / add_to_queue / build_ffmpeg_commands）共用，
/// 保证门控只由后端强制、无法从前端绕过。
pub fn ensure_config_allowed(
    license: &LicenseManager,
    config: &EncodeConfig,
) -> AppResult<()> {
    if config.hw_accel.is_some() {
        license.ensure_pro("硬件加速编码")?;
    }
    if !config.video_settings.additional_params.is_empty() {
        license.ensure_pro("高级参数透传")?;
    }
    Ok(())
}
