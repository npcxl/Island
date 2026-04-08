//! Chrome 前台跟随：用窗口标题表示当前页签，WASAPI 会话检测/静音。

use serde::Serialize;
use tauri::AppHandle;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ChromeFocusBarPayload {
    pub page_title: String,
    /// UI 自动化读地址栏（尽力而为）；Chrome 版本/语言不同可能为空
    pub url: Option<String>,
    pub has_sessions: bool,
    pub audible: bool,
    pub chrome_muted: bool,
}

#[cfg(target_os = "windows")]
mod win {
    use super::ChromeFocusBarPayload;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter};
    use windows::core::Interface;
    use windows::Win32::Foundation::{CloseHandle, GlobalFree, HANDLE, HWND, S_OK};
    use windows::Win32::Media::Audio::{
        AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        MMDeviceEnumerator, eConsole, eRender,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker, IUIAutomationValuePattern,
        UIA_ValuePatternId,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    static LAST_EMITTED: OnceLock<Mutex<Option<ChromeFocusBarPayload>>> = OnceLock::new();

    struct ChromeUrlCache {
        url: Option<String>,
        last_uia_at: Option<Instant>,
        last_hwnd: Option<isize>,
        /// 上次用于拉取地址栏的标题；标题变说明可能已换页，必须重读 UIA，避免沿用错误域名
        last_title: Option<String>,
    }
    static URL_CACHE: Mutex<ChromeUrlCache> = Mutex::new(ChromeUrlCache {
        url: None,
        last_uia_at: None,
        last_hwnd: None,
        last_title: None,
    });

    fn last_slot() -> &'static Mutex<Option<ChromeFocusBarPayload>> {
        LAST_EMITTED.get_or_init(|| Mutex::new(None))
    }

    fn bstr_to_string(b: &windows::core::BSTR) -> String {
        if b.is_empty() {
            return String::new();
        }
        unsafe {
            let p = b.as_ptr();
            if p.is_null() {
                return String::new();
            }
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
                if len > 16384 {
                    break;
                }
            }
            let sl = std::slice::from_raw_parts(p, len);
            String::from_utf16_lossy(sl)
        }
    }

    fn score_url_candidate(s: &str) -> Option<u32> {
        let t = s.trim();
        if t.len() < 4 || t.len() > 8192 {
            return None;
        }
        let tl = t.to_ascii_lowercase();
        if tl.contains("search google")
            || tl.contains("type a url")
            || tl.contains("google 搜索")
            || tl.contains("search or type web address")
        {
            return None;
        }
        if t.starts_with("https://") {
            return Some(2000 + t.len().min(4096) as u32);
        }
        if t.starts_with("http://") {
            return Some(1500 + t.len().min(4096) as u32);
        }
        if t.starts_with("chrome://")
            || t.starts_with("edge://")
            || t.starts_with("about:")
            || t.starts_with("file://")
            || t.starts_with("devtools://")
        {
            return Some(1200 + t.len() as u32);
        }
        // 无 scheme 的「像域名」串容易来自页面内链接/书签栏，分值压低，避免压过真地址栏
        if (t.starts_with("www.") || tl.contains("://"))
            && t.contains('/')
            && !t.contains(' ')
            && t.chars().filter(|c| *c == '.').count() >= 1
        {
            return Some(120 + t.len().min(512) as u32);
        }
        None
    }

    /// 明显不是地址栏、而是搜索建议等控件上的「像 URL」文本，整段丢弃
    fn should_skip_url_value_field(elem: &IUIAutomationElement) -> bool {
        let name = unsafe {
            elem.CurrentName()
                .ok()
                .map(|b| bstr_to_string(&b))
                .unwrap_or_default()
        };
        let nl = name.to_ascii_lowercase();
        nl.contains("suggestion")
            || nl.contains("search suggestions")
            || nl.contains("推荐")
            || nl.contains("联想")
            || nl.contains("autocomplete")
    }

    /// Chromium 地址栏常见可访问名称（IDS_ACCNAME_LOCATION 等），命中则大幅提高权重
    fn omnibox_accessibility_bonus(elem: &IUIAutomationElement) -> u32 {
        let (name, class) = unsafe {
            let name = elem
                .CurrentName()
                .ok()
                .map(|b| bstr_to_string(&b))
                .unwrap_or_default();
            let class = elem
                .CurrentClassName()
                .ok()
                .map(|b| bstr_to_string(&b))
                .unwrap_or_default();
            (name, class)
        };
        let nl = name.to_ascii_lowercase();
        let cl = class.to_ascii_lowercase();

        let mut b = 0u32;
        if nl.contains("address") && (nl.contains("search") || nl.contains("bar")) {
            b += 14_000;
        } else if nl.contains("address and search") {
            b += 14_000;
        } else if nl.contains("网址")
            || nl.contains("地址栏")
            || nl.contains("地址和搜索")
            || nl.contains("地址和搜索栏")
        {
            b += 12_000;
        } else if nl.contains("address") {
            b += 9_000;
        } else if nl.contains("omnibox") {
            b += 10_000;
        }
        if cl.contains("omnibox") {
            b += 8_000;
        }
        if nl.contains("adresse") || nl.contains("dirección") || nl.contains("direccion") {
            b += 7_000;
        }
        b
    }

    /// 同一分数带里更浅的节点更像工具栏上的地址栏，深层多是页面可访问树里的链接
    fn depth_shallower_bonus(depth: usize) -> u32 {
        let d = depth.min(40) as u32;
        (40u32.saturating_sub(d)).saturating_mul(14)
    }

    /// Google 跳转链接 ?q= 展开为真实目标 URL，避免岛条显示成 google.com/url
    fn unwrap_google_redirect_url(s: &str) -> Option<String> {
        let t = s.trim();
        let lower = t.to_ascii_lowercase();
        if !lower.contains("google.") || !lower.contains("/url") {
            return None;
        }
        let q_part = t.split_once('?')?.1;
        for seg in q_part.split('&') {
            let Some(encoded) = seg.strip_prefix("q=") else {
                continue;
            };
            let Ok(decoded) = urlencoding::decode(encoded) else {
                continue;
            };
            let out = decoded.trim().to_string();
            if out.starts_with("http://") || out.starts_with("https://") {
                return Some(out);
            }
        }
        None
    }

    fn postprocess_chrome_url(s: Option<String>) -> Option<String> {
        let s = s?;
        if let Some(inner) = unwrap_google_redirect_url(&s) {
            return Some(inner);
        }
        Some(s)
    }

    unsafe fn walk_for_url(
        elem: &IUIAutomationElement,
        walker: &IUIAutomationTreeWalker,
        depth: usize,
        best: &mut Option<(u32, String)>,
    ) {
        if depth > 42 {
            return;
        }
        if let Ok(vp) = elem.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
            if let Ok(b) = vp.CurrentValue() {
                let s = bstr_to_string(&b);
                if !should_skip_url_value_field(elem) {
                    if let Some(base) = score_url_candidate(&s) {
                        let bonus = omnibox_accessibility_bonus(elem);
                        let dbonus = depth_shallower_bonus(depth);
                        let score = base.saturating_add(bonus).saturating_add(dbonus);
                        let replace = match best {
                            None => true,
                            Some((sc, cur)) => {
                                score > *sc
                                    || (score == *sc && s.len() < cur.len())
                                    // 同分优先更短 https，减少误选超长跟踪参数
                                    || (score == *sc && s.len() == cur.len() && s < *cur)
                            }
                        };
                        if replace {
                            *best = Some((score, s));
                        }
                    }
                }
            }
        }
        let Ok(child) = walker.GetFirstChildElement(elem) else {
            return;
        };
        let mut cur = child;
        loop {
            walk_for_url(&cur, walker, depth + 1, best);
            let Ok(next) = walker.GetNextSiblingElement(&cur) else {
                break;
            };
            cur = next;
        }
    }

    unsafe fn try_read_chrome_omnibox_url(hwnd: HWND) -> Option<String> {
        let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let root = auto.ElementFromHandle(hwnd).ok()?;
        let walker = auto.ControlViewWalker().ok()?;
        let mut best: Option<(u32, String)> = None;
        walk_for_url(&root, &walker, 0, &mut best);
        best.map(|(_, s)| s)
    }

    fn cached_chrome_url(hwnd: HWND, page_title: &str) -> Option<String> {
        const UIA_INTERVAL: Duration = Duration::from_millis(480);
        let mut c = URL_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let hwnd_key = hwnd.0 as isize;
        let title_changed = c.last_title.as_deref() != Some(page_title);
        let stale_time = c
            .last_uia_at
            .map(|t| now.duration_since(t) >= UIA_INTERVAL)
            .unwrap_or(true);
        let need = c.last_hwnd != Some(hwnd_key) || stale_time || title_changed;
        if title_changed {
            c.last_title = Some(page_title.to_string());
        }
        if need {
            let raw = unsafe { try_read_chrome_omnibox_url(hwnd) };
            c.url = postprocess_chrome_url(raw);
            c.last_uia_at = Some(now);
            c.last_hwnd = Some(hwnd_key);
        }
        postprocess_chrome_url(c.url.clone())
    }

    pub fn copy_text_to_clipboard(text: &str) -> bool {
        const CF_UNICODETEXT: u32 = 13;
        unsafe {
            if OpenClipboard(None).is_err() {
                return false;
            }
            let _ = EmptyClipboard();
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let byte_len = wide.len() * core::mem::size_of::<u16>();
            let hglobal = match GlobalAlloc(GMEM_MOVEABLE, byte_len) {
                Ok(h) => h,
                Err(_) => {
                    let _ = CloseClipboard();
                    return false;
                }
            };
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                let _ = GlobalFree(Some(hglobal));
                let _ = CloseClipboard();
                return false;
            }
            core::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            let _ = GlobalUnlock(hglobal);
            if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hglobal.0))).is_err() {
                let _ = GlobalFree(Some(hglobal));
                let _ = CloseClipboard();
                return false;
            }
            let _ = CloseClipboard();
            true
        }
    }

    fn strip_chrome_suffix(title: &str) -> String {
        const SUFFIXES: &[&str] = &[
            " - Google Chrome",
            " – Google Chrome",
            " - Chromium",
            " - Google Chrome Canary",
        ];
        let mut s = title.to_string();
        for suf in SUFFIXES {
            if let Some(i) = s.rfind(suf) {
                s.truncate(i);
                break;
            }
        }
        let t = s.trim();
        if t.is_empty() {
            "Chrome".to_string()
        } else {
            t.to_string()
        }
    }

    unsafe fn exe_base_name_lower(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 520];
        let mut sz = buf.len() as u32;
        let pw = windows::core::PWSTR(buf.as_mut_ptr());
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, pw, &mut sz);
        let _ = CloseHandle(h);
        if r.is_err() || sz == 0 {
            return None;
        }
        buf.truncate(sz as usize);
        let wide = String::from_utf16_lossy(&buf);
        wide
            .rsplit(['\\', '/'])
            .next()
            .map(|s| s.to_ascii_lowercase())
    }

    fn is_chrome_exe(name: &str) -> bool {
        name == "chrome.exe" || name == "chromium.exe"
    }

    unsafe fn foreground_chrome_hwnd_title() -> Option<(HWND, String)> {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let exe = exe_base_name_lower(pid)?;
        if !is_chrome_exe(&exe) {
            return None;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf) as usize;
        let title = if n == 0 {
            "Chrome".to_string()
        } else {
            strip_chrome_suffix(&String::from_utf16_lossy(&buf[..n]))
        };
        Some((hwnd, title))
    }

    unsafe fn session_manager2() -> Option<IAudioSessionManager2> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        device.Activate(CLSCTX_ALL, None).ok()
    }

    /// (has_sessions, audible, chrome_muted)
    unsafe fn chrome_audio_aggregate(sm: &IAudioSessionManager2) -> (bool, bool, bool) {
        let Ok(enum_) = sm.GetSessionEnumerator() else {
            return (false, false, false);
        };
        let Ok(n) = enum_.GetCount() else {
            return (false, false, false);
        };
        let mut has_sessions = false;
        let mut audible = false;
        let mut any_unmuted = false;
        for i in 0..n {
            let Ok(sc) = enum_.GetSession(i) else {
                continue;
            };
            let Ok(sc2) = sc.cast::<IAudioSessionControl2>() else {
                continue;
            };
            if sc2.IsSystemSoundsSession() == S_OK {
                continue;
            }
            let Ok(pid) = sc2.GetProcessId() else {
                continue;
            };
            let Some(exe) = exe_base_name_lower(pid) else {
                continue;
            };
            if !is_chrome_exe(&exe) {
                continue;
            }
            let Ok(guid) = sc2.GetGroupingParam() else {
                continue;
            };
            let Ok(vol) = sm.GetSimpleAudioVolume(Some(std::ptr::from_ref(&guid)), 0) else {
                continue;
            };
            has_sessions = true;
            let mute = vol.GetMute().ok().map(|b| b.as_bool()).unwrap_or(false);
            let level = vol.GetMasterVolume().unwrap_or(0.0);
            let state = sc2
                .GetState()
                .unwrap_or(windows::Win32::Media::Audio::AudioSessionStateInactive);
            if !mute {
                any_unmuted = true;
            }
            if state == AudioSessionStateActive && !mute && level > 0.001 {
                audible = true;
            }
        }
        let chrome_muted = has_sessions && !any_unmuted;
        (has_sessions, audible, chrome_muted)
    }

    pub unsafe fn poll_payload() -> Option<ChromeFocusBarPayload> {
        let (hwnd, page_title) = foreground_chrome_hwnd_title()?;
        let url = cached_chrome_url(hwnd, &page_title);
        let (has_sessions, audible, chrome_muted) = match session_manager2() {
            Some(sm) => chrome_audio_aggregate(&sm),
            None => (false, false, false),
        };
        Some(ChromeFocusBarPayload {
            page_title,
            url,
            has_sessions,
            audible,
            chrome_muted,
        })
    }

    pub fn emit_if_changed(app: &AppHandle) {
        let next = unsafe { poll_payload() };
        let mut g = last_slot()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if next == *g {
            return;
        }
        *g = next.clone();
        let _ = app.emit("chrome-focus", next);
    }

    pub unsafe fn set_sessions_mute(muted: bool) -> bool {
        let Some(sm) = session_manager2() else {
            return false;
        };
        let Ok(enum_) = sm.GetSessionEnumerator() else {
            return false;
        };
        let Ok(n) = enum_.GetCount() else {
            return false;
        };
        let mut seen: Vec<windows::core::GUID> = Vec::new();
        let mut ok = false;
        for i in 0..n {
            let Ok(sc) = enum_.GetSession(i) else {
                continue;
            };
            let Ok(sc2) = sc.cast::<IAudioSessionControl2>() else {
                continue;
            };
            if sc2.IsSystemSoundsSession() == S_OK {
                continue;
            }
            let Ok(pid) = sc2.GetProcessId() else {
                continue;
            };
            let Some(exe) = exe_base_name_lower(pid) else {
                continue;
            };
            if !is_chrome_exe(&exe) {
                continue;
            }
            let Ok(guid) = sc2.GetGroupingParam() else {
                continue;
            };
            if seen.iter().any(|x| *x == guid) {
                continue;
            }
            seen.push(guid);
            let gref = seen.last().unwrap();
            let Ok(vol) = sm.GetSimpleAudioVolume(Some(std::ptr::from_ref(gref)), 0) else {
                continue;
            };
            if vol
                .SetMute(muted.into(), std::ptr::null())
                .is_ok()
            {
                ok = true;
            }
        }
        ok
    }
}

#[cfg(target_os = "windows")]
pub fn emit_chrome_focus_if_changed(app: &AppHandle) {
    win::emit_if_changed(app);
}

#[cfg(not(target_os = "windows"))]
pub fn emit_chrome_focus_if_changed(_app: &AppHandle) {}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_chrome_sessions_mute(muted: bool) -> bool {
    unsafe { win::set_sessions_mute(muted) }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn set_chrome_sessions_mute(_muted: bool) -> bool {
    false
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn copy_text_to_clipboard(text: String) -> bool {
    win::copy_text_to_clipboard(&text)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn copy_text_to_clipboard(_text: String) -> bool {
    false
}
