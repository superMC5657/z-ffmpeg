//! 设备标识（机器码）：首次运行生成 uuid v4 持久化到 `{data_dir}/zffmpeg/device.id`，
//! 同一次安装内保持稳定（契约要求：deviceId 变化 = 换设备，会占用新名额）。

use std::path::Path;

/// 读取（或首次创建）稳定的 deviceId。读不到/写不进时退回临时 uuid，
/// 仅影响本次会话的激活，不产生更坏的后果。
pub fn get_or_create_device_id(data_dir: &Path) -> String {
    let file = data_dir.join("device.id");

    if let Ok(id) = std::fs::read_to_string(&file) {
        let id = id.trim().to_string();
        if (8..=128).contains(&id.len()) {
            return id;
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = std::fs::write(&file, &id) {
        log::warn!("写入 device.id 失败（deviceId 将不跨重启稳定）: {e}");
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_is_stable_and_persisted() {
        let dir = std::env::temp_dir().join(format!("zffmpeg-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let first = get_or_create_device_id(&dir);
        let second = get_or_create_device_id(&dir);
        assert_eq!(first, second);
        assert!((8..=128).contains(&first.len()));
        assert!(dir.join("device.id").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
