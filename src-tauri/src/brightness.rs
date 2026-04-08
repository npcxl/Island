use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BrightnessInfo {
    pub supported: bool,
    pub percent: u8,
}

// WMI classes:
// - WmiMonitorBrightness (read)
// - WmiMonitorBrightnessMethods (write: WmiSetBrightness)
//
// 说明：外接显示器通常不支持这两类 WMI 亮度控制。

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_brightness() -> BrightnessInfo {
    use serde::Deserialize;
    use wmi::{COMLibrary, WMIConnection};

    #[derive(Debug, Deserialize)]
    struct Row {
        CurrentBrightness: u8,
    }

    let com = match COMLibrary::new() {
        Ok(v) => v,
        Err(_) => {
            return BrightnessInfo {
                supported: false,
                percent: 50,
            }
        }
    };
    let wmi_con = match WMIConnection::new(com.into()) {
        Ok(v) => v,
        Err(_) => {
            return BrightnessInfo {
                supported: false,
                percent: 50,
            }
        }
    };

    let rows: Result<Vec<Row>, _> = wmi_con.raw_query("SELECT CurrentBrightness FROM WmiMonitorBrightness");
    match rows {
        Ok(mut v) if !v.is_empty() => {
            let r = v.remove(0);
            BrightnessInfo {
                supported: true,
                percent: r.CurrentBrightness.min(100),
            }
        }
        _ => {
            // 外接显示器：尝试 DDC/CI 读取
            if let Some(p) = ddc_get_brightness() {
                BrightnessInfo {
                    supported: true,
                    percent: p,
                }
            } else {
                BrightnessInfo {
                    supported: false,
                    percent: 50,
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_brightness() -> BrightnessInfo {
    BrightnessInfo {
        supported: false,
        percent: 50,
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_brightness(percent: u8) -> bool {
    let pct = percent.min(100);
    // 1) WMI 直接调用（适合笔记本内屏）
    if wmi_set_brightness_ps(pct) {
        return true;
    }
    // 2) DDC/CI（外接显示器）
    ddc_set_brightness(pct)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_brightness(_percent: u8) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn wmi_set_brightness_ps(pct: u8) -> bool {
    use std::process::Command;
    // 更兼容的写法：直接执行 WmiSetBrightness
    let script = format!(
        "(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1,{}) | Out-Null",
        pct
    );
    Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn ddc_get_brightness() -> Option<u8> {
    use ddc_winapi::Monitor;
    let monitors = Monitor::enumerate().ok()?;
    for m in monitors {
        if let Ok((_ty, cur, max)) = m.winapi_get_vcp_feature_and_vcp_feature_reply(0x10) {
            if max > 0 {
                let pct = ((cur as f32 / max as f32) * 100.0).round() as i32;
                return Some(pct.clamp(0, 100) as u8);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn ddc_set_brightness(pct: u8) -> bool {
    use ddc_winapi::Monitor;
    let monitors = match Monitor::enumerate() {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut any_ok = false;
    for m in monitors {
        // VCP 0x10 = Brightness
        if m.winapi_set_vcp_feature(0x10, pct as u32).is_ok() {
            any_ok = true;
        }
    }
    any_ok
}

