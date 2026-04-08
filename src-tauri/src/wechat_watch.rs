//! 新版微信（Weixin.exe、单进程、主窗标题固定「微信」）无法依赖传统 UIA 控件树。
//!
//! **已实现（与方案 2/3 对齐）**
//! - 轮询 `EnumWindows`：仅 **可见顶层窗**（非 WS_CHILD）+ 进程映像为 `Weixin.exe` / `WeChat.exe`；
//! - 用 **窗口标题关键字** 识别通话/来电相关文案（如「语音通话」「视频通话」「邀请你」等）。
//!
//! **系统通知**：`wechat_notifications.rs` 仅推送 **非微信** 的 Toast（QQ 等）；微信 **只走本文件窗口枚举**。

#[cfg(target_os = "windows")]
mod win {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, BOOL};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        GetWindowLongPtrW, GWL_STYLE, WS_CHILD,
    };

    #[derive(Debug, Clone)]
    pub struct WeChatHint {
        /// "window_enum"
        pub source: String,
        pub title: String,
        pub class_name: String,
        /// "call" | "message"
        pub kind: String,
        pub body: String,
    }

    struct EnumCtx {
        hints: Vec<WeChatHint>,
    }

    fn exe_for_hwnd(hwnd: HWND) -> Option<String> {
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)); }
        if pid == 0 {
            return None;
        }
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = vec![0u16; 520];
            let mut size: u32 = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                h,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(h);
            if !ok || size == 0 {
                return None;
            }
            let s = OsString::from_wide(&buf[..size as usize])
                .to_string_lossy()
                .to_string();
            Some(s)
        }
    }

    fn is_wechat_process(exe: &str) -> bool {
        let lower = exe.to_lowercase();
        let file = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
        file == "weixin.exe" || file == "wechat.exe" || file == "wechatappex.exe"
    }

    /// 来电/通话相关标题子串（不依赖控件树）
    fn classify_wechat_title(title: &str) -> Option<&'static str> {
        let t = title.trim();
        if t.is_empty() {
            return None;
        }
        let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        let lower = compact.to_lowercase();
        const CALL_MARKERS: &[&str] = &[
            "语音通话",
            "视频通话",
            "邀请你语音",
            "邀请你视频",
            "来电",
            "正在呼叫",
            "请求视频",
            "请求语音",
            "voicecall",
            "videocall",
            "voicechat",
        ];
        for m in CALL_MARKERS {
            if lower.contains(&m.to_lowercase()) || t.contains(m) {
                return Some("call");
            }
        }
        // 主窗带未读：「微信(2)」「微信（3）」等 — 单进程架构下这是少数能反映新消息的方式之一
        if t.starts_with("微信") && t != "微信" {
            return Some("message");
        }
        // 独立顶层窗：联系人名、群名、单独聊天窗（标题一般不是「微信」）
        if t.len() >= 2 && !t.starts_with("微信") {
            return Some("message");
        }
        None
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumCtx);

        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if style & WS_CHILD.0 as isize != 0 {
            return BOOL::from(true);
        }

        if !windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() {
            return BOOL::from(true);
        }

        let exe = match exe_for_hwnd(hwnd) {
            Some(e) => e,
            None => return BOOL::from(true),
        };
        if !is_wechat_process(&exe) {
            return BOOL::from(true);
        }

        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return BOOL::from(true);
        }
        let mut buf = vec![0u16; (len as usize) + 1];
        let _ = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..len as usize]);

        let mut cls_buf = vec![0u16; 256];
        let n = GetClassNameW(hwnd, &mut cls_buf) as usize;
        let class_name = if n > 0 {
            String::from_utf16_lossy(&cls_buf[..n])
        } else {
            String::new()
        };

        if let Some(kind) = classify_wechat_title(&title) {
            let body = if kind == "call" {
                "微信 · 通话/来电".to_string()
            } else {
                "微信 · 会话或未读".to_string()
            };
            ctx.hints.push(WeChatHint {
                source: "window_enum".to_string(),
                title: title.trim().to_string(),
                class_name,
                kind: kind.to_string(),
                body,
            });
        }

        BOOL::from(true)
    }

    /// 收集所有匹配窗后：优先通话；否则取「信息量最大」的会话提示（标题更长往往更有用）
    pub fn scan_top_level_wechat() -> Option<WeChatHint> {
        let mut ctx = EnumCtx { hints: Vec::new() };
        let ptr = &mut ctx as *mut EnumCtx as isize;
        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(ptr));
        }
        if ctx.hints.is_empty() {
            return None;
        }
        if let Some(h) = ctx.hints.iter().find(|h| h.kind == "call").cloned() {
            return Some(h);
        }
        ctx.hints
            .into_iter()
            .max_by_key(|h| h.title.len())
    }
}

#[cfg(target_os = "windows")]
pub use win::{scan_top_level_wechat, WeChatHint};

/// 防抖：同一签名在 `dedupe` 时间内不重复上报
#[cfg(target_os = "windows")]
pub struct WeChatDebounce {
    last: Option<(String, std::time::Instant)>,
    dedupe: std::time::Duration,
}

#[cfg(target_os = "windows")]
impl WeChatDebounce {
    pub fn new(dedupe: std::time::Duration) -> Self {
        Self { last: None, dedupe }
    }

    pub fn try_emit(&mut self, hint: WeChatHint) -> Option<WeChatHint> {
        // 系统通知：同一人连发多条时 title 常相同，必须把 body 纳入签名，否则会整段被 10s 防抖吞掉
        let sig = if hint.source == "user_notification" {
            format!("{}|{}|{}|{}", hint.kind, hint.title, hint.body, hint.source)
        } else {
            format!("{}|{}|{}", hint.kind, hint.title, hint.source)
        };
        if let Some((s, t)) = &self.last {
            if s == &sig && t.elapsed() < self.dedupe {
                return None;
            }
        }
        self.last = Some((sig, std::time::Instant::now()));
        Some(hint)
    }
}

#[cfg(not(target_os = "windows"))]
pub struct WeChatDebounce;
#[cfg(not(target_os = "windows"))]
impl WeChatDebounce {
    pub fn new(_: std::time::Duration) -> Self {
        Self
    }
}
