mod album_art;
mod battery;
mod event_queue;
mod keyboard_hook;
mod media;
mod media_control;
mod netease_cover;
mod netease_local;
mod brightness;
mod focus_assist;
mod theme;
mod theme_mode;
mod volume;
mod radios;
mod system_actions;
mod window;
mod wechat_watch;
#[cfg(target_os = "windows")]
mod wechat_notifications;
mod chrome_focus;
mod ide_focus;
mod bookmarks;

use album_art::{fetch_album_art_with_download, AlbumArtPayload};
use battery::BatteryInfo;
use event_queue::{new_queue, SystemEvent};
use media::MediaInfo;
use netease_cover::CoverUrlPayload;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone)]
struct SharedMedia(Arc<Mutex<Option<MediaInfo>>>);

/// 获取当前媒体（前端启动兜底，避免错过初始事件）
#[tauri::command]
fn get_current_media(shared: State<'_, SharedMedia>) -> Option<MediaInfo> {
    // 优先读传感线程缓存；为空或锁 poison 时直接同步读 GSMTC（与 media.rs 一致，无 tokio await）
    let from_cache = match shared.0.lock() {
        Ok(g) => (*g).clone(),
        Err(e) => {
            eprintln!("[cmd] get_current_media: shared mutex poisoned: {e}");
            None
        }
    };
    let v = if from_cache.is_some() {
        from_cache
    } else {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let fresh = media::get_current_media_sync();
        if fresh.is_some() {
            if let Ok(mut g) = shared.0.lock() {
                *g = fresh.clone();
            }
        }
        fresh
    };
    println!(
        "[cmd] get_current_media => {}",
        if v.is_some() { "some" } else { "none" }
    );
    v
}

/// 获取专辑封面
#[tauri::command]
fn get_album_art(title: String, artist: String, album: String, source: String) -> Option<AlbumArtPayload> {
    fetch_album_art_with_download(&title, &artist, &album, &source)
}

/// 从 URL 下载图片
#[tauri::command]
fn download_image(url: String) -> Option<String> {
    album_art::download_image_as_base64(&url)
}


