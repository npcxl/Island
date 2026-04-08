use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ThemeModeInfo {
    pub mode: String, // "light" | "dark" | "unknown"
}

#[cfg(target_os = "windows")]
fn read_apps_use_light_theme() -> Option<u32> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    use windows::core::PCWSTR;

    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value: Vec<u16> = "AppsUseLightTheme"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut data: u32 = 0;
        let mut cb: u32 = std::mem::size_of::<u32>() as u32;
        // RegGetValueW 会自己打开子键，无需 RegOpenKeyEx
        let st = RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut cb),
        );
        if st.is_ok() { Some(data) } else { None }
    }
}

#[cfg(target_os = "windows")]
fn set_personalize_dword(name: &str, val: u32) -> bool {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyW, RegSetValueExW, HKEY_CURRENT_USER, HKEY, REG_DWORD,
    };
    use windows::core::PCWSTR;

    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut hkey: HKEY = HKEY::default();
        if RegCreateKeyW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), &mut hkey).is_err() {
            return false;
        }
        let bytes = val.to_le_bytes();
        let st2 = RegSetValueExW(
            hkey,
            PCWSTR(value.as_ptr()),
            Some(0),
            REG_DWORD,
            Some(&bytes),
        );
        let _ = RegCloseKey(hkey);
        st2.is_ok()
    }
}

#[cfg(target_os = "windows")]
fn broadcast_theme_change() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    // lParam = "ImmersiveColorSet" 常用于通知主题/颜色变化
    let s: Vec<u16> = "ImmersiveColorSet".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(s.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_theme_mode() -> ThemeModeInfo {
    let v = read_apps_use_light_theme().unwrap_or(1);
    ThemeModeInfo {
        mode: if v == 0 { "dark" } else { "light" }.to_string(),
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_theme_mode() -> ThemeModeInfo {
    ThemeModeInfo { mode: "unknown".to_string() }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_theme_mode(mode: String) -> bool {
    let light = match mode.as_str() {
        "light" => true,
        "dark" => false,
        _ => return false,
    };
    let v = if light { 1u32 } else { 0u32 };

    // AppsUseLightTheme + SystemUsesLightTheme 同步设置
    let ok1 = set_personalize_dword("AppsUseLightTheme", v);
    let ok2 = set_personalize_dword("SystemUsesLightTheme", v);
    if ok1 || ok2 {
        broadcast_theme_change();
        return true;
    }
    false
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_theme_mode(_mode: String) -> bool {
    false
}

