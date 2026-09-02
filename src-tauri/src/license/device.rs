//! 设备标识（机器码）：读取操作系统级机器指纹（Windows = 注册表
//! MachineGuid），系统级唯一且重装应用/清数据不变、重装系统才变化。
//! 契约要求：deviceId 变化 = 换设备，会占用新的设备名额。

/// 当前设备的稳定机器码。进程内缓存（OnceLock）：激活/验证/续验/
/// 埋点都会反复调用，注册表打开+读值不该重复发生（GUID 运行期不变）。
pub fn device_id() -> String {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(fallback_or_fingerprint).clone()
}

fn fallback_or_fingerprint() -> String {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Cryptography")
            .and_then(|k| k.get_value::<String, _>("MachineGuid"))
            .unwrap_or_else(|_| fallback_device_id())
    }
    #[cfg(not(windows))]
    {
        // 非 Windows 仅为编译兼容（发布流水线目前只出 NSIS），
        // 没有稳定的系统级指纹可用，直接走兜底。
        fallback_device_id()
    }
}

/// 兜底：注册表读取失败时用主机名+用户名组合，
/// 避免所有机器共用同一 id（跨机复制许可证）。
fn fallback_device_id() -> String {
    #[cfg(windows)]
    let (host_var, user_var) = ("COMPUTERNAME", "USERNAME");
    #[cfg(not(windows))]
    let (host_var, user_var) = ("HOSTNAME", "USER");
    format!(
        "fallback-{}-{}",
        std::env::var(host_var).unwrap_or_else(|_| "host".into()),
        std::env::var(user_var).unwrap_or_else(|_| "user".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_is_stable_and_within_contract_length() {
        let first = device_id();
        let second = device_id();
        assert_eq!(first, second, "同进程内 deviceId 必须稳定");
        assert!(
            (8..=128).contains(&first.len()),
            "deviceId 长度必须在 8-128 之间，实际 {first}（{first_len} 字符）",
            first_len = first.len()
        );
    }
}
