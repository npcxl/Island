use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SystemEvent {
    VolumeChange { percent: u8, muted: bool },
    CapsLock     { on: bool },
    LowBattery   { percent: u8 },
    /// 微信：仅窗口枚举（不走通知中心，避免与 QQ 等 Toast 混淆）
    WeChat {
        source: String,
        kind: String,
        title: String,
        body: String,
    },
    /// 其它应用：系统通知中心 Toast（QQ、Outlook、Teams…）
    AppToast {
        app_name: String,
        /// `AppUserModelId`，用于前端匹配常用应用图标
        aumid: Option<String>,
        title: String,
        body: String,
        icon_base64: Option<String>,
    },
}

pub type SharedQueue = Arc<Mutex<VecDeque<SystemEvent>>>;

pub fn new_queue() -> SharedQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}
