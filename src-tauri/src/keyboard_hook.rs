use crate::event_queue::{SharedQueue, SystemEvent};

pub fn start(queue: SharedQueue) {
    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        hook_thread(queue);
        #[cfg(not(target_os = "windows"))]
        let _ = queue;
    });
}

// ─────────────────────────────────────────────────────────────
// Windows 实现
// ─────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
mod imp {
    use super::{SharedQueue, SystemEvent};
    use std::cell::RefCell;

    // 模块级 thread_local，供钩子回调访问
    thread_local! {
        pub(super) static TL_QUEUE: RefCell<Option<SharedQueue>> = RefCell::new(None);
    }

    pub(super) unsafe extern "system" fn hook_proc(
        code: i32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            CallNextHookEx, HHOOK, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
        };

        if code >= 0 {
            let msg = wparam.0 as u32;
            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                let vk = VIRTUAL_KEY(kb.vkCode as u16);

                let is_vol   = vk == VK_VOLUME_UP || vk == VK_VOLUME_DOWN || vk == VK_VOLUME_MUTE;
                let is_caps  = vk == VK_CAPITAL;

                if is_vol || is_caps {
                    TL_QUEUE.with(|tq| {
                        if let Some(q) = tq.borrow().as_ref() {
                            if is_caps {
                                // WH_KEYBOARD_LL 触发时 Caps 状态尚未切换，取反即为新状态
                                let cur = GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0;
                                q.lock().unwrap().push_back(SystemEvent::CapsLock { on: !cur });
                            } else {
                                let (pct, muted) = super::read_volume()
                                    .unwrap_or((50, vk == VK_VOLUME_MUTE));
                                q.lock().unwrap().push_back(
                                    SystemEvent::VolumeChange { percent: pct, muted },
                                );
                            }
                        }
                    });
                }
            }
        }
        CallNextHookEx(Some(HHOOK::default()), code, wparam, lparam)
    }
}

#[cfg(target_os = "windows")]
fn hook_thread(queue: SharedQueue) {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, SetWindowsHookExW, WH_KEYBOARD_LL,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    // 把队列放入线程局部存储供回调使用
    imp::TL_QUEUE.with(|tq| *tq.borrow_mut() = Some(queue));

    let _hook = unsafe {
        // GetModuleHandleW 返回 HMODULE，SetWindowsHookExW 需要 Option<HINSTANCE>
        // 两者底层都是 *mut c_void，可以 transmute
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        let hinstance = windows::Win32::Foundation::HINSTANCE(hmod.0);
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(imp::hook_proc), Some(hinstance), 0)
            .expect("SetWindowsHookExW failed")
    };

    // 消息循环保持钩子存活
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    }
}

/// 读取当前默认输出设备音量 (0–100) 和静音状态
#[cfg(target_os = "windows")]
fn read_volume() -> Option<(u8, bool)> {
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        // windows-rs 0.59: Activate<T>(dwclsctx, pactivationparams) -> Result<T>
        let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        let vol   = endpoint.GetMasterVolumeLevelScalar().ok()?;
        let muted = endpoint.GetMute().ok()?.as_bool();
        Some(((vol * 100.0).round() as u8, muted))
    }
}

#[cfg(not(target_os = "windows"))]
fn read_volume() -> Option<(u8, bool)> { None }