fn start_sensor_thread(app: AppHandle, shared_media: Arc<Mutex<Option<MediaInfo>>>) {
    let queue = new_queue();
    keyboard_hook::start(queue.clone());

    std::thread::spawn(move || {
        println!("[sensor] thread started");
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        // 等待窗口初始化完成（媒体读取不依赖窗口，缩短等待避免前端启动时拿不到缓存）
        std::thread::sleep(Duration::from_millis(50));
        println!("[sensor] after initial sleep");

        // ── 初始状态推送 ──────────────────────────────────
        let init_media = media::get_current_media_sync();
        println!("[sensor] init media = {}", if init_media.is_some() { "some" } else { "none" });
        if let Some(m) = &init_media {
            println!(
                "[sensor] init title={:?} artist={:?} status={} pos_ms={}",
                m.title, m.artist, m.status, m.position_ms
            );
        }
        {
            let mut g = shared_media
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *g = init_media.clone();
            println!("[sensor] shared_media set (init)");
        }
        let _ = app.emit("media-change", init_media.clone());
        let init_bat = battery::get_battery();
        let _ = app.emit("battery-change", init_bat.clone());

        let last_media: Arc<Mutex<Option<MediaInfo>>> = Arc::new(Mutex::new(init_media));
        let last_battery: Arc<Mutex<Option<BatteryInfo>>> = Arc::new(Mutex::new(Some(init_bat)));
        let last_cover_key: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
        let mut last_lowbat_alert: Option<Instant> = None;
        #[cfg(target_os = "windows")]
        let mut wechat_debounce =
            wechat_watch::WeChatDebounce::new(Duration::from_secs(10));

        // Toast 与队列每 tick 处理；媒体/电池/微信等较重逻辑每 SENSOR_FULL_EVERY tick 一次，避免整圈被拖慢。
        const SENSOR_TICK_MS: u64 = 100;
        const SENSOR_FULL_EVERY: u32 = 5;
        let mut sensor_phase: u32 = 0;

        loop {
            let do_full = sensor_phase % SENSOR_FULL_EVERY == 0;

            if do_full {
            // ── 媒体 ──────────────────────────────────────
            let info = media::get_current_media_sync();
            
            {
                let mut guard = last_media
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let changed = match (&*guard, &info) {
                    (None, None) => false,
                    (Some(a), Some(b)) => {
                        a.title != b.title
                            || a.artist != b.artist
                            || a.status != b.status
                            || a.thumbnail != b.thumbnail
                            || (b.status == "playing"
                                && (a.position_ms / 1000) != (b.position_ms / 1000))
                            // 暂停时进度条若变化（拖拽进度）也推送
                            || (b.status != "playing"
                                && (a.position_ms / 1000) != (b.position_ms / 1000))
                    }
                    _ => true,
                };
                if changed {
                    *guard = info.clone();
                    drop(guard);
                    {
                        let mut g = shared_media
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        *g = info.clone();
                        println!("[sensor] shared_media set (change)");
                    }
                    if let Some(m) = &info {
                        println!("[sensor] emit media-change: {:?} - {:?} {}", m.title, m.artist, m.status);
                    } else {
                        println!("[sensor] emit media-change: none");
                    }
                    let _ = app.emit("media-change", info);
                }
            }

            // ── 网易云封面 URL（仅封面，无歌词；子线程 panic 不拖垮传感循环）────
            {
                let cur = last_media
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                match cur {
                    Some(m) if !m.title.trim().is_empty() => {
                        let key = (m.title.clone(), m.artist.clone());
                        let mut ck = last_cover_key
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        let need = ck.as_ref().map(|k| k != &key).unwrap_or(true);
                        if need {
                            *ck = Some(key.clone());
                            drop(ck);
                            let app2 = app.clone();
                            std::thread::spawn(move || {
                                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    let url =
                                        netease_cover::fetch_netease_cover_url(&key.0, &key.1);
                                    let payload = CoverUrlPayload {
                                        title: key.0,
                                        artist: key.1,
                                        url,
                                        source: "netease".to_string(),
                                    };
                                    let _ = app2.emit("cover-url-change", payload);
                                }));
                                if r.is_err() {
                                    eprintln!("[cover:netease] worker panicked (ignored)");
                                }
                            });
                        }
                    }
                    _ => {
                        let mut ck = last_cover_key
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if ck.take().is_some() {
                            drop(ck);
                            let payload = CoverUrlPayload {
                                title: "".to_string(),
                                artist: "".to_string(),
                                url: None,
                                source: "netease".to_string(),
                            };
                            let _ = app.emit("cover-url-change", payload);
                        }
                    }
                }
            }

            // ── 电池 ──────────────────────────────────────
            let bat = battery::get_battery();
            let charging_now = bat.charging;
            let bat_pct = bat.percent;
            {
                let mut guard = last_battery
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let changed = guard.as_ref() != Some(&bat);
                if changed {
                    *guard = Some(bat.clone());
                    drop(guard);
                    let _ = app.emit("battery-change", bat);
                }
            }

            // ── 微信：仅顶层窗口（不走通知中心）──────────────────
            #[cfg(target_os = "windows")]
            {
                if let Some(h) = wechat_watch::scan_top_level_wechat() {
                    if let Some(h) = wechat_debounce.try_emit(h) {
                        if h.kind == "call" || h.kind == "message" {
                            queue.lock().unwrap().push_back(SystemEvent::WeChat {
                                source: h.source,
                                kind: h.kind,
                                title: h.title,
                                body: String::new(),
                            });
                        }
                    }
                }
            }

            // ── 低电量告警（60s 节流）────────────────────
            if bat_pct <= 15 && !charging_now {
                let should = last_lowbat_alert
                    .map(|t| t.elapsed() >= Duration::from_secs(60))
                    .unwrap_or(true);
                if should {
                    last_lowbat_alert = Some(Instant::now());
                    queue.lock().unwrap().push_back(
                        SystemEvent::LowBattery { percent: bat_pct as u8 }
                    );
                }
            }

            } // do_full

            // ── 其它应用：系统 Toast（非微信）──────────────────────
            #[cfg(target_os = "windows")]
            if let Some(t) = wechat_notifications::poll_non_wechat_toast() {
                queue.lock().unwrap().push_back(SystemEvent::AppToast {
                    app_name: t.app_name,
                    aumid: t.aumid,
                    title: t.title,
                    body: t.body,
                    icon_base64: t.icon_base64,
                });
            }

            // ── Chrome 前台跟随（每 tick，与 Toast 同频）──────────
            chrome_focus::emit_chrome_focus_if_changed(&app);
            // ── Cursor / VS Code 前台与最小化托底 ─────────────────
            ide_focus::emit_ide_focus_if_changed(&app);

            // ── 消费事件队列 → emit system-event ──────────

            {
                let mut q = queue.lock().unwrap();
                let mut best: Option<SystemEvent> = None;
                let mut remainder = std::collections::VecDeque::new();
                while let Some(ev) = q.pop_front() {
                    let p = priority(&ev);
                    let bp = best.as_ref().map(priority).unwrap_or(0);
                    if p > bp {
                        if let Some(old) = best.take() { remainder.push_back(old); }
                        best = Some(ev);
                    } else {
                        remainder.push_back(ev);
                    }
                }
                // 只保留最新一个 VolumeChange / WeChat / AppToast，防止积压
                let mut seen_vol = false;
                let mut seen_wx = false;
                let mut seen_toast = false;
                for ev in remainder {
                    match &ev {
                        SystemEvent::VolumeChange { .. } => {
                            if !seen_vol { seen_vol = true; q.push_back(ev); }
                        }
                        SystemEvent::WeChat { .. } => {
                            if !seen_wx { seen_wx = true; q.push_back(ev); }
                        }
                        SystemEvent::AppToast { .. } => {
                            if !seen_toast { seen_toast = true; q.push_back(ev); }
                        }
                        _ => q.push_back(ev),
                    }
                }

                drop(q);
                if let Some(ev) = best {
                    let _ = app.emit("system-event", ev);
                }
                
            }

            sensor_phase = sensor_phase.wrapping_add(1);
            std::thread::sleep(Duration::from_millis(SENSOR_TICK_MS));
        }
    });
}

