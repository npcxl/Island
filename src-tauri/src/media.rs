use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub thumbnail: String,
    pub accent: String,
    pub status: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub source: String,
}

fn extract_accent(data: &[u8]) -> String {
    let step = (data.len() / 3 / 64).max(3);
    let (mut rs, mut gs, mut bs, mut n) = (0u64, 0u64, 0u64, 0u64);
    let mut i = 0usize;
    while i + 2 < data.len() {
        let (r, g, b) = (data[i] as u64, data[i + 1] as u64, data[i + 2] as u64);
        let br = (r + g + b) / 3;
        if br > 30 && br < 220 {
            rs += r;
            gs += g;
            bs += b;
            n += 1;
        }
        i += step;
    }
    if n == 0 {
        return "#a0a0a0".to_string();
    }
    format!(
        "#{:02x}{:02x}{:02x}",
        (rs / n) as u8,
        (gs / n) as u8,
        (bs / n) as u8
    )
}

fn normalize_source(source: &str) -> String {
    let s = source.to_lowercase();
    if s.contains("qqmusic") || s.contains("qq音乐") {
        "qqmusic".to_string()
    } else if s.contains("cloudmusic") || s.contains("netease") || s.contains("orpheus") {
        "cloudmusic".to_string()
    } else if s.contains("spotify") {
        "spotify".to_string()
    } else if s.contains("chrome") {
        "chrome".to_string()
    } else {
        "unknown".to_string()
    }
}

fn parse_title(title: &str, app_hint: &str) -> (String, String) {
    let suffixes = [" - 网易云音乐", " - NetEase Cloud Music", " - QQ音乐", " - Spotify"];
    let mut t = title.to_string();
    for s in &suffixes {
        if t.ends_with(s) {
            t = t[..t.len() - s.len()].to_string();
            break;
        }
    }
    if let Some(pos) = t.find(" - ") {
        let song = t[..pos].trim().to_string();
        let artist = t[pos + 3..].trim().to_string();
        let artist = artist.split(" - ").next().unwrap_or("").trim().to_string();
        (song, artist)
    } else {
        let _ = app_hint;
        (t, String::new())
    }
}

/// 从窗口标题兜底（网易云 / QQ 音乐 / Spotify）
#[cfg(target_os = "windows")]
fn get_media_from_window_title() -> Option<MediaInfo> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetClassNameW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId,
    };

    let candidates: &[(&str, &str)] = &[
        ("OrpheusBrowserHost", "cloudmusic"),
        ("TXGuiFoundation", "qqmusic"),
        ("ApolloRuntimeContentWindow", "spotify"),
    ];

    let fg = unsafe { GetForegroundWindow() };

    fn exe_name_from_hwnd(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)); }
        if pid == 0 {
            return None;
        }
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = vec![0u16; 260];
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
            let full = String::from_utf16_lossy(&buf[..size as usize]);
            let file = full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string();
            Some(file.to_lowercase())
        }
    }

    if fg.0 != std::ptr::null_mut() {
        let len = unsafe { GetWindowTextLengthW(fg) };
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            unsafe { GetWindowTextW(fg, &mut buf); }
            let title = String::from_utf16_lossy(&buf)
                .trim_end_matches('\0')
                .to_string();

            let mut cls_buf = vec![0u16; 256];
            let n = unsafe { GetClassNameW(fg, &mut cls_buf) } as usize;
            let class_name = if n > 0 {
                String::from_utf16_lossy(&cls_buf[..n])
            } else {
                "".to_string()
            };

            let exe = exe_name_from_hwnd(fg).unwrap_or_default();

            if !title.contains("IslandCore") && !title.trim().is_empty() {
                let app_hint =
                    if exe.contains("cloudmusic")
                        || title.contains("网易云音乐")
                        || class_name.contains("OrpheusBrowserHost")
                    {
                        "cloudmusic"
                    } else if exe.contains("qqmusic")
                        || title.contains("QQ音乐")
                        || class_name.contains("TXGuiFoundation")
                    {
                        "qqmusic"
                    } else if exe.contains("spotify")
                        || title.contains("Spotify")
                        || class_name.contains("ApolloRuntimeContentWindow")
                    {
                        "spotify"
                    } else {
                        ""
                    };

                if !app_hint.is_empty() {
                    let (song, artist) = parse_title(&title, app_hint);
                    if !song.is_empty() {
                        return Some(MediaInfo {
                            title: song,
                            artist,
                            thumbnail: String::new(),
                            accent: "#a0a0a0".to_string(),
                            status: "playing".to_string(),
                            position_ms: 0,
                            duration_ms: 0,
                            source: app_hint.to_string(),
                        });
                    }
                }
            }
        }
    }

    for (class_name, app_hint) in candidates {
        let class_w: Vec<u16> = class_name.encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = match unsafe { FindWindowW(windows::core::PCWSTR(class_w.as_ptr()), None) } {
            Ok(h) => h,
            Err(_) => continue,
        };

        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len == 0 {
            continue;
        }

        let mut buf = vec![0u16; (len + 1) as usize];
        unsafe { GetWindowTextW(hwnd, &mut buf); }
        let title = String::from_utf16_lossy(&buf)
            .trim_end_matches('\0')
            .to_string();
        if title.is_empty() {
            continue;
        }

        let (song, artist) = parse_title(&title, app_hint);
        if song.is_empty() {
            continue;
        }

        return Some(MediaInfo {
            title: song,
            artist,
            thumbnail: String::new(),
            accent: "#a0a0a0".to_string(),
            status: "playing".to_string(),
            position_ms: 0,
            duration_ms: 0,
            source: app_hint.to_string(),
        });
    }
    None
}

