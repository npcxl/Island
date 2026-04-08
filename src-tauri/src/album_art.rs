use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AlbumArtPayload {
    pub url: Option<String>,
    pub base64: Option<String>,
    pub source: String,
}

/// 封面获取器 trait - 方便扩展新平台
trait AlbumArtFetcher {
    fn name(&self) -> &'static str;
    fn fetch(&self, title: &str, artist: &str, album: &str) -> Option<AlbumArtPayload>;
}

/// 封面获取管理器
pub struct AlbumArtManager {
    fetchers: Vec<Box<dyn AlbumArtFetcher + Send + Sync>>,
}

impl AlbumArtManager {
    pub fn new() -> Self {
        let mut fetchers: Vec<Box<dyn AlbumArtFetcher + Send + Sync>> = vec![];
        
        // 注册各个平台获取器（按优先级排序）
        fetchers.push(Box::new(NeteaseFetcher));
        fetchers.push(Box::new(QQMusicFetcher));
        fetchers.push(Box::new(SpotifyFetcher));
        fetchers.push(Box::new(ITunesFetcher));  // 通用 fallback
        fetchers.push(Box::new(LastFmFetcher));  // 备用 fallback
        
        Self { fetchers }
    }
    
    /// 根据来源优先尝试特定平台，失败后按优先级尝试其他平台
    pub fn fetch(&self, title: &str, artist: &str, album: &str, preferred_source: &str) -> Option<AlbumArtPayload> {
        let title = title.trim();
        let artist = artist.trim();
        
        if title.is_empty() {
            return None;
        }
        
        let source_lower = preferred_source.to_lowercase();
        
        // 1. 先尝试优先平台（如果匹配）
        for fetcher in &self.fetchers {
            if Self::source_matches(fetcher.name(), &source_lower) {
                if let Some(result) = fetcher.fetch(title, artist, album) {
                    return Some(result);
                }
                break; // 匹配的平台失败了，继续尝试其他
            }
        }
        
        // 2. 按优先级尝试其他平台
        for fetcher in &self.fetchers {
            // 跳过已经失败的平台
            if Self::source_matches(fetcher.name(), &source_lower) {
                continue;
            }
            if let Some(result) = fetcher.fetch(title, artist, album) {
                return Some(result);
            }
        }
        
        None
    }
    
    fn source_matches(fetcher_name: &str, source: &str) -> bool {
        match fetcher_name {
            "netease" => source.contains("netease") || source.contains("cloudmusic") || source.contains("orpheus"),
            "qqmusic" => source.contains("qqmusic"),
            "spotify" => source.contains("spotify"),
            _ => false,
        }
    }
}

impl Default for AlbumArtManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============== 各平台实现 ==============

struct NeteaseFetcher;
impl AlbumArtFetcher for NeteaseFetcher {
    fn name(&self) -> &'static str { "netease" }
    
    fn fetch(&self, title: &str, artist: &str, _album: &str) -> Option<AlbumArtPayload> {
        let q = if artist.is_empty() {
            urlencoding::encode(title).to_string()
        } else {
            format!("{} {}", urlencoding::encode(title), urlencoding::encode(artist))
        };
        
        let url = format!(
            "https://music.163.com/api/search/get/web?csrf_token=&hlpretag=&hlposttag=&s={}&type=1&offset=0&total=true&limit=5",
            q
        );
        
        let resp = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .set("Referer", "https://music.163.com/")
            .call()
            .ok()?;
        
        let json: serde_json::Value = resp.into_json().ok()?;
        let songs = json.get("result")?.get("songs")?.as_array()?;
        
        for song in songs {
            let album_id = song.get("album")?.get("id")?.as_i64()?;
            // 网易云封面 URL 格式
            let cover_url = format!(
                "https://p2.music.126.net/6y-UleORITEDbvrOLV0Q8A==/{}/{}.jpg?param=200y200",
                album_id / 100, album_id
            );
            
            return Some(AlbumArtPayload {
                url: Some(cover_url),
                base64: None,
                source: "netease".to_string(),
            });
        }
        
        None
    }
}

struct QQMusicFetcher;
impl AlbumArtFetcher for QQMusicFetcher {
    fn name(&self) -> &'static str { "qqmusic" }
    
    fn fetch(&self, title: &str, artist: &str, _album: &str) -> Option<AlbumArtPayload> {
        let q = if artist.is_empty() {
            urlencoding::encode(title).to_string()
        } else {
            format!("{} {}", urlencoding::encode(title), urlencoding::encode(artist))
        };
        
        let url = format!(
            "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?ct=24&qqmusic_ver=1298&new_json=1&remoteplace=txt.yqq.song&searchid=&t=0&aggr=1&cr=1&catZhida=1&lossless=0&flag_qc=0&p=1&n=5&w={}",
            q
        );
        
        let resp = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .set("Referer", "https://y.qq.com/")
            .call()
            .ok()?;
        
        let text = resp.into_string().ok()?;
        let json_str = text.trim_start_matches("callback(").trim_end_matches(")").trim();
        let json: serde_json::Value = serde_json::from_str(json_str).ok()?;
        
        let list = json.get("data")?.get("song")?.get("list")?.as_array()?;
        
        for song in list {
            let albummid = song.get("albummid")?.as_str()?;
            let cover_url = format!(
                "https://y.gtimg.cn/music/photo_new/T002R300x300M000{}.jpg",
                albummid
            );
            
            return Some(AlbumArtPayload {
                url: Some(cover_url),
                base64: None,
                source: "qqmusic".to_string(),
            });
        }
        
        None
    }
}

