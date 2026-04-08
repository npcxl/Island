//! 系统通知嗅探：看微信（及其它应用）是否真的出现在 Windows 通知中心。
//!
//! **不要照抄网上的 `Title()` / `Body()` 示例** —— WinRT 里没有这套链式 API，
//! 正确做法是 `Notification → Visual → GetBinding(ToastGeneric) 或遍历 Bindings → GetTextElements`。
//!
//! **托盘角标**：本程序会额外用 **UI Automation** 扫 `Shell_TrayWnd` 子树，查找
//! `Name` / `HelpText` 里是否出现「微信」「WeChat」等。  
//! 若只能看到应用名而**没有数字**，说明角标多半是画在图标位图里的，**没有公开「未读数」属性**可给第三方读。
//!
//! 运行（在 `src-tauri` 目录）:
//! ```text
//! cargo run --example wechat_notify_demo
//! ```
//!
//! 前置条件：
//! - 设置 → 隐私 → 通知：允许 **本终端里跑起来的这个 exe** 访问通知（首次运行会弹系统授权）。
//! - 微信：设置 → 通知 → 打开「允许电脑通知」；Windows 里微信通知总开关也要开。

#[cfg(not(windows))]
fn main() {
    eprintln!("wechat_notify_demo 仅在 Windows 上可用。");
}

#[cfg(windows)]
use windows::ApplicationModel::AppInfo;
#[cfg(windows)]
use windows::UI::Notifications::KnownNotificationBindings;
#[cfg(windows)]
use windows::UI::Notifications::UserNotification;
#[cfg(windows)]
use windows::Win32::UI::Accessibility::{IUIAutomationElement, IUIAutomationTreeWalker};

#[cfg(windows)]
fn main() {
    if let Err(e) = run() {
        eprintln!("错误: {e:?}");
        std::process::exit(1);
    }
}

