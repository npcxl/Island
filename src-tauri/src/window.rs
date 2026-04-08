use tauri::WebviewWindow;

#[cfg(target_os = "windows")]
pub fn setup_island_window(win: &WebviewWindow) {
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Dwm::DwmExtendFrameIntoClientArea,
        UI::Controls::MARGINS,
        UI::WindowsAndMessaging::{
            GetSystemMetrics, GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW,
            SetWindowPos, GWL_EXSTYLE, GWL_STYLE, HWND_TOPMOST, LWA_ALPHA,
            SM_CXSCREEN, SWP_FRAMECHANGED, SWP_NOSIZE, SWP_SHOWWINDOW, WS_CAPTION,
            WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX,
            WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
        },
    };

    let hwnd = HWND(win.hwnd().unwrap().0 as _);

    // 窗口逻辑宽度（与 tauri.conf.json 保持一致）
    let win_w: i32 = 400;

    unsafe {
        // ── 1. 移除标题栏 / 边框 / 系统按钮 ──
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let remove_style =
            (WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU).0;
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !remove_style) as isize);

        // ── 2. ExStyle: 分层透明 + 不出现在任务栏 + 不抢焦点 ──
        let exstyle = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let add_ex = (WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE).0;
        let remove_ex = WS_EX_APPWINDOW.0;
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ((exstyle & !remove_ex) | add_ex) as isize,
        );

        // ── 3. 分层窗口：完全不透明（动画透明度由前端 CSS 处理）──
        use windows::Win32::Foundation::COLORREF;
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

        // ── 4. DWM：将客户区扩展到整个窗口，让 WebView backdrop-filter 正常工作 ──
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // ── 5. 水平居中 + 贴近屏幕顶部 ──
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let x = (screen_w - win_w) / 2;

        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            8,
            0,
            0,
            SWP_NOSIZE | SWP_FRAMECHANGED,
        );

        // ── 6. 显示窗口（tauri.conf.json 里 visible=false，这里手动显示） ──
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            8,
            0,
            0,
            SWP_NOSIZE | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn setup_island_window(_win: &WebviewWindow) {}
