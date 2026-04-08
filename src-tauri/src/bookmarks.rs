//! 网站收藏：本地 SQLite（应用数据目录 `bookmarks.sqlite`）。

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::AppHandle;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub created_at: i64,
}

fn db_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create_dir: {e}"))?;
    Ok(dir.join("bookmarks.sqlite"))
}

fn open_conn(app: &AppHandle) -> Result<Connection, String> {
    let p = db_path(app)?;
    let c = Connection::open(p).map_err(|e| e.to_string())?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL,
            title TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE(url)
        );",
    )
    .map_err(|e| e.to_string())?;
    Ok(c)
}

#[tauri::command]
pub fn bookmark_list(app: AppHandle) -> Result<Vec<Bookmark>, String> {
    let c = open_conn(&app)?;
    let mut stmt = c
        .prepare("SELECT id, url, title, created_at FROM bookmarks ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Bookmark {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[tauri::command]
pub fn bookmark_add(app: AppHandle, url: String, title: String) -> Result<Bookmark, String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("url 为空".into());
    }
    let title = title.trim().to_string();
    let title = if title.is_empty() {
        url.clone()
    } else {
        title
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let c = open_conn(&app)?;
    c.execute(
        "INSERT INTO bookmarks (url, title, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(url) DO UPDATE SET title = excluded.title",
        params![&url, &title, now],
    )
    .map_err(|e| e.to_string())?;

    let id: i64 = c
        .query_row("SELECT id FROM bookmarks WHERE url = ?1", params![&url], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;

    Ok(Bookmark {
        id,
        url,
        title,
        created_at: now,
    })
}

#[tauri::command]
pub fn bookmark_remove(app: AppHandle, id: i64) -> Result<(), String> {
    let c = open_conn(&app)?;
    c.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn bookmark_exists(app: AppHandle, url: String) -> Result<bool, String> {
    let url = url.trim();
    if url.is_empty() {
        return Ok(false);
    }
    let c = open_conn(&app)?;
    let n: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM bookmarks WHERE url = ?1",
            params![url],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

#[tauri::command]
pub fn bookmark_open_url(url: String) -> bool {
    let u = url.trim();
    if u.is_empty() {
        return false;
    }
    crate::system_actions::open_external_url(u)
}
