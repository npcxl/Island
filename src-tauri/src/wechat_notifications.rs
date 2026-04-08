//! Windows 用户通知（`UserNotificationListener`）。
//! - **微信**：不在此模块推送（由 `wechat_watch` 窗口枚举负责，避免与 QQ 等腾讯系混淆）。
//! - **其它应用**（QQ、邮件、Teams 等）：`poll_non_wechat_toast` 从操作中心取 **非微信** 的新 Toast。
//!
//! 需在 **设置 → 隐私和安全性 → 通知** 中允许本应用访问通知。
//!
//! 调试：`ISLAND_DEBUG_WECHAT=1` 或 `ISLAND_LOG_NOTIFICATIONS=1`。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppToastHint {
    pub app_name: String,
    pub aumid: Option<String>,
    pub title: String,
    pub body: String,
    pub icon_base64: Option<String>,
}

#[cfg(target_os = "windows")]
mod win {
    use super::AppToastHint;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    use windows::ApplicationModel::AppInfo;
    use windows::Foundation::Size;
    use windows::Storage::Streams::DataReader;
    use windows::UI::Notifications::KnownNotificationBindings;
    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    use windows::UI::Notifications::{NotificationKinds, UserNotification};

    #[derive(Clone, Copy, Eq, PartialEq, Hash)]
    struct SeenNotif {
        id: u32,
        universal_time: i64,
    }

    struct Row {
        key: SeenNotif,
        app: Option<AppInfo>,
        title: String,
        body: String,
    }

    struct State {
        access_requested: bool,
        baselined: bool,
        seen: HashSet<SeenNotif>,
        logged_access: bool,
        last_notify_debug_empty: Option<std::time::Instant>,
    }

    static STATE: OnceLock<Mutex<State>> = OnceLock::new();

