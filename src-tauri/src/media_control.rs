/// 模拟系统媒体键（SendInput）
/// action: "play_pause" | "next" | "prev"
#[cfg(target_os = "windows")]
#[tauri::command]
pub fn media_control(action: &str) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    // 媒体虚拟键码
    let vk: u16 = match action {
        "play_pause" => 0xB3, // VK_MEDIA_PLAY_PAUSE
        "next"       => 0xB0, // VK_MEDIA_NEXT_TRACK
        "prev"       => 0xB1, // VK_MEDIA_PREV_TRACK
        _ => return,
    };

    let make_input = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: KEYEVENTF_EXTENDEDKEY | flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let inputs = [
        make_input(KEYBD_EVENT_FLAGS(0)),       // key down
        make_input(KEYEVENTF_KEYUP),            // key up
    ];

    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn media_control(_action: &str) {}
