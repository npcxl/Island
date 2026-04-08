#[cfg(target_os = "windows")]
fn argb_to_hex(argb: u32) -> String {
    let r = ((argb >> 16) & 0xff) as u8;
    let g = ((argb >> 8) & 0xff) as u8;
    let b = (argb & 0xff) as u8;
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_accent_color() -> Option<String> {
    use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;

    unsafe {
        let mut color: u32 = 0;
        let mut opaque: windows::Win32::Foundation::BOOL = windows::Win32::Foundation::BOOL(0);
        if DwmGetColorizationColor(&mut color, &mut opaque).is_ok() {
            return Some(argb_to_hex(color));
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_accent_color() -> Option<String> {
    None
}