    fn state_mutex() -> &'static Mutex<State> {
        STATE.get_or_init(|| {
            Mutex::new(State {
                access_requested: false,
                baselined: false,
                seen: HashSet::new(),
                logged_access: false,
                last_notify_debug_empty: None,
            })
        })
    }

    fn debug_notifications() -> bool {
        let ok = |v: &str| v == "1" || v.eq_ignore_ascii_case("true") || v == "all";
        std::env::var("ISLAND_DEBUG_WECHAT")
            .map(|v| ok(&v))
            .unwrap_or(false)
            || std::env::var("ISLAND_LOG_NOTIFICATIONS")
                .map(|v| ok(&v))
                .unwrap_or(false)
    }

    fn preview_toast_text(un: &UserNotification) -> String {
        match toast_lines(un) {
            Some((t, b)) if !b.is_empty() => format!("{t} | {b}"),
            Some((t, _)) => t,
            None => String::from("(无 Toast 文本/绑定)"),
        }
    }

    fn dump_all_notifications(list: &windows::Foundation::Collections::IVectorView<UserNotification>) {
        let n = list.Size().unwrap_or(0);
        eprintln!("======== [Island notif-debug] 通知中心 Toast/Unknown 共 {n} 条 ========");
        for i in 0..n {
            let Ok(un) = list.GetAt(i) else {
                eprintln!("  [#{i}] GetAt 失败");
                continue;
            };
            let id = un.Id().unwrap_or(0);
            let ut = un
                .CreationTime()
                .map(|dt| dt.UniversalTime)
                .unwrap_or(0);
            let preview = preview_toast_text(&un);
            let preview_short: String = preview.chars().take(240).collect();
            let suffix = if preview.chars().count() > 240 { "…" } else { "" };

            match un.AppInfo() {
                Ok(app_info) => {
                    let aumid = app_info
                        .AppUserModelId()
                        .ok()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let disp = app_info
                        .DisplayInfo()
                        .ok()
                        .and_then(|d| d.DisplayName().ok().map(|s| s.to_string()))
                        .unwrap_or_default();
                    let pfn = app_info
                        .PackageFamilyName()
                        .ok()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let app_id = app_info
                        .Id()
                        .ok()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let is_wx = is_wechat_app(&app_info);
                    eprintln!("  [#{i}] id={id} creation_ticks={ut}");
                    eprintln!("       AppUserModelId: {aumid}");
                    eprintln!("       DisplayName:    {disp}");
                    eprintln!("       PackageFamily:  {pfn}");
                    eprintln!("       AppInfo.Id:     {app_id}");
                    eprintln!("       match_wechat:   {is_wx}");
                    eprintln!("       text_preview:   {preview_short}{suffix}");
                }
                Err(e) => {
                    eprintln!("  [#{i}] id={id} creation_ticks={ut}");
                    eprintln!("       AppInfo: **不可用** {e:?}");
                    eprintln!("       text_preview:   {preview_short}{suffix}");
                }
            }
        }
        eprintln!("======== [Island notif-debug] dump 结束 ========");
    }

    /// 仅识别 **微信**，不要用泛化 `tencent`（否则 QQ 等会被当成微信而不走 Toast）。
    fn is_wechat_app(info: &AppInfo) -> bool {
        let aumid = info
            .AppUserModelId()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default()
            .to_lowercase();
        let name = info
            .DisplayInfo()
            .ok()
            .and_then(|d| d.DisplayName().ok().map(|s| s.to_string()))
            .unwrap_or_default()
            .to_lowercase();
        let pfn = info
            .PackageFamilyName()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default()
            .to_lowercase();
        let app_id = info
            .Id()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default()
            .to_lowercase();
        aumid.contains("wechat")
            || aumid.contains("weixin")
            || aumid.contains("txwechat")
            || name.contains("微信")
            || name.contains("wechat")
            || name.contains("weixin")
            || pfn.contains("wechat")
            || pfn.contains("weixin")
            || app_id.contains("wechat")
            || app_id.contains("weixin")
    }

    fn lines_from_binding(binding: &windows::UI::Notifications::NotificationBinding) -> Vec<String> {
        let Ok(texts) = binding.GetTextElements() else {
            return Vec::new();
        };
        let count = texts.Size().unwrap_or(0);
        let mut lines: Vec<String> = Vec::new();
        for i in 0..count {
            if let Ok(t) = texts.GetAt(i) {
                if let Ok(s) = t.Text() {
                    let st = s.to_string();
                    if !st.trim().is_empty() {
                        lines.push(st);
                    }
                }
            }
        }
        lines
    }

    fn toast_lines(notification: &UserNotification) -> Option<(String, String)> {
        let n = notification.Notification().ok()?;
        let visual = n.Visual().ok()?;
        if let Ok(bind_name) = KnownNotificationBindings::ToastGeneric() {
            if let Ok(binding) = visual.GetBinding(&bind_name) {
                let lines = lines_from_binding(&binding);
                if !lines.is_empty() {
                    let title = lines[0].clone();
                    let body = if lines.len() > 1 {
                        lines[1..].join(" ")
                    } else {
                        String::new()
                    };
                    return Some((title, body));
                }
            }
        }
        if let Ok(bindings) = visual.Bindings() {
            let sz = bindings.Size().unwrap_or(0);
            for i in 0..sz {
                if let Ok(binding) = bindings.GetAt(i) {
                    let lines = lines_from_binding(&binding);
                    if !lines.is_empty() {
                        let title = lines[0].clone();
                        let body = if lines.len() > 1 {
                            lines[1..].join(" ")
                        } else {
                            String::new()
                        };
                        return Some((title, body));
                    }
                }
            }
        }
        None
    }

    /// 应用显示名：`DisplayName` → `Description` → `AppUserModelId` 尾部 → `PackageFamilyName`
    fn app_display_name(info: &AppInfo) -> String {
        if let Ok(d) = info.DisplayInfo() {
            if let Ok(n) = d.DisplayName() {
                let s = n.to_string();
                if !s.trim().is_empty() {
                    return s;
                }
            }
            if let Ok(desc) = d.Description() {
                let s = desc.to_string();
                if !s.trim().is_empty() && s.len() < 160 {
                    return s;
                }
            }
        }
        if let Ok(aumid) = info.AppUserModelId() {
            let s = aumid.to_string();
            if !s.is_empty() {
                let after_slash = s.rsplit('\\').next().unwrap_or(&s);
                let tail = after_slash.rsplit_once('!').map(|(_, t)| t).unwrap_or(after_slash);
                let tail = tail.trim();
                if !tail.is_empty() && tail.len() < 120 {
                    return tail.to_string();
                }
            }
        }
        if let Ok(pfn) = info.PackageFamilyName() {
            let s = pfn.to_string();
            if !s.trim().is_empty() {
                return s;
            }
        }
        "应用".to_string()
    }

    /// 从 `AppDisplayInfo::GetLogo` 读图标（多档边长，失败则 `None`）。部分 Win32 应用无清单图标会拿不到。
    fn try_app_logo_base64(info: &AppInfo) -> Option<String> {
        let disp = info.DisplayInfo().ok()?;
        // 单档优先：多档串行 OpenReadAsync 会明显拖慢 Toast 首帧
        for px in [48.0_f32] {
            let logo_ref = disp.GetLogo(Size { Width: px, Height: px }).ok()?;
            let stream = logo_ref.OpenReadAsync().ok()?.get().ok()?;
            let size_u = stream.Size().ok()?;
            if size_u == 0 || size_u > 512 * 1024 {
                continue;
            }
            let size = size_u as u32;
            let reader = DataReader::CreateDataReader(&stream).ok()?;
            if reader.LoadAsync(size).ok()?.get().is_err() {
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            if reader.ReadBytes(&mut buf).is_err() {
                continue;
            }
            return Some(STANDARD.encode(&buf));
        }
        None
    }

    pub fn poll_non_wechat_toast() -> Option<AppToastHint> {
        let listener = UserNotificationListener::Current().ok()?;
        let status = listener.GetAccessStatus().ok()?;
        let mut st = state_mutex()
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if status != UserNotificationListenerAccessStatus::Allowed {
            if matches!(status, UserNotificationListenerAccessStatus::Denied) && !st.logged_access {
                st.logged_access = true;
                eprintln!(
                    "[toast:notif] 通知访问被系统拒绝。请在「设置 → 隐私和安全性 → 通知」中允许 IslandCore 访问通知。"
                );
            }
            if !st.access_requested {
                st.access_requested = true;
                drop(st);
                let _ = listener.RequestAccessAsync().and_then(|op| op.get());
            }
            return None;
        }

        let kinds = NotificationKinds::Toast | NotificationKinds::Unknown;
        let Some(list) = listener
            .GetNotificationsAsync(kinds)
            .ok()
            .and_then(|op| op.get().ok())
        else {
            return None;
        };
        let n = list.Size().unwrap_or(0);

        if debug_notifications() {
            if n == 0 {
                let now = std::time::Instant::now();
                let say = st
                    .last_notify_debug_empty
                    .map(|t| t.elapsed() >= std::time::Duration::from_secs(10))
                    .unwrap_or(true);
                if say {
                    st.last_notify_debug_empty = Some(now);
                    eprintln!(
                        "[notif-debug] 轮询: 通知中心当前 0 条 (Toast|Unknown)。"
                    );
                }
            } else {
                st.last_notify_debug_empty = None;
                dump_all_notifications(&list);
            }
        }

        let mut rows: Vec<Row> = Vec::new();
        for i in 0..n {
            let Ok(un) = list.GetAt(i) else {
                continue;
            };
            let id = un.Id().unwrap_or(0);
            let ut = un
                .CreationTime()
                .map(|dt| dt.UniversalTime)
                .unwrap_or(0);
            let key = SeenNotif { id, universal_time: ut };
            let app = un.AppInfo().ok();
            let (title, body) = toast_lines(&un).unwrap_or_else(|| ("通知".to_string(), String::new()));
            rows.push(Row {
                key,
                app,
                title,
                body,
            });
        }

        if rows.is_empty() {
            if !st.baselined {
                st.baselined = true;
            }
            return None;
        }

        if !st.baselined {
            for row in &rows {
                st.seen.insert(row.key);
            }
            st.baselined = true;
            return None;
        }

        rows.sort_by_key(|r| std::cmp::Reverse(r.key.universal_time));

        for row in rows {
            if st.seen.contains(&row.key) {
                continue;
            }
            st.seen.insert(row.key);

            if let Some(ref info) = row.app {
                if is_wechat_app(info) {
                    continue;
                }
                return Some(AppToastHint {
                    app_name: app_display_name(info),
                    aumid: info
                        .AppUserModelId()
                        .ok()
                        .map(|s| s.to_string())
                        .filter(|s| !s.trim().is_empty()),
                    title: row.title,
                    body: row.body,
                    icon_base64: try_app_logo_base64(info),
                });
            }

            return Some(AppToastHint {
                app_name: "通知".to_string(),
                aumid: None,
                title: row.title,
                body: row.body,
                icon_base64: None,
            });
        }

        None
    }
}

#[cfg(target_os = "windows")]
pub use win::poll_non_wechat_toast;

#[cfg(not(target_os = "windows"))]
pub fn poll_non_wechat_toast() -> Option<AppToastHint> {
    None
}
