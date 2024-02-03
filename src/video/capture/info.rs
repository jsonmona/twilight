use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub resolution: Resolution,
    pub refresh_rate: RefreshRate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefreshRate {
    pub num: u32,
    pub den: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}