fn priority(ev: &SystemEvent) -> u8 {
    match ev {
        SystemEvent::WeChat { .. }     => 5,
        SystemEvent::AppToast { .. }   => 4,
        SystemEvent::LowBattery { .. } => 3,
        SystemEvent::CapsLock { .. }   => 2,
        SystemEvent::VolumeChange { .. } => 1,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared_media: Arc<Mutex<Option<MediaInfo>>> = Arc::new(Mutex::new(None));
    tauri::Builder::default()
        .manage(SharedMedia(shared_media.clone()))
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let win = app.get_webview_window("main").unwrap();
            window::setup_island_window(&win);
            start_sensor_thread(app.handle().clone(), shared_media.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_current_media,
            get_album_art,
            download_image,
            media_control::media_control,
            volume::get_volume,
            volume::set_volume,
            volume::set_mute,
            brightness::get_brightness,
            brightness::set_brightness,
            theme::get_accent_color,
            radios::get_radios_state,
            radios::set_radio_state,
            focus_assist::get_focus_assist,
            focus_assist::set_focus_assist,
            system_actions::screenshot,
            system_actions::open_quick_settings,
            system_actions::open_system_uri,
            system_actions::open_now_playing_app,
            theme_mode::get_theme_mode,
            theme_mode::set_theme_mode,
            chrome_focus::set_chrome_sessions_mute,
            chrome_focus::copy_text_to_clipboard,
            bookmarks::bookmark_list,
            bookmarks::bookmark_add,
            bookmarks::bookmark_remove,
            bookmarks::bookmark_exists,
            bookmarks::bookmark_open_url,
            ide_focus::focus_tracked_ide
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
