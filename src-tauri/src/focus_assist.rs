use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FocusAssistInfo {
    pub supported: bool,
    pub on: Option<bool>,
}

// 免打扰/专注（Focus Assist）在不同 Windows 版本/策略下实现差异较大。
// 第一版采用 best-effort：当前先提供“不可直接切换”的返回，让前端走点击打开设置兜底。

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_focus_assist() -> FocusAssistInfo {
    FocusAssistInfo {
        supported: false,
        on: None,
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn get_focus_assist() -> FocusAssistInfo {
    FocusAssistInfo {
        supported: false,
        on: None,
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_focus_assist(_on: bool) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_focus_assist(_on: bool) -> bool {
    false
}