#[cfg(target_os = "windows")]
fn timespan_to_ms(ts: windows::Foundation::TimeSpan) -> u64 {
    let d = ts.Duration;
    if d <= 0 {
        0
    } else {
        (d as u64) / 10_000
    }
}

#[cfg(target_os = "windows")]
fn aumid_is_qq_music_smtc_broken(id: &str) -> bool {
    let l = id.to_lowercase();
    if l.is_empty() {
        return false;
    }
    l.contains("qqmusic")
        || l.contains("qq音乐")
        || l.contains("tencent.qqmusic")
        || l.contains("y.qq.com")
}

#[cfg(target_os = "windows")]
fn app_id_matches_media_source(app_id: &str, source: &str) -> bool {
    let l = app_id.to_lowercase();
    match source {
        "qqmusic" => aumid_is_qq_music_smtc_broken(app_id),
        "cloudmusic" => l.contains("cloudmusic") || l.contains("netease") || l.contains("orpheus"),
        "spotify" => l.contains("spotify"),
        _ => false,
    }
}

/// 只读播放状态，不碰 GetCurrentSession / TryGetMediaProperties（给窗口标题补暂停态）
/// 循环内禁止用 `?`：否则会话 0 失败会导致整段合并放弃
#[cfg(target_os = "windows")]
fn try_playback_status_for_source(
    sessions: &windows::Foundation::Collections::IVectorView<
        windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    >,
    count: u32,
    source: &str,
) -> Option<String> {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Ps;
    for i in 0..count {
        let Ok(s) = sessions.GetAt(i) else {
            continue;
        };
        let Ok(aid) = s.SourceAppUserModelId() else {
            continue;
        };
        let app_id = aid.to_string();
        if !app_id_matches_media_source(&app_id, source) {
            continue;
        }
        let Ok(pb) = s.GetPlaybackInfo() else {
            continue;
        };
        let Ok(st) = pb.PlaybackStatus() else {
            continue;
        };
        return Some(match st {
            Ps::Playing => "playing".to_string(),
            Ps::Paused => "paused".to_string(),
            _ => "stopped".to_string(),
        });
    }
    None
}

/// 仅一个 SMTC 会话时，不依赖 AUMID 匹配，直接读状态（窗口标题与 AUMID 对不上时的兜底）
#[cfg(target_os = "windows")]
fn try_playback_status_single_session(
    sessions: &windows::Foundation::Collections::IVectorView<
        windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    >,
    count: u32,
) -> Option<String> {
    if count != 1 {
        return None;
    }
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus as Ps;
    let s = sessions.GetAt(0).ok()?;
    let pb = s.GetPlaybackInfo().ok()?;
    let st = pb.PlaybackStatus().ok()?;
    Some(match st {
        Ps::Playing => "playing".to_string(),
        Ps::Paused => "paused".to_string(),
        _ => "stopped".to_string(),
    })
}

