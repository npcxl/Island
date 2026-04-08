use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct VolumeInfo {
    pub percent: u8,
    pub muted: bool,
}

#[cfg(target_os = "windows")]
fn endpoint() -> Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED};

    unsafe {
        // CoreAudio COM needs COM initialized on this thread.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let ep: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        Some(ep)
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_volume() -> Option<VolumeInfo> {
    unsafe {
        let ep = endpoint()?;
        let vol = ep.GetMasterVolumeLevelScalar().ok()?;
        let muted = ep.GetMute().ok()?.as_bool();
        Some(VolumeInfo {
            percent: (vol * 100.0).round() as u8,
            muted,
        })
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_volume() -> Option<VolumeInfo> {
    None
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_volume(percent: u8) -> bool {
    let p = (percent.min(100) as f32) / 100.0;
    unsafe {
        let ep = match endpoint() {
            Some(v) => v,
            None => return false,
        };
        ep.SetMasterVolumeLevelScalar(p, std::ptr::null()).is_ok()
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_volume(_percent: u8) -> bool {
    false
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_mute(muted: bool) -> bool {
    unsafe {
        let ep = match endpoint() {
            Some(v) => v,
            None => return false,
        };
        ep.SetMute(muted, std::ptr::null()).is_ok()
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_mute(_muted: bool) -> bool {
    false
}