struct SpotifyFetcher;
impl AlbumArtFetcher for SpotifyFetcher {
    fn name(&self) -> &'static str { "spotify" }
    
    fn fetch(&self, title: &str, artist: &str, _album: &str) -> Option<AlbumArtPayload> {
        // Spotify 需要 OAuth2 认证，这里尝试使用无认证搜索
        // 实际使用可能需要用户自己配置 Token
        let q = if artist.is_empty() {
            urlencoding::encode(title).to_string()
        } else {
            format!("{} artist:{}", 
                urlencoding::encode(title),
                urlencoding::encode(artist))
        };
        
        let url = format!(
            "https://api.spotify.com/v1/search?q={}&type=track&limit=1",
            q
        );
        
        // 尝试访问，失败则返回 None，让下一个 fetcher 尝试
        if let Ok(resp) = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0")
            .call() {
            if let Ok(json) = resp.into_json::<serde_json::Value>() {
                if let Some(url) = json
                    .get("tracks")?
                    .get("items")?
                    .get(0)?
                    .get("album")?
                    .get("images")?
                    .get(0)?
                    .get("url")?
                    .as_str() {
                    return Some(AlbumArtPayload {
                        url: Some(url.to_string()),
                        base64: None,
                        source: "spotify".to_string(),
                    });
                }
            }
        }
        
        None
    }
}

struct ITunesFetcher;
impl AlbumArtFetcher for ITunesFetcher {
    fn name(&self) -> &'static str { "itunes" }
    
    fn fetch(&self, title: &str, artist: &str, _album: &str) -> Option<AlbumArtPayload> {
        let term = if artist.is_empty() {
            urlencoding::encode(title).to_string()
        } else {
            format!("{} {}", 
                urlencoding::encode(title),
                urlencoding::encode(artist))
        };
        
        let url = format!(
            "https://itunes.apple.com/search?term={}&media=music&entity=song&limit=5",
            term
        );
        
        let resp = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0")
            .call()
            .ok()?;
        
        let json: serde_json::Value = resp.into_json().ok()?;
        let results = json.get("results")?.as_array()?;
        
        for result in results {
            let artwork = result.get("artworkUrl100")?.as_str()?;
            let high_res = artwork.replace("100x100", "600x600");
            
            return Some(AlbumArtPayload {
                url: Some(high_res),
                base64: None,
                source: "itunes".to_string(),
            });
        }
        
        None
    }
}

struct LastFmFetcher;
impl AlbumArtFetcher for LastFmFetcher {
    fn name(&self) -> &'static str { "lastfm" }
    
    fn fetch(&self, title: &str, artist: &str, album: &str) -> Option<AlbumArtPayload> {
        // Last.fm 图片质量较低，作为最后的备选
        let method = if !album.is_empty() {
            format!("album.getinfo&album={}&artist={}",
                urlencoding::encode(album),
                urlencoding::encode(artist))
        } else {
            format!("track.getinfo&track={}&artist={}",
                urlencoding::encode(title),
                urlencoding::encode(artist))
        };
        
        let _url = format!(
            "https://ws.audioscrobbler.com/2.0/?method={}&api_key=YOUR_API_KEY&format=json",
            method
        );
        
        // 需要 API key，这里简化处理，实际使用时配置
        // 如果没有 API key，可以直接返回 None
        let _ = _url;
        None
    }
}

// ============== 公共函数 ==============

use std::sync::OnceLock;

static MANAGER: OnceLock<AlbumArtManager> = OnceLock::new();

fn get_manager() -> &'static AlbumArtManager {
    MANAGER.get_or_init(AlbumArtManager::new)
}

/// 下载图片并转为 base64
pub fn download_image_as_base64(url: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .call()
        .ok()?;
    
    let mut bytes = Vec::new();
    let mut reader = resp.into_reader();
    std::io::copy(&mut reader, &mut bytes).ok()?;
    
    if bytes.is_empty() {
        return None;
    }
    
    Some(STANDARD.encode(&bytes))
}

/// 获取专辑封面（自动选择平台）
pub fn fetch_album_art(title: &str, artist: &str, album: &str, source_hint: &str) -> Option<AlbumArtPayload> {
    get_manager().fetch(title, artist, album, source_hint)
}

/// 获取专辑封面（带下载）
pub fn fetch_album_art_with_download(title: &str, artist: &str, album: &str, source_hint: &str) -> Option<AlbumArtPayload> {
    println!("[album-art] fetching: {} - {} (source: {})", title, artist, source_hint);
    
    let mut result = fetch_album_art(title, artist, album, source_hint)?;
    
    println!("[album-art] found from: {}", result.source);
    
    if let Some(url) = &result.url {
        println!("[album-art] downloading: {}", url);
        if let Some(base64) = download_image_as_base64(url) {
            println!("[album-art] downloaded: {} bytes", base64.len() * 3 / 4);
            result.base64 = Some(base64);
        } else {
            println!("[album-art] download failed");
        }
    }
    
    Some(result)
}

/// 注册自定义封面获取器（供插件扩展）
pub fn register_fetcher(_fetcher: Box<dyn AlbumArtFetcher + Send + Sync>) {
    // 实际实现需要可变的 static，这里简化
    // 可以考虑使用 lazy_static + Mutex
}