#[cfg(target_os = "windows")]
pub fn get_current_media_sync() -> Option<MediaInfo> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };
    use windows::Storage::Streams::DataReader;

    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .ok()?
        .get()
        .ok()?;

    let sessions = manager.GetSessions().ok()?;
    let count = sessions.Size().unwrap_or(0);

    let mut non_qq = Vec::new();
    for i in 0..count {
        if let Ok(s) = sessions.GetAt(i) {
            let app_id = s
                .SourceAppUserModelId()
                .ok()
                .map(|v| v.to_string())
                .unwrap_or_default();
            if aumid_is_qq_music_smtc_broken(&app_id) {
                continue;
            }
            non_qq.push(s);
        }
    }

    if non_qq.is_empty() {
        let mut info = get_media_from_window_title()?;
        if let Some(st) = try_playback_status_for_source(&sessions, count, &info.source) {
            info.status = st;
        } else if let Some(st) = try_playback_status_single_session(&sessions, count) {
            info.status = st;
        }
        return Some(info);
    }

    let session = non_qq.into_iter().max_by_key(|s| {
        let status_score = s
            .GetPlaybackInfo()
            .ok()
            .and_then(|pb| pb.PlaybackStatus().ok())
            .map(|st| match st {
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => 30,
                GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => 20,
                _ => 10,
            })
            .unwrap_or(0);

        let has_meta = s
            .TryGetMediaPropertiesAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .map(|p| {
                let t = p.Title().ok().map(|x| x.to_string()).unwrap_or_default();
                let a = p.Artist().ok().map(|x| x.to_string()).unwrap_or_default();
                (!t.trim().is_empty()) || (!a.trim().is_empty())
            })
            .unwrap_or(false);

        status_score + if has_meta { 5 } else { 0 }
    })?;

    let status_str = session
        .GetPlaybackInfo()
        .ok()
        .and_then(|pb| pb.PlaybackStatus().ok())
        .map(|st| match st {
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => "playing",
            GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => "paused",
            _ => "stopped",
        })
        .unwrap_or("stopped");

    let (mut position_ms, mut duration_ms) = (0u64, 0u64);
    if let Ok(tl) = session.GetTimelineProperties() {
        position_ms = tl.Position().ok().map(timespan_to_ms).unwrap_or(0);
        duration_ms = tl.EndTime().ok().map(timespan_to_ms).unwrap_or(0);
    }

    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;

    let title = props.Title().ok().map(|s| s.to_string()).unwrap_or_default();
    let artist = props.Artist().ok().map(|s| s.to_string()).unwrap_or_default();

    if title.is_empty() && artist.is_empty() {
        let mut w = get_media_from_window_title()?;
        if let Some(st) = try_playback_status_for_source(&sessions, count, &w.source) {
            w.status = st;
        } else if let Some(st) = try_playback_status_single_session(&sessions, count) {
            w.status = st;
        }
        return Some(w);
    }

    let (mut thumbnail, mut accent) = (String::new(), "#a0a0a0".to_string());
    if let Ok(thumb_ref) = props.Thumbnail() {
        if let Ok(open_op) = thumb_ref.OpenReadAsync() {
            if let Ok(stream) = open_op.get() {
                if let Ok(size_u) = stream.Size() {
                    let size = size_u as u32;
                    if size > 0 && size < 2 * 1024 * 1024 {
                        if let Ok(reader) = DataReader::CreateDataReader(&stream) {
                            if let Ok(load_op) = reader.LoadAsync(size) {
                                if load_op.get().is_ok() {
                                    let mut buf = vec![0u8; size as usize];
                                    if reader.ReadBytes(&mut buf).is_ok() {
                                        accent = extract_accent(&buf);
                                        thumbnail = STANDARD.encode(&buf);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let raw_source = session
        .SourceAppUserModelId()
        .ok()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Some(MediaInfo {
        title,
        artist,
        thumbnail,
        accent,
        status: status_str.to_string(),
        position_ms,
        duration_ms,
        source: normalize_source(&raw_source),
    })
}

#[cfg(not(target_os = "windows"))]
pub fn get_current_media_sync() -> Option<MediaInfo> {
    None
}
