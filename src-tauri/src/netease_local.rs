use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct NcmResponse {
    data: Option<NcmData>,
}

#[derive(Debug, Clone, Deserialize)]
struct NcmData {
    progress: Option<i64>,  // ms
    duration: Option<i64>,  // ms
    id: Option<u64>,
    name: Option<String>,
    artist: Option<String>,
    status: Option<i32>, // 1=playing (常见)
}

#[derive(Debug, Clone)]
pub struct NeteaseLocalStatus {
    pub progress_ms: u64,
    pub duration_ms: u64,
    pub id: Option<u64>,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[cfg(target_os = "windows")]
pub fn get_player_status() -> Option<NeteaseLocalStatus> {
    let url = "http://localhost:10690/api/player/getPlayerStatus";

    // 用 ureq（我们项目已引入），避免 tokio/reqwest
    let resp = ureq::get(url)
        .set("User-Agent", "IslandCore/0.1 (netease-local)")
        .timeout(std::time::Duration::from_millis(900))
        .call()
        .ok()?;

    let json: NcmResponse = resp.into_json().ok()?;
    let data = json.data?;

    let progress = data.progress?;
    let duration = data.duration?;
    let title = data.name.unwrap_or_default();
    let artist = data.artist.unwrap_or_default();

    if title.trim().is_empty() {
        return None;
    }

    let playing = data.status.unwrap_or(0) == 1;

    Some(NeteaseLocalStatus {
        progress_ms: progress.max(0) as u64,
        duration_ms: duration.max(0) as u64,
        id: data.id,
        title,
        artist,
        playing,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn get_player_status() -> Option<NeteaseLocalStatus> {
    None
}

