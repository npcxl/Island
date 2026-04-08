#[cfg(target_os = "windows")]
fn send_combo(vks: &[u16]) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };

    let mut inputs: Vec<INPUT> = Vec::new();
    let make = |vk: u16, flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    for &vk in vks {
        inputs.push(make(vk, KEYBD_EVENT_FLAGS(0)));
    }
    for &vk in vks.iter().rev() {
        inputs.push(make(vk, KEYEVENTF_KEYUP));
    }

    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn screenshot() {
    // Win + Shift + S
    const VK_LWIN: u16 = 0x5B;
    const VK_SHIFT: u16 = 0x10;
    const VK_S: u16 = 0x53;
    send_combo(&[VK_LWIN, VK_SHIFT, VK_S]);
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn screenshot() {}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn open_quick_settings() {
    // Win + A (Win11: 快速设置；Win10: 操作中心)
    const VK_LWIN: u16 = 0x5B;
    const VK_A: u16 = 0x41;
    send_combo(&[VK_LWIN, VK_A]);
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn open_quick_settings() {}

/// 用系统默认程序打开 URL / 协议（收藏夹打开网页等）
pub fn open_external_url(target: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        shell_execute_open(target)
    }
    #[cfg(not(target_os = "windows"))]
    {
        match std::process::Command::new("xdg-open").arg(target).spawn() {
            Ok(mut c) => c.wait().map(|s| s.success()).unwrap_or(false),
            Err(_) => false,
        }
    }
}

#[cfg(target_os = "windows")]
fn shell_execute_open(target: &str) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let op: Vec<u16> = OsStr::new("open").encode_wide().chain(std::iter::once(0)).collect();
    let u: Vec<u16> = OsStr::new(target).encode_wide().chain(std::iter::once(0)).collect();

    unsafe {
        let h = ShellExecuteW(
            None,
            windows::core::PCWSTR(op.as_ptr()),
            windows::core::PCWSTR(u.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
        (h.0 as isize) > 32
    }
}

#[cfg(target_os = "windows")]
fn expand_env_path(raw: &str) -> std::path::PathBuf {
    let mut out = raw.to_string();
    if let Ok(v) = std::env::var("LOCALAPPDATA") {
        out = out.replace("%LOCALAPPDATA%", &v);
    }
    if let Ok(v) = std::env::var("ProgramFiles") {
        out = out.replace("%ProgramFiles%", &v);
    }
    if let Ok(v) = std::env::var("ProgramFiles(x86)") {
        out = out.replace("%ProgramFiles(x86)%", &v);
    }
    std::path::PathBuf::from(out)
}

#[cfg(target_os = "windows")]
fn try_open_first_existing_exe(candidates: &[&str]) -> bool {
    for c in candidates {
        let p = expand_env_path(c);
        if p.is_file() {
            if let Some(s) = p.to_str() {
                if shell_execute_open(s) {
                    return true;
                }
            }
        }
    }
    false
}

/// 根据当前媒体 `source` 唤起对应播放器（协议 / 常见安装路径）
#[cfg(target_os = "windows")]
#[tauri::command]
pub fn open_now_playing_app(source: String) -> bool {
    let s = source.to_lowercase();
    match s.as_str() {
        "spotify" => {
            shell_execute_open("spotify:")
                || try_open_first_existing_exe(&[r"%LOCALAPPDATA%\Microsoft\WindowsApps\Spotify.exe"])
        }
        "cloudmusic" => {
            shell_execute_open("orpheus://")
                || shell_execute_open("cloudmusic://")
                || try_open_first_existing_exe(&[
                    r"%LOCALAPPDATA%\NetEase\CloudMusic\cloudmusic.exe",
                    r"%ProgramFiles%\CloudMusic\cloudmusic.exe",
                    r"%ProgramFiles(x86)%\CloudMusic\cloudmusic.exe",
                    r"%ProgramFiles%\NetEase\CloudMusic\cloudmusic.exe",
                ])
        }
        "qqmusic" => {
            shell_execute_open("qqmusic://")
                || try_open_first_existing_exe(&[
                    r"%ProgramFiles%\Tencent\QQMusic\QQMusic.exe",
                    r"%ProgramFiles(x86)%\Tencent\QQMusic\QQMusic.exe",
                ])
        }
        "chrome" => shell_execute_open("chrome://newtab"),
        _ => false,
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn open_now_playing_app(_source: String) -> bool {
    false
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn open_system_uri(uri: String) -> bool {
    shell_execute_open(&uri)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn open_system_uri(_uri: String) -> bool {
    false
}

