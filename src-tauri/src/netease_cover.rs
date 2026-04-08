use serde::Serialize;
use serde_json::Value;
use urlencoding::encode;

#[derive(Debug, Clone, Serialize)]
pub struct CoverUrlPayload {
    pub title: String,
    pub artist: String,
    pub url: Option<String>,
    pub source: String, // "netease"
}

fn add_hd_param(url: &str) -> String {
    // 网易云图片支持 ?param=WxH
    // 同时统一成 https，避免 WebView 混合内容拦截
    let mut u = url.to_string();
    if u.starts_with("http://") {
        u = u.replacen("http://", "https://", 1);
    }
    if u.contains("?param=") {
        u
    } else {
        format!("{u}?param=512y512")
    }
}

fn extract_pic_url(v: &Value) -> Option<String> {
    // 兼容不同接口返回结构：
    // - result.songs[0].album.picUrl
    // - result.songs[0].al.picUrl
    // - result.songs[0].al.picUrl（cloudsearch 常见）
    let songs = v.get("result")?.get("songs")?.as_array()?;
    let first = songs.first()?;

    // album.picUrl
    if let Some(s) = first
        .get("album")
        .and_then(|a| a.get("picUrl"))
        .and_then(|x| x.as_str())
    {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }

    // al.picUrl
    if let Some(s) = first
        .get("al")
        .and_then(|a| a.get("picUrl"))
        .and_then(|x| x.as_str())
    {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }

    None
}

fn fetch_json(url: &str, keyword: &str) -> Option<Value> {
    let req = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) IslandCore/0.1")
        .set("Accept", "application/json, text/plain, */*")
        .set("Referer", "https://music.163.com/")
        // 有些接口会根据 cookie 返回不同结构/拦截
        .set("Cookie", "os=pc; appver=2.0.0; channel=netease");

    let resp = req.call().ok()?;
    let status = resp.status();

    // ureq 在 into_json 失败时没有很好调试信息：先读成字符串再 parse
    let text = resp.into_string().ok()?;
    if status < 200 || status >= 300 {
        let head = text.chars().take(180).collect::<String>();
        println!("[cover:netease] http {status}, body_head={head:?} kw={keyword:?}");
        return None;
    }
    let v: Value = serde_json::from_str(&text).ok()?;
    Some(v)
}

#[cfg(target_os = "windows")]
pub fn fetch_netease_cover_url(title: &str, artist: &str) -> Option<String> {
    let title = title.trim();
    let artist = artist.trim();
    if title.is_empty() {
        return None;
    }

    let keyword = if artist.is_empty() {
        title.to_string()
    } else {
        format!("{title} {artist}")
    };

    println!("[cover:netease] search: {keyword:?}");

    // 多个入口兜底（网易云接口经常变/按地区拦截）
    let endpoints = [
        format!(
            "https://music.163.com/api/search/pc?s={}&type=1&limit=1",
            encode(&keyword)
        ),
        format!(
            "https://music.163.com/api/search/get?s={}&type=1&limit=1",
            encode(&keyword)
        ),
        format!(
            "https://music.163.com/api/cloudsearch/pc?s={}&type=1&limit=1",
            encode(&keyword)
        ),
    ];

    for url in endpoints {
        println!("[cover:netease] try: {url}");
        let v = fetch_json(&url, &keyword);
        let Some(v) = v else { continue };
        if let Some(pic) = extract_pic_url(&v) {
            let hd = add_hd_param(&pic);
            println!("[cover:netease] picUrl: {hd}");
            return Some(hd);
        }
        // 打印一下顶层结构，方便判断“字段变了”还是“返回了别的东西”
        let keys: Vec<String> = v
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        println!("[cover:netease] no picUrl, top_keys={keys:?}");
    }

    println!("[cover:netease] no picUrl (all endpoints)");
    None
}

#[cfg(not(target_os = "windows"))]
pub fn fetch_netease_cover_url(_title: &str, _artist: &str) -> Option<String> {
    None
}

