use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use rusqlite::{params, Connection, Result};
use crate::models::{AppSettings, Folder, LibraryStats, Photo};

#[derive(Clone)]
pub struct DbManager {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct PhotoInsert {
    pub folder_id: i64,
    pub file_path: String,
    pub file_hash: String,
    pub file_name: String,
    pub file_size: i64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub date_taken: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub orientation: Option<u32>,
}

impl DbManager {
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path();
        if let Some(parent) = db_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)?;

        // WAL mode & high-performance SQLite settings for 500,000+ photos
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 268435456;
             PRAGMA cache_size = -64000;
             PRAGMA foreign_keys = ON;",
        )?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.create_tables()?;
        Ok(db)
    }

    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;",
        )?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.create_tables()?;
        Ok(db)
    }

    pub fn get_db_path() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pica")
            .join("pica.db")
    }

    fn create_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT UNIQUE NOT NULL,
                display_name TEXT NOT NULL,
                scan_mode TEXT NOT NULL DEFAULT 'always',
                last_scanned INTEGER,
                photo_count INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS photos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                file_path TEXT UNIQUE NOT NULL,
                file_hash TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                width INTEGER,
                height INTEGER,
                date_taken INTEGER,
                camera_make TEXT,
                camera_model TEXT,
                orientation INTEGER,
                is_favorite INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_photos_folder ON photos(folder_id);
            CREATE INDEX IF NOT EXISTS idx_photos_date ON photos(date_taken DESC);
            CREATE INDEX IF NOT EXISTS idx_photos_hash ON photos(file_hash);",
        )?;

        Ok(())
    }

    pub fn get_folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id, f.path, f.display_name, f.scan_mode, f.last_scanned,
                    (SELECT COUNT(*) FROM photos p WHERE p.folder_id = f.id) as photo_count
             FROM folders f
             ORDER BY f.display_name ASC",
        )?;

        let folders_iter = stmt.query_map([], |row| {
            Ok(Folder {
                id: row.get(0)?,
                path: row.get(1)?,
                display_name: row.get(2)?,
                scan_mode: row.get(3)?,
                last_scanned: row.get(4)?,
                photo_count: row.get(5)?,
            })
        })?;

        let mut folders = Vec::new();
        for folder in folders_iter {
            folders.push(folder?);
        }
        Ok(folders)
    }

    pub fn add_or_update_folder(&self, path: &str, display_name: &str, scan_mode: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO folders (path, display_name, scan_mode)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
                display_name = excluded.display_name,
                scan_mode = excluded.scan_mode",
            params![path, display_name, scan_mode],
        )?;

        let id = conn.query_row(
            "SELECT id FROM folders WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn update_folder_mode(&self, folder_id: i64, scan_mode: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE folders SET scan_mode = ?1 WHERE id = ?2",
            params![scan_mode, folder_id],
        )?;
        Ok(())
    }

    pub fn remove_folder(&self, folder_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM photos WHERE folder_id = ?1", params![folder_id])?;
        conn.execute("DELETE FROM folders WHERE id = ?1", params![folder_id])?;
        Ok(())
    }

    pub fn update_folder_scanned(&self, folder_id: i64, count: i64, timestamp: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE folders SET last_scanned = ?1, photo_count = ?2 WHERE id = ?3",
            params![timestamp, count, folder_id],
        )?;
        Ok(())
    }

    pub fn batch_upsert_photos(&self, items: &[PhotoInsert]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO photos (
                    folder_id, file_path, file_hash, file_name, file_size,
                    width, height, date_taken, camera_make, camera_model, orientation
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(file_path) DO UPDATE SET
                    folder_id = excluded.folder_id,
                    file_hash = excluded.file_hash,
                    file_size = excluded.file_size,
                    width = excluded.width,
                    height = excluded.height,
                    date_taken = excluded.date_taken,
                    camera_make = excluded.camera_make,
                    camera_model = excluded.camera_model,
                    orientation = excluded.orientation",
            )?;

            for p in items {
                stmt.execute(params![
                    p.folder_id,
                    p.file_path,
                    p.file_hash,
                    p.file_name,
                    p.file_size,
                    p.width,
                    p.height,
                    p.date_taken,
                    p.camera_make,
                    p.camera_model,
                    p.orientation
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_photos_for_folder(&self, folder_id: i64) -> Result<Vec<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, file_path, file_hash, file_name, file_size,
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite
             FROM photos
             WHERE folder_id = ?1
               AND file_path NOT LIKE '%/.pica_originals/%'
               AND file_path NOT LIKE '%/.picasaoriginals/%'
             ORDER BY date_taken DESC, file_name ASC",
        )?;

        let iter = stmt.query_map(params![folder_id], Self::map_photo_row)?;
        let mut photos = Vec::new();
        for p in iter {
            photos.push(p?);
        }
        Ok(photos)
    }

    pub fn get_photo_by_id(&self, photo_id: i64) -> Result<Option<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, file_path, file_hash, file_name, file_size,
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite
             FROM photos WHERE id = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![photo_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_photo_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_photo_by_path(&self, file_path: &str) -> Result<Option<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, file_path, file_hash, file_name, file_size,
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite
             FROM photos WHERE file_path = ?1 LIMIT 1",
        )?;
        let mut rows = stmt.query(params![file_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::map_photo_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn update_photo_after_edit(
        &self,
        id: i64,
        new_hash: &str,
        new_size: i64,
        width: Option<u32>,
        height: Option<u32>,
        orientation: Option<u32>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE photos SET
                file_hash = ?1,
                file_size = ?2,
                width = ?3,
                height = ?4,
                orientation = ?5
             WHERE id = ?6",
            params![new_hash, new_size, width, height, orientation, id],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_app_settings(&self) -> Result<AppSettings> {
        let original_handling = self
            .get_setting("original_handling")?
            .unwrap_or_else(|| "backup".to_string());
        let hide_originals = self
            .get_setting("hide_originals")?
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        Ok(AppSettings {
            original_handling,
            hide_originals,
        })
    }

    pub fn update_app_settings(&self, settings: &AppSettings) -> Result<()> {
        self.set_setting("original_handling", &settings.original_handling)?;
        self.set_setting("hide_originals", if settings.hide_originals { "true" } else { "false" })?;
        Ok(())
    }

    pub fn get_photo_path_by_hash(&self, file_hash: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT file_path FROM photos WHERE file_hash = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![file_hash])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_all_photos(&self, limit: i64, offset: i64) -> Result<Vec<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, file_path, file_hash, file_name, file_size,
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite
             FROM photos
             WHERE file_path NOT LIKE '%/.pica_originals/%'
               AND file_path NOT LIKE '%/.picasaoriginals/%'
             ORDER BY date_taken DESC, id DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let iter = stmt.query_map(params![limit, offset], Self::map_photo_row)?;
        let mut photos = Vec::new();
        for p in iter {
            photos.push(p?);
        }
        Ok(photos)
    }

    pub fn toggle_favorite(&self, photo_id: i64) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let current: i32 = conn.query_row(
            "SELECT is_favorite FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )?;
        let new_state = if current == 1 { 0 } else { 1 };
        conn.execute(
            "UPDATE photos SET is_favorite = ?1 WHERE id = ?2",
            params![new_state, photo_id],
        )?;
        Ok(new_state == 1)
    }

    pub fn get_stats(&self, is_scanning: bool) -> Result<LibraryStats> {
        let conn = self.conn.lock().unwrap();
        let total_photos: i64 = conn.query_row(
            "SELECT COUNT(*) FROM photos",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let total_folders: i64 = conn.query_row(
            "SELECT COUNT(*) FROM folders WHERE scan_mode != 'exclude'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let total_favorites: i64 = conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE is_favorite = 1",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        Ok(LibraryStats {
            total_photos,
            total_folders,
            total_favorites,
            is_scanning,
        })
    }

    fn map_photo_row(row: &rusqlite::Row) -> Result<Photo> {
        let fav_int: i32 = row.get(12)?;
        Ok(Photo {
            id: row.get(0)?,
            folder_id: row.get(1)?,
            file_path: row.get(2)?,
            file_hash: row.get(3)?,
            file_name: row.get(4)?,
            file_size: row.get(5)?,
            width: row.get(6)?,
            height: row.get(7)?,
            date_taken: row.get(8)?,
            camera_make: row.get(9)?,
            camera_model: row.get(10)?,
            orientation: row.get(11)?,
            is_favorite: fav_int == 1,
            thumbnail_path: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_init_and_operations() {
        let db = DbManager::new_in_memory().expect("in-memory db init failed");
        
        // 1. Add folder
        let folder_id = db.add_or_update_folder("/test/photos", "Photos", "always")
            .expect("add folder failed");
        assert!(folder_id > 0);

        let folders = db.get_folders().expect("get folders failed");
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].display_name, "Photos");

        // 2. Batch insert photos
        let photos = vec![
            PhotoInsert {
                folder_id,
                file_path: "/test/photos/img1.jpg".to_string(),
                file_hash: "hash123".to_string(),
                file_name: "img1.jpg".to_string(),
                file_size: 1024,
                width: Some(1920),
                height: Some(1080),
                date_taken: Some(1700000000000),
                camera_make: Some("Canon".to_string()),
                camera_model: Some("EOS R5".to_string()),
                orientation: Some(1),
            },
            PhotoInsert {
                folder_id,
                file_path: "/test/photos/img2.jpg".to_string(),
                file_hash: "hash456".to_string(),
                file_name: "img2.jpg".to_string(),
                file_size: 2048,
                width: Some(3840),
                height: Some(2160),
                date_taken: Some(1700000001000),
                camera_make: Some("Sony".to_string()),
                camera_model: Some("A7IV".to_string()),
                orientation: Some(1),
            },
        ];

        db.batch_upsert_photos(&photos).expect("batch upsert failed");

        let fetched = db.get_photos_for_folder(folder_id).expect("fetch photos failed");
        assert_eq!(fetched.len(), 2);
        // Ordered by date_taken DESC
        assert_eq!(fetched[0].file_name, "img2.jpg");
        assert_eq!(fetched[1].file_name, "img1.jpg");

        // 3. Toggle favorite
        let fav = db.toggle_favorite(fetched[0].id).expect("toggle favorite failed");
        assert!(fav);

        // 4. Verify stats
        let stats = db.get_stats(false).expect("get stats failed");
        assert_eq!(stats.total_photos, 2);
        assert_eq!(stats.total_folders, 1);
        assert_eq!(stats.total_favorites, 1);
    }
}
