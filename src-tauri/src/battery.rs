use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BatteryInfo {
    pub percent: u32,
    pub charging: bool,
}

#[cfg(target_os = "windows")]
pub fn get_battery() -> BatteryInfo {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { let _ = GetSystemPowerStatus(&mut status); }

    let has_battery = status.BatteryLifePercent != 255;
    let percent = if has_battery {
        status.BatteryLifePercent as u32
    } else {
        100 // 纯 AC 供电（台式机/无电池设备），显示 100%
    };

    // 只有 有实体电池 且 接入外部电源 才算"充电"
    // 台式机 / 无电池笔记本：has_battery=false，不触发充电动画
    let charging = has_battery && status.ACLineStatus == 1;

    BatteryInfo { percent, charging }
}

#[cfg(not(target_os = "windows"))]
pub fn get_battery() -> BatteryInfo {
    BatteryInfo { percent: 100, charging: false }
}
