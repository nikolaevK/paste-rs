use crate::model::{ClipItem, ItemKind, NewItem, Payload, Pinboard, make_preview};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS pinboards (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    text TEXT NOT NULL DEFAULT '',
    rtf BLOB,
    html TEXT,
    title TEXT,
    color TEXT,
    image_path TEXT,
    thumb_path TEXT,
    favicon_path TEXT,
    files TEXT,
    app_bundle TEXT NOT NULL DEFAULT '',
    app_name TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    char_count INTEGER NOT NULL DEFAULT 0,
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0,
    byte_size INTEGER NOT NULL DEFAULT 0,
    hash TEXT NOT NULL DEFAULT '',
    pinboard_id INTEGER REFERENCES pinboards(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS items_created ON items(pinboard_id, created_at DESC);
CREATE INDEX IF NOT EXISTS items_hash ON items(hash);

CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
    text, title, app_name,
    content='items', content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS items_ai AFTER INSERT ON items BEGIN
    INSERT INTO items_fts(rowid, text, title, app_name)
    VALUES (new.id, new.text, new.title, new.app_name);
END;
CREATE TRIGGER IF NOT EXISTS items_ad AFTER DELETE ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, text, title, app_name)
    VALUES ('delete', old.id, old.text, old.title, old.app_name);
END;
CREATE TRIGGER IF NOT EXISTS items_au AFTER UPDATE OF text, title, app_name ON items BEGIN
    INSERT INTO items_fts(items_fts, rowid, text, title, app_name)
    VALUES ('delete', old.id, old.text, old.title, old.app_name);
    INSERT INTO items_fts(rowid, text, title, app_name)
    VALUES (new.id, new.text, new.title, new.app_name);
END;
"#;

fn row_to_item(row: &Row) -> rusqlite::Result<ClipItem> {
    let kind: String = row.get("kind")?;
    let text: String = row.get("text")?;
    let files: Option<String> = row.get("files")?;
    let files = files
        .and_then(|f| serde_json::from_str::<Vec<String>>(&f).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();
    Ok(ClipItem {
        id: row.get("id")?,
        kind: ItemKind::parse(&kind),
        preview: make_preview(&text),
        title: row.get("title")?,
        color: row.get("color")?,
        image_path: row.get::<_, Option<String>>("image_path")?.map(PathBuf::from),
        thumb_path: row.get::<_, Option<String>>("thumb_path")?.map(PathBuf::from),
        favicon_path: row.get::<_, Option<String>>("favicon_path")?.map(PathBuf::from),
        files,
        app_bundle: row.get("app_bundle")?,
        app_name: row.get("app_name")?,
        created_at: row.get("created_at")?,
        char_count: row.get("char_count")?,
        width: row.get("width")?,
        height: row.get("height")?,
        byte_size: row.get("byte_size")?,
        pinboard_id: row.get("pinboard_id")?,
        hash: row.get("hash")?,
    })
}

const ITEM_COLS: &str = "id, kind, substr(text, 1, 2400) AS text, title, color, image_path, thumb_path, favicon_path, files, app_bundle, app_name, created_at, char_count, width, height, byte_size, pinboard_id, hash";

impl Db {
    pub fn open(path: PathBuf) -> Result<Db> {
        let conn = Connection::open(&path).with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db { conn: Arc::new(Mutex::new(conn)) })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db { conn: Arc::new(Mutex::new(conn)) })
    }

    pub fn insert(&self, item: &NewItem) -> Result<ClipItem> {
        let conn = self.conn.lock().unwrap();
        let files = if item.files.is_empty() {
            None
        } else {
            Some(serde_json::to_string(
                &item.files.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
            )?)
        };
        let created = now_ms();
        conn.execute(
            "INSERT INTO items (kind, text, rtf, html, color, image_path, thumb_path, files, app_bundle, app_name, created_at, char_count, width, height, byte_size, hash, pinboard_id, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, NULL, 0)",
            params![
                item.kind.as_str(),
                item.text,
                item.rtf,
                item.html,
                item.color,
                item.image_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                item.thumb_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                files,
                item.app_bundle,
                item.app_name,
                created,
                item.char_count,
                item.width,
                item.height,
                item.byte_size,
                item.hash,
            ],
        )?;
        let id = conn.last_insert_rowid();
        Ok(ClipItem {
            id,
            kind: item.kind,
            preview: make_preview(&item.text),
            title: None,
            color: item.color.clone(),
            image_path: item.image_path.clone(),
            thumb_path: item.thumb_path.clone(),
            favicon_path: None,
            files: item.files.clone(),
            app_bundle: item.app_bundle.clone(),
            app_name: item.app_name.clone(),
            created_at: created,
            char_count: item.char_count,
            width: item.width,
            height: item.height,
            byte_size: item.byte_size,
            pinboard_id: None,
            hash: item.hash.clone(),
        })
    }

    /// All history items (not pinned), newest first.
    pub fn history(&self) -> Result<Vec<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ITEM_COLS} FROM items WHERE pinboard_id IS NULL ORDER BY created_at DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], row_to_item)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn pinboard_items(&self, pinboard_id: i64) -> Result<Vec<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {ITEM_COLS} FROM items WHERE pinboard_id = ?1 ORDER BY position ASC, created_at DESC"
        ))?;
        let rows = stmt.query_map([pinboard_id], row_to_item)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get(&self, id: i64) -> Result<Option<ClipItem>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(&format!("SELECT {ITEM_COLS} FROM items WHERE id = ?1"), [id], row_to_item)
            .optional()?)
    }

    pub fn payload(&self, id: i64) -> Result<Payload> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT text, rtf, html FROM items WHERE id = ?1",
            [id],
            |row| {
                Ok(Payload {
                    text: row.get(0)?,
                    rtf: row.get(1)?,
                    html: row.get(2)?,
                })
            },
        )?)
    }

    /// Full-text search returning matching item ids (any pinboard).
    pub fn search_ids(&self, query: &str) -> Result<Vec<i64>> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
            .collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let expr = tokens.join(" ");
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT rowid FROM items_fts WHERE items_fts MATCH ?1")?;
        let rows = stmt.query_map([expr], |r| r.get::<_, i64>(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn find_recent_by_hash(&self, hash: &str) -> Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT id FROM items WHERE hash = ?1 AND pinboard_id IS NULL ORDER BY created_at DESC LIMIT 1",
                [hash],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Moves an item to the top of the history by refreshing its timestamp.
    pub fn touch(&self, id: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = now_ms();
        conn.execute("UPDATE items SET created_at = ?1 WHERE id = ?2", params![now, id])?;
        Ok(now)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let paths = self.orphan_paths(id)?;
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM items WHERE id = ?1", [id])?;
        drop(conn);
        for p in paths {
            let _ = std::fs::remove_file(p);
        }
        Ok(())
    }

    /// Image files that are only referenced by this item.
    fn orphan_paths(&self, id: i64) -> Result<Vec<PathBuf>> {
        let conn = self.conn.lock().unwrap();
        let mut out = Vec::new();
        for col in ["image_path", "thumb_path"] {
            let path: Option<String> = conn
                .query_row(&format!("SELECT {col} FROM items WHERE id = ?1"), [id], |r| r.get(0))
                .optional()?
                .flatten();
            if let Some(path) = path {
                let refs: i64 = conn.query_row(
                    &format!("SELECT COUNT(*) FROM items WHERE {col} = ?1"),
                    [&path],
                    |r| r.get(0),
                )?;
                if refs <= 1 {
                    out.push(PathBuf::from(path));
                }
            }
        }
        Ok(out)
    }

    pub fn update_link_meta(&self, id: i64, title: Option<&str>, favicon: Option<&PathBuf>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE items SET title = COALESCE(?1, title), favicon_path = COALESCE(?2, favicon_path) WHERE id = ?3 OR (hash = (SELECT hash FROM items WHERE id = ?3))",
            params![title, favicon.map(|p| p.to_string_lossy().to_string()), id],
        )?;
        Ok(())
    }

    pub fn copy_to_pinboard(&self, id: i64, pinboard_id: i64) -> Result<ClipItem> {
        let conn = self.conn.lock().unwrap();
        let pos: i64 = conn.query_row(
            "SELECT COALESCE(MIN(position), 1) - 1 FROM items WHERE pinboard_id = ?1",
            [pinboard_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO items (kind, text, rtf, html, title, color, image_path, thumb_path, favicon_path, files, app_bundle, app_name, created_at, char_count, width, height, byte_size, hash, pinboard_id, position)
             SELECT kind, text, rtf, html, title, color, image_path, thumb_path, favicon_path, files, app_bundle, app_name, created_at, char_count, width, height, byte_size, hash, ?2, ?3 FROM items WHERE id = ?1",
            params![id, pinboard_id, pos],
        )?;
        let new_id = conn.last_insert_rowid();
        drop(conn);
        self.get(new_id)?.context("copied item missing")
    }

    pub fn pinboards(&self) -> Result<Vec<Pinboard>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, color, position FROM pinboards ORDER BY position ASC, id ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Pinboard { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, position: r.get(3)? })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn create_pinboard(&self, name: &str, color: &str) -> Result<Pinboard> {
        let conn = self.conn.lock().unwrap();
        let pos: i64 = conn.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM pinboards", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO pinboards (name, color, position) VALUES (?1, ?2, ?3)",
            params![name, color, pos],
        )?;
        Ok(Pinboard { id: conn.last_insert_rowid(), name: name.into(), color: color.into(), position: pos })
    }

    pub fn rename_pinboard(&self, id: i64, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE pinboards SET name = ?1 WHERE id = ?2", params![name, id])?;
        Ok(())
    }

    pub fn recolor_pinboard(&self, id: i64, color: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE pinboards SET color = ?1 WHERE id = ?2", params![color, id])?;
        Ok(())
    }

    pub fn delete_pinboard(&self, id: i64) -> Result<()> {
        let ids: Vec<i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id FROM items WHERE pinboard_id = ?1")?;
            let rows = stmt.query_map([id], |r| r.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        for item in ids {
            let _ = self.delete(item);
        }
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM pinboards WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_history(&self) -> Result<()> {
        let ids: Vec<i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id FROM items WHERE pinboard_id IS NULL")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        for id in ids {
            let _ = self.delete(id);
        }
        Ok(())
    }

    pub fn purge_older_than(&self, cutoff_ms: i64) -> Result<usize> {
        let ids: Vec<i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT id FROM items WHERE pinboard_id IS NULL AND created_at < ?1")?;
            let rows = stmt.query_map([cutoff_ms], |r| r.get(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };
        let n = ids.len();
        for id in ids {
            let _ = self.delete(id);
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_item(text: &str) -> NewItem {
        NewItem {
            kind: ItemKind::Text,
            text: text.into(),
            rtf: None,
            html: None,
            color: None,
            image_path: None,
            thumb_path: None,
            files: vec![],
            app_bundle: "com.test".into(),
            app_name: "Test".into(),
            char_count: text.chars().count() as i64,
            width: 0,
            height: 0,
            byte_size: text.len() as i64,
            hash: format!("{:x}", text.len()),
        }
    }

    #[test]
    fn insert_search_pin_delete() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert(&text_item("hello world")).unwrap();
        let _b = db.insert(&text_item("another entry")).unwrap();
        assert_eq!(db.history().unwrap().len(), 2);
        let ids = db.search_ids("hel").unwrap();
        assert_eq!(ids, vec![a.id]);
        let pb = db.create_pinboard("Snippets", "#007AFF").unwrap();
        let pinned = db.copy_to_pinboard(a.id, pb.id).unwrap();
        assert_eq!(pinned.pinboard_id, Some(pb.id));
        assert_eq!(db.pinboard_items(pb.id).unwrap().len(), 1);
        assert_eq!(db.history().unwrap().len(), 2);
        db.delete(a.id).unwrap();
        assert_eq!(db.history().unwrap().len(), 1);
        assert_eq!(db.search_ids("hel").unwrap(), vec![pinned.id]);
        db.delete_pinboard(pb.id).unwrap();
        assert!(db.pinboards().unwrap().is_empty());
        assert!(db.search_ids("hel").unwrap().is_empty());
    }
}
