//! Cursor / VS Code 前台跟随：窗口标题 + UI Automation 尽力抓取对话/状态文案；
//! 窗口最小化后仍跟踪最后一次前台时的 HWND，继续轮询 UIA（Electron 可能仍可读）。

use serde::Serialize;
use tauri::AppHandle;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct IdeFocusBarPayload {
    /// `cursor` | `vscode`
    pub app_id: String,
    pub display_name: String,
    /// 去掉 ` - Cursor` 等后缀后的标题（多为文件/工作区）
    pub window_title: String,
    /// 从无障碍树抓到的较长文案（对话片段、提示等），尽力而为
    pub context_line: Option<String>,
    /// 检测到「运行 / 接受 / 允许」等按钮名时置 true，提示需切回确认
    pub needs_attention: bool,
    /// 当前是否处于「IDE 已最小化、岛在托底」状态
    pub minimized: bool,
}

#[cfg(target_os = "windows")]
mod win {
    use super::IdeFocusBarPayload;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use tauri::Emitter;
    use windows::core::Interface;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        IUIAutomationValuePattern, UIA_ButtonControlTypeId, UIA_ValuePatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, SetForegroundWindow,
        ShowWindow, SW_RESTORE,
    };

    static LAST_EMITTED: OnceLock<Mutex<Option<IdeFocusBarPayload>>> = OnceLock::new();
    /// 最近一次在前台的 IDE 窗口，用于最小化后继续托底
    static LAST_IDE_HWND: Mutex<Option<isize>> = Mutex::new(None);

    struct SurfaceCache {
        hwnd: isize,
        at: Instant,
        context_line: Option<String>,
        needs_attention: bool,
    }
    static SURFACE_CACHE: Mutex<Option<SurfaceCache>> = Mutex::new(None);

    const SCRAPE_MIN_INTERVAL: Duration = Duration::from_millis(520);
    const WALK_MAX_DEPTH: usize = 48;
    const WALK_MAX_NODES: usize = 650;

    fn last_slot() -> &'static Mutex<Option<IdeFocusBarPayload>> {
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

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum IdeKind {
        Cursor,
        VsCode,
    }

    fn classify_exe(name: &str) -> Option<IdeKind> {
        match name {
            "cursor.exe" => Some(IdeKind::Cursor),
            "code.exe" => Some(IdeKind::VsCode),
            _ => None,
        }
    }

    fn ide_kind_label(k: IdeKind) -> (&'static str, &'static str) {
        match k {
            IdeKind::Cursor => ("cursor", "Cursor"),
            IdeKind::VsCode => ("vscode", "VS Code"),
        }
    }

    unsafe fn hwnd_pid(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid
    }

    unsafe fn is_ide_hwnd(hwnd: HWND) -> Option<IdeKind> {
        if hwnd.is_invalid() || !IsWindow(Some(hwnd)).as_bool() {
            return None;
        }
        let pid = hwnd_pid(hwnd);
        let exe = exe_base_name_lower(pid)?;
        classify_exe(&exe)
    }

    fn strip_ide_title(title: &str, kind: IdeKind) -> String {
        const SUFFIXES: &[&str] = &[
            " - Cursor",
            " – Cursor",
            " - Visual Studio Code",
            " – Visual Studio Code",
            " - Code",
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
            match kind {
                IdeKind::Cursor => "Cursor".to_string(),
                IdeKind::VsCode => "VS Code".to_string(),
            }
        } else {
            t.to_string()
        }
    }

    unsafe fn read_window_title(hwnd: HWND) -> String {
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf) as usize;
        if n == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..n])
        }
    }

    fn name_hints_attention(raw: &str) -> bool {
        let t = raw.to_ascii_lowercase();
        t.contains("accept")
            || t.contains("allow")
            || t.contains("approve")
            || t.contains("run command")
            || t.contains("reject")
            || t.contains("deny")
            || raw.contains("运行")
            || raw.contains("接受")
            || raw.contains("允许")
            || raw.contains("确认")
            || raw.contains("拒绝")
            || raw.contains("批准")
    }

    fn is_junk_context(s: &str, window_title_stripped: &str) -> bool {
        let t = s.trim();
        if t.len() < 18 || t.len() > 3600 {
            return true;
        }
        if t == window_title_stripped {
            return true;
        }
        let tl = t.to_ascii_lowercase();
        if tl == "cursor" || tl == "visual studio code" || tl == "welcome" {
            return true;
        }
        if tl.starts_with("open ") && tl.len() < 30 {
            return true;
        }
        false
    }

    fn pick_context_line(cands: &[String], window_title_stripped: &str) -> Option<String> {
        let mut best: Option<&str> = None;
        let mut best_len = 0usize;
        for s in cands {
            let tr = s.trim();
            if is_junk_context(tr, window_title_stripped) {
                continue;
            }
            if tr.len() > best_len {
                best_len = tr.len();
                best = Some(tr);
            }
        }
        best.map(|s| {
            let mut o = s.to_string();
            const MAX: usize = 160;
            if o.chars().count() > MAX {
                let t: String = o.chars().take(MAX - 1).collect();
                o = format!("{t}…");
            }
            o
        })
    }

    unsafe fn walk_ide_surface(
        elem: &IUIAutomationElement,
        walker: &IUIAutomationTreeWalker,
        depth: usize,
        window_title_stripped: &str,
        cands: &mut Vec<String>,
        needs_attention: &mut bool,
        nodes: &mut usize,
    ) {
        if depth > WALK_MAX_DEPTH || *nodes >= WALK_MAX_NODES {
            return;
        }
        *nodes += 1;

        let name = elem
            .CurrentName()
            .ok()
            .map(|b| bstr_to_string(&b))
            .unwrap_or_default();
        let name_trim = name.trim();
        if !name_trim.is_empty() {
            if let Ok(ct) = elem.CurrentControlType() {
                if ct == UIA_ButtonControlTypeId && name_hints_attention(name_trim) {
                    *needs_attention = true;
                }
            }
            if name_trim.len() >= 22 && !is_junk_context(name_trim, window_title_stripped) {
                cands.push(name_trim.to_string());
            }
        }

        if let Ok(vp) = elem.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
            if let Ok(b) = vp.CurrentValue() {
                let v = bstr_to_string(&b);
                let vt = v.trim();
                if vt.len() >= 22 {
                    cands.push(vt.to_string());
                }
            }
        }

        let Ok(child) = walker.GetFirstChildElement(elem) else {
            return;
        };
        let mut cur = child;
        loop {
            walk_ide_surface(
                &cur,
                walker,
                depth + 1,
                window_title_stripped,
                cands,
                needs_attention,
                nodes,
            );
            let Ok(next) = walker.GetNextSiblingElement(&cur) else {
                break;
            };
            cur = next;
        }
    }

    unsafe fn scrape_surface(hwnd: HWND, window_title_stripped: &str) -> (Option<String>, bool) {
        let hwnd_key = hwnd.0 as isize;
        let now = Instant::now();
        if let Ok(g) = SURFACE_CACHE.lock() {
            if let Some(c) = g.as_ref() {
                if c.hwnd == hwnd_key && now.duration_since(c.at) < SCRAPE_MIN_INTERVAL {
                    return (c.context_line.clone(), c.needs_attention);
                }
            }
        }

        let mut cands: Vec<String> = Vec::new();
        let mut needs = false;
        let mut node_count = 0usize;

        let scraped = (|| {
            let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let root = auto.ElementFromHandle(hwnd).ok()?;
            let walker = auto.ControlViewWalker().ok()?;
            walk_ide_surface(
                &root,
                &walker,
                0,
                window_title_stripped,
                &mut cands,
                &mut needs,
                &mut node_count,
            );
            Some(())
        })();

        let ctx = if scraped.is_some() {
            pick_context_line(&cands, window_title_stripped)
        } else {
            None
        };

        if let Ok(mut g) = SURFACE_CACHE.lock() {
            *g = Some(SurfaceCache {
                hwnd: hwnd_key,
                at: now,
                context_line: ctx.clone(),
                needs_attention: needs,
            });
        }

        (ctx, needs)
    }

    unsafe fn build_for_hwnd(hwnd: HWND, kind: IdeKind, minimized: bool) -> IdeFocusBarPayload {
        let full_title = read_window_title(hwnd);
        let stripped = strip_ide_title(&full_title, kind);
        let (app_id, display) = ide_kind_label(kind);
        let (context_line, mut needs_attention) = scrape_surface(hwnd, &stripped);
        if name_hints_attention(&full_title) {
            needs_attention = true;
        }
        IdeFocusBarPayload {
            app_id: app_id.to_string(),
            display_name: display.to_string(),
            window_title: stripped,
            context_line,
            needs_attention,
            minimized,
        }
    }

    pub unsafe fn poll_payload() -> Option<IdeFocusBarPayload> {
        let fg = GetForegroundWindow();
        if let Some(kind) = is_ide_hwnd(fg) {
            if let Ok(mut g) = LAST_IDE_HWND.lock() {
                *g = Some(fg.0 as isize);
            }
            return Some(build_for_hwnd(fg, kind, false));
        }

        let tracked = LAST_IDE_HWND.lock().ok().and_then(|g| *g)?;
        let hwnd = HWND(tracked as *mut core::ffi::c_void);
        if !IsWindow(Some(hwnd)).as_bool() {
            if let Ok(mut g) = LAST_IDE_HWND.lock() {
                *g = None;
            }
            return None;
        }
        if is_ide_hwnd(hwnd).is_none() {
            if let Ok(mut g) = LAST_IDE_HWND.lock() {
                *g = None;
            }
            return None;
        }
        if !IsIconic(hwnd).as_bool() {
            // 未最小化但已失焦：不占用岛（用户能在后台看到窗口）
            return None;
        }
        let kind = is_ide_hwnd(hwnd)?;
        Some(build_for_hwnd(hwnd, kind, true))
    }

    pub fn emit_if_changed(app: &AppHandle) {
        let next = unsafe { poll_payload() };
        let mut g = last_slot().lock().unwrap_or_else(|e| e.into_inner());
        if next == *g {
            return;
        }
        *g = next.clone();
        let _ = app.emit("ide-focus", next);
    }

    pub fn focus_tracked_ide() -> bool {
        unsafe {
            let tracked = match LAST_IDE_HWND.lock() {
                Ok(g) => *g,
                Err(e) => *e.into_inner(),
            };
            let Some(t) = tracked else {
                return false;
            };
            let hwnd = HWND(t as *mut core::ffi::c_void);
            if !IsWindow(Some(hwnd)).as_bool() {
                return false;
            }
            let _ = ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd).as_bool()
        }
    }
}

#[cfg(target_os = "windows")]
pub fn emit_ide_focus_if_changed(app: &AppHandle) {
    win::emit_if_changed(app);
}

#[cfg(not(target_os = "windows"))]
pub fn emit_ide_focus_if_changed(_app: &AppHandle) {}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn focus_tracked_ide() -> bool {
    win::focus_tracked_ide()
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub fn focus_tracked_ide() -> bool {
    false
}