/// 用 UI Automation 扫任务栏根窗口子树，看「微信」相关节点在 Name/HelpText 里暴露什么。
/// 这不是 `ITaskbarList3` 读角标（系统不提供），只是验证 Explorer 是否给出可读文本。
#[cfg(windows)]
fn probe_tray_wechat_uia() -> windows::core::Result<()> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
    };
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::w;

    unsafe {
        let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let tray = FindWindowW(w!("Shell_TrayWnd"), None)?;
        if tray.is_invalid() {
            println!("    找不到 Shell_TrayWnd");
            return Ok(());
        }
        let root: IUIAutomationElement = auto.ElementFromHandle(tray)?;
        let walker: IUIAutomationTreeWalker = auto.RawViewWalker()?;
        let mut hits: Vec<String> = Vec::new();
        let mut visited: usize = 0;
        visit_tray_nodes(
            &walker,
            &root,
            0,
            32,
            12_000,
            &mut visited,
            &mut hits,
        );
        println!("    已遍历 UIA 节点约 {visited} 个（深度≤32，最多约 12000 节点）");
        if hits.is_empty() {
            println!("    （无）未在 Name/HelpText 中发现 微信 | wechat | weixin");
            println!("    可能：角标画在图标上、或项在折叠区/版本差异未暴露文本。");
        } else {
            for h in &hits {
                println!("    · {h}");
            }
            println!("    若条目里没有阿拉伯数字，通常仍无法当作「系统未读数字段」使用。");
        }
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn visit_tray_nodes(
    walker: &IUIAutomationTreeWalker,
    el: &IUIAutomationElement,
    depth: u32,
    max_depth: u32,
    max_nodes: usize,
    visited: &mut usize,
    hits: &mut Vec<String>,
) {
    if depth > max_depth || *visited >= max_nodes {
        return;
    }
    *visited += 1;

    let n = el
        .CurrentName()
        .ok()
        .map(|b: windows::core::BSTR| b.to_string())
        .unwrap_or_default();
    let help = el
        .CurrentHelpText()
        .ok()
        .map(|b: windows::core::BSTR| b.to_string())
        .unwrap_or_default();
    let cls = el
        .CurrentClassName()
        .ok()
        .map(|b: windows::core::BSTR| b.to_string())
        .unwrap_or_default();
    let blob = format!("{n} {help}").to_lowercase();
    if blob.contains("微信")
        || blob.contains("wechat")
        || blob.contains("weixin")
    {
        hits.push(format!(
            "depth={depth} class={cls} name={n:?} help={help:?}"
        ));
    }

    let Ok(child) = walker.GetFirstChildElement(el) else {
        return;
    };
    let mut cur = child;
    loop {
        visit_tray_nodes(walker, &cur, depth + 1, max_depth, max_nodes, visited, hits);
        let Ok(next) = walker.GetNextSiblingElement(&cur) else {
            break;
        };
        cur = next;
    }
}

#[cfg(windows)]
fn run() -> windows::core::Result<()> {
    use std::io::{self, Write};
    use std::thread;
    use std::time::Duration;

    use windows::UI::Notifications::Management::{
        UserNotificationListener, UserNotificationListenerAccessStatus,
    };
    use windows::UI::Notifications::NotificationKinds;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    println!("=== 系统通知测试（UserNotificationListener）===");
    println!("按 Ctrl+C 结束。\n");

    println!(">>> [托盘/任务栏 UIA 探测] 启动时一次，之后每约 5 秒重复");
    if let Err(e) = probe_tray_wechat_uia() {
        eprintln!("    UIA 探测失败: {e:?}");
    }

    let listener = UserNotificationListener::Current()?;

    let mut status = listener.GetAccessStatus()?;
    println!("GetAccessStatus: {} (1=Allowed)", status.0);

    if status != UserNotificationListenerAccessStatus::Allowed {
        println!("正在 RequestAccessAsync，请在系统对话框中允许访问通知…");
        status = listener.RequestAccessAsync()?.get()?;
        println!("授权后状态: {} (1=Allowed)", status.0);
    }

    if status != UserNotificationListenerAccessStatus::Allowed {
        eprintln!(
            "\n仍无权限：请到「设置 → 隐私和安全性 → 通知」打开「通知访问权限」里对本应用的开关。\n"
        );
        return Ok(());
    }

    let mut poll: u64 = 0;
    loop {
        poll += 1;
        if poll % 5 == 0 {
            println!("\n>>> [托盘/任务栏 UIA 探测] 定时重复 (poll={poll})");
            let _ = probe_tray_wechat_uia();
        }

        let kinds = NotificationKinds::Toast | NotificationKinds::Unknown;
        let list = match listener.GetNotificationsAsync(kinds) {
            Ok(op) => match op.get() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("GetNotificationsAsync().get() 失败: {e:?}");
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            },
            Err(e) => {
                eprintln!("GetNotificationsAsync 失败: {e:?}");
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        };

        let n = list.Size().unwrap_or(0);
        println!("\n----- 本轮共 {n} 条 (Toast|Unknown) -----");

        for i in 0..n {
            let Ok(un) = list.GetAt(i) else {
                continue;
            };
            print_one(i, &un);
        }

        let _ = io::stdout().flush();
        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(windows)]
fn lines_from_binding(
    binding: &windows::UI::Notifications::NotificationBinding,
) -> Vec<String> {
    let Ok(texts) = binding.GetTextElements() else {
        return Vec::new();
    };
    let count = texts.Size().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..count {
        if let Ok(t) = texts.GetAt(i) {
            if let Ok(s) = t.Text() {
                let st = s.to_string();
                if !st.trim().is_empty() {
                    out.push(st);
                }
            }
        }
    }
    out
}

#[cfg(windows)]
fn toast_text(un: &UserNotification) -> String {
    let Ok(n) = un.Notification() else {
        return "(无法取 Notification)".into();
    };
    let Ok(visual) = n.Visual() else {
        return "(无法取 Visual)".into();
    };

    if let Ok(bind_name) = KnownNotificationBindings::ToastGeneric() {
        if let Ok(binding) = visual.GetBinding(&bind_name) {
            let lines = lines_from_binding(&binding);
            if !lines.is_empty() {
                return lines.join(" | ");
            }
        }
    }

    if let Ok(bindings) = visual.Bindings() {
        let sz = bindings.Size().unwrap_or(0);
        for i in 0..sz {
            if let Ok(binding) = bindings.GetAt(i) {
                let lines = lines_from_binding(&binding);
                if !lines.is_empty() {
                    return lines.join(" | ");
                }
            }
        }
    }

    "(无文本绑定)".into()
}

#[cfg(windows)]
fn is_wechat_hint(info: &AppInfo) -> bool {
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
    aumid.contains("wechat")
        || aumid.contains("weixin")
        || aumid.contains("tencent")
        || name.contains("微信")
        || name.contains("wechat")
        || pfn.contains("tencent")
}

#[cfg(windows)]
fn print_one(idx: u32, un: &UserNotification) {
    let id = un.Id().unwrap_or(0);
    let tick = un
        .CreationTime()
        .map(|dt| dt.UniversalTime)
        .unwrap_or(0);
    let text = toast_text(un);

    println!("\n  [#{idx}] id={id} creation_ticks={tick}");
    println!("       text: {text}");

    match un.AppInfo() {
        Ok(info) => {
            let aumid = info
                .AppUserModelId()
                .ok()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let disp = info
                .DisplayInfo()
                .ok()
                .and_then(|d| d.DisplayName().ok().map(|s| s.to_string()))
                .unwrap_or_default();
            let pfn = info
                .PackageFamilyName()
                .ok()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let app_id = info
                .Id()
                .ok()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let wx = is_wechat_hint(&info);
            println!("       AppUserModelId: {aumid}");
            println!("       DisplayName:    {disp}");
            println!("       PackageFamily:  {pfn}");
            println!("       AppInfo.Id:     {app_id}");
            println!("       >>> 是否像微信: {wx}");
        }
        Err(e) => {
            println!("       AppInfo: **失败** {e:?}（无法显示应用名，只能看上面 text）");
        }
    }
}
