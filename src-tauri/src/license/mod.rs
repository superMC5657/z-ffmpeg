//! 软糖铺授权（激活 / 续验 / 注销 + 离线验签）与 Pro 门控。

pub mod client;
pub mod config;
pub mod device;
pub mod jwt;
pub mod manager;

pub use manager::{LicenseManager, LicenseStatus};
