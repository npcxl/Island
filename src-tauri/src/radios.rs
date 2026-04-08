use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RadiosState {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

#[cfg(target_os = "windows")]
fn to_bool_state(state: windows::Devices::Radios::RadioState) -> Option<bool> {
    use windows::Devices::Radios::RadioState;
    match state {
        RadioState::On => Some(true),
        RadioState::Off => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn to_target_state(on: bool) -> windows::Devices::Radios::RadioState {
    use windows::Devices::Radios::RadioState;
    if on { RadioState::On } else { RadioState::Off }
}

#[cfg(target_os = "windows")]
fn with_sta<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    // WinRT Radios 需要在 STA 下跑；我们用临时线程避免与其他 COM 上下文纠缠
    std::thread::spawn(move || {
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        f()
    })
    .join()
    .ok()
    .expect("sta thread panicked")
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_radios_state() -> RadiosState {
    use windows::Devices::Radios::{Radio, RadioKind};

    with_sta(|| {
        let mut wifi = None;
        let mut bluetooth = None;

        let list = match Radio::GetRadiosAsync() {
            Ok(op) => op.get().ok(),
            Err(_) => None,
        };
        if let Some(radios) = list {
            for r in radios {
                let kind = r.Kind().ok();
                let st = r.State().ok();
                match (kind, st) {
                    (Some(RadioKind::WiFi), Some(s)) => wifi = to_bool_state(s),
                    (Some(RadioKind::Bluetooth), Some(s)) => bluetooth = to_bool_state(s),
                    _ => {}
                }
            }
        }

        RadiosState { wifi, bluetooth }
    })
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_radios_state() -> RadiosState {
    RadiosState {
        wifi: None,
        bluetooth: None,
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_radio_state(kind: String, on: bool) -> bool {
    use windows::Devices::Radios::{Radio, RadioKind};

    with_sta(move || {
        let target_kind = match kind.as_str() {
            "wifi" => RadioKind::WiFi,
            "bluetooth" => RadioKind::Bluetooth,
            _ => return false,
        };

        let radios = match Radio::GetRadiosAsync().and_then(|op| op.get()) {
            Ok(v) => v,
            Err(_) => return false,
        };

        for r in radios {
            if r.Kind().ok() == Some(target_kind) {
                let desired = to_target_state(on);
                if let Ok(op) = r.SetStateAsync(desired) {
                    return op.get().is_ok();
                }
                return false;
            }
        }
        false
    })
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_radio_state(_kind: String, _on: bool) -> bool {
    false
}

