use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use rusqlite::{params, Connection, Result};
use crate::models::{AppSettings, FaceBox, FaceCollection, Folder, LibraryStats, Photo};

#[derive(Clone)]
pub struct DbManager {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
pub struct FaceInsertData {
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub confidence: f32,
    pub embedding: Option<Vec<f32>>,
    pub person_id: Option<i64>,
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
            CREATE INDEX IF NOT EXISTS idx_photos_hash ON photos(file_hash);

            CREATE TABLE IF NOT EXISTS face_persons (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                cover_face_id INTEGER,
                photo_count INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS faces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                person_id INTEGER REFERENCES face_persons(id) ON DELETE SET NULL,
                box_x REAL NOT NULL,
                box_y REAL NOT NULL,
                box_w REAL NOT NULL,
                box_h REAL NOT NULL,
                confidence REAL NOT NULL,
                embedding BLOB,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);
            CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);
            CREATE INDEX IF NOT EXISTS idx_face_persons_count ON face_persons(photo_count DESC, updated_at DESC);",
        )?;

        let _ = conn.execute("ALTER TABLE photos ADD COLUMN edit_rotation INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE photos ADD COLUMN has_unsaved_edits INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE folders ADD COLUMN face_detect_enabled INTEGER DEFAULT 1", []);
        let _ = conn.execute("ALTER TABLE photos ADD COLUMN face_scanned INTEGER DEFAULT 0", []);

        // Auto-heal / sanitize corrupt heuristic face records if duplicate "Person 1" entries exist
        let duplicate_person_ones: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM face_persons WHERE name = 'Person 1'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if duplicate_person_ones > 1 {
            log::info!("Detected {} duplicate 'Person 1' records from previous heuristic detector; resetting face tables...", duplicate_person_ones);
            let _ = conn.execute("DELETE FROM faces", []);
            let _ = conn.execute("DELETE FROM face_persons", []);
            let _ = conn.execute("UPDATE photos SET face_scanned = 0", []);
        }

        // Upgrade face embeddings to InsightFace ArcFace MobileFaceNet (512-D, w600k_mbf):
        // Automatically purges old 128-D embeddings and resets photos for clean, accurate re-scanning
        let current_embedder_ver = "arcface_mbf_v1";
        let saved_ver: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'face_embedder_version'",
                [],
                |r| r.get(0),
            )
            .ok();
        if saved_ver.as_deref() != Some(current_embedder_ver) {
            log::info!("Upgrading face embeddings to InsightFace ArcFace MobileFaceNet; resetting face tables for clean re-scan...");
            let _ = conn.execute("DELETE FROM faces", []);
            let _ = conn.execute("DELETE FROM face_persons", []);
            let _ = conn.execute("UPDATE photos SET face_scanned = 0", []);
            let _ = conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('face_embedder_version', ?)",
                [current_embedder_ver],
            );
        }

        Ok(())
    }

    pub fn get_folders(&self) -> Result<Vec<Folder>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id, f.path, f.display_name, f.scan_mode, f.last_scanned,
                    (SELECT COUNT(*) FROM photos p WHERE p.folder_id = f.id) as photo_count,
                    COALESCE(f.face_detect_enabled, 1) as face_detect_enabled
             FROM folders f
             ORDER BY f.display_name ASC",
        )?;

        let folders_iter = stmt.query_map([], |row| {
            let face_enabled: i32 = row.get(6).unwrap_or(1);
            Ok(Folder {
                id: row.get(0)?,
                path: row.get(1)?,
                display_name: row.get(2)?,
                scan_mode: row.get(3)?,
                last_scanned: row.get(4)?,
                photo_count: row.get(5)?,
                face_detect_enabled: face_enabled != 0,
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

    pub fn enable_folder_face_detect(&self, folder_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE folders SET face_detect_enabled = 1 WHERE id = ?1",
            params![folder_id],
        )?;
        conn.execute(
            "UPDATE photos SET face_scanned = 0 WHERE folder_id = ?1 AND id NOT IN (SELECT photo_id FROM faces)",
            params![folder_id],
        )?;
        Ok(())
    }

    pub fn remove_folder_faces(&self, folder_id: i64) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // 1. Gather all affected person IDs before deleting faces
        let mut stmt = tx.prepare(
            "SELECT DISTINCT person_id FROM faces
             WHERE photo_id IN (SELECT id FROM photos WHERE folder_id = ?1)
               AND person_id IS NOT NULL",
        )?;
        let affected_person_ids: Vec<i64> = stmt
            .query_map(params![folder_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        // 2. Delete faces belonging to photos in this folder
        let deleted_count = tx.execute(
            "DELETE FROM faces WHERE photo_id IN (SELECT id FROM photos WHERE folder_id = ?1)",
            params![folder_id],
        )?;

        // 3. Reset face_scanned on photos in this folder so they can be re-scanned if re-enabled
        tx.execute(
            "UPDATE photos SET face_scanned = 0 WHERE folder_id = ?1",
            params![folder_id],
        )?;

        // 4. Set folder face_detect_enabled = 0
        tx.execute(
            "UPDATE folders SET face_detect_enabled = 0 WHERE id = ?1",
            params![folder_id],
        )?;

        // 5. Clean up affected person clusters
        let now = chrono::Utc::now().timestamp_millis();
        for pid in affected_person_ids {
            let remaining_count: i64 = tx.query_row(
                "SELECT COUNT(DISTINCT photo_id) FROM faces WHERE person_id = ?1",
                params![pid],
                |r| r.get(0),
            ).unwrap_or(0);

            if remaining_count == 0 {
                // Delete person since they have no faces remaining anywhere in the library
                tx.execute("DELETE FROM face_persons WHERE id = ?1", params![pid])?;
            } else {
                // Check if current cover_face_id still exists
                let cover_exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM faces WHERE id = (SELECT cover_face_id FROM face_persons WHERE id = ?1))",
                    params![pid],
                    |r| r.get(0),
                ).unwrap_or(false);

                if !cover_exists {
                    let new_cover: Option<i64> = tx.query_row(
                        "SELECT id FROM faces WHERE person_id = ?1 ORDER BY confidence DESC LIMIT 1",
                        params![pid],
                        |r| r.get(0),
                    ).ok();

                    tx.execute(
                        "UPDATE face_persons SET photo_count = ?1, cover_face_id = ?2, updated_at = ?3 WHERE id = ?4",
                        params![remaining_count, new_cover, now, pid],
                    )?;
                } else {
                    tx.execute(
                        "UPDATE face_persons SET photo_count = ?1, updated_at = ?2 WHERE id = ?3",
                        params![remaining_count, now, pid],
                    )?;
                }
            }
        }

        tx.commit()?;
        Ok(deleted_count)
    }

    pub fn update_folder_face_detect(&self, folder_id: i64, enabled: bool) -> Result<()> {
        if enabled {
            self.enable_folder_face_detect(folder_id)
        } else {
            self.remove_folder_faces(folder_id).map(|_| ())
        }
    }

    pub fn remove_folder(&self, folder_id: i64) -> Result<()> {
        let _ = self.remove_folder_faces(folder_id);
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
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite, edit_rotation, has_unsaved_edits
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
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite, edit_rotation, has_unsaved_edits
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
                    width, height, date_taken, camera_make, camera_model, orientation, is_favorite, edit_rotation, has_unsaved_edits
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
                orientation = ?5,
                edit_rotation = 0,
                has_unsaved_edits = 0
             WHERE id = ?6",
            params![new_hash, new_size, width, height, orientation, id],
        )?;
        Ok(())
    }

    pub fn update_photo_edit_rotation(&self, photo_id: i64, rotation: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE photos SET edit_rotation = ?1 WHERE id = ?2",
            params![rotation, photo_id],
        )?;
        Ok(())
    }

    pub fn update_photo_unsaved_edits(&self, photo_id: i64, has_unsaved: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let val = if has_unsaved { 1 } else { 0 };
        conn.execute(
            "UPDATE photos SET has_unsaved_edits = ?1 WHERE id = ?2",
            params![val, photo_id],
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

    pub fn get_face_collections(&self) -> Result<Vec<FaceCollection>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT fp.id, fp.name, fp.cover_face_id,
                    (SELECT p.file_hash FROM faces f JOIN photos p ON f.photo_id = p.id WHERE f.id = fp.cover_face_id LIMIT 1) as cover_photo_hash,
                    fp.photo_count, fp.updated_at
             FROM face_persons fp
             WHERE fp.photo_count > 0
             ORDER BY fp.photo_count DESC, fp.updated_at DESC",
        )?;

        let iter = stmt.query_map([], |row| {
            Ok(FaceCollection {
                id: row.get(0)?,
                name: row.get(1)?,
                cover_face_id: row.get(2)?,
                cover_photo_hash: row.get(3)?,
                photo_count: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;

        let mut collections = Vec::new();
        for item in iter {
            collections.push(item?);
        }
        Ok(collections)
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<FaceBox>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id, f.photo_id, f.person_id, fp.name,
                    f.box_x, f.box_y, f.box_w, f.box_h, f.confidence
             FROM faces f
             LEFT JOIN face_persons fp ON f.person_id = fp.id
             WHERE f.photo_id = ?1
             ORDER BY f.id ASC",
        )?;

        let iter = stmt.query_map(params![photo_id], |row| {
            Ok(FaceBox {
                id: row.get(0)?,
                photo_id: row.get(1)?,
                person_id: row.get(2)?,
                person_name: row.get(3)?,
                box_x: row.get(4)?,
                box_y: row.get(5)?,
                box_w: row.get(6)?,
                box_h: row.get(7)?,
                confidence: row.get(8)?,
            })
        })?;

        let mut faces = Vec::new();
        for item in iter {
            faces.push(item?);
        }
        Ok(faces)
    }

    pub fn get_photos_for_person(&self, person_id: i64) -> Result<Vec<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT p.id, p.folder_id, p.file_path, p.file_hash, p.file_name, p.file_size,
                    p.width, p.height, p.date_taken, p.camera_make, p.camera_model, p.orientation,
                    p.is_favorite, p.edit_rotation, p.has_unsaved_edits
             FROM photos p
             JOIN faces f ON f.photo_id = p.id
             WHERE f.person_id = ?1
               AND p.file_path NOT LIKE '%/.pica_originals/%'
               AND p.file_path NOT LIKE '%/.picasaoriginals/%'
             ORDER BY p.date_taken DESC, p.file_name ASC",
        )?;

        let iter = stmt.query_map(params![person_id], Self::map_photo_row)?;
        let mut photos = Vec::new();
        for p in iter {
            photos.push(p?);
        }
        Ok(photos)
    }

    pub fn update_person_name(&self, person_id: i64, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE face_persons SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, chrono::Utc::now().timestamp_millis(), person_id],
        )?;
        Ok(())
    }

    pub fn get_pending_face_photos(&self, limit: usize) -> Result<Vec<Photo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.folder_id, p.file_path, p.file_hash, p.file_name, p.file_size,
                    p.width, p.height, p.date_taken, p.camera_make, p.camera_model, p.orientation,
                    p.is_favorite, p.edit_rotation, p.has_unsaved_edits
             FROM photos p
             JOIN folders f ON p.folder_id = f.id
             WHERE f.scan_mode != 'exclude'
               AND COALESCE(f.face_detect_enabled, 1) = 1
               AND COALESCE(p.face_scanned, 0) = 0
               AND p.file_path NOT LIKE '%/.pica_originals/%'
               AND p.file_path NOT LIKE '%/.picasaoriginals/%'
             ORDER BY p.id ASC
             LIMIT ?1",
        )?;

        let iter = stmt.query_map(params![limit as i64], Self::map_photo_row)?;
        let mut photos = Vec::new();
        for p in iter {
            photos.push(p?);
        }
        Ok(photos)
    }

    pub fn get_face_counts(&self) -> Result<(i64, i64, i64)> {
        let conn = self.conn.lock().unwrap();
        let total_faces: i64 = conn
            .query_row("SELECT COUNT(*) FROM faces", [], |r| r.get(0))
            .unwrap_or(0);
        let total_people: i64 = conn
            .query_row("SELECT COUNT(*) FROM face_persons WHERE photo_count > 0", [], |r| r.get(0))
            .unwrap_or(0);
        let pending: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM photos p
                 JOIN folders f ON p.folder_id = f.id
                 WHERE f.scan_mode != 'exclude'
                   AND COALESCE(f.face_detect_enabled, 1) = 1
                   AND COALESCE(p.face_scanned, 0) = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok((total_faces, total_people, pending))
    }

    pub fn get_next_person_number(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM face_persons WHERE name LIKE 'Person %'")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut used = std::collections::HashSet::new();
        for name in names.flatten() {
            if let Some(num_str) = name.strip_prefix("Person ") {
                if let Ok(num) = num_str.parse::<i64>() {
                    used.insert(num);
                }
            }
        }
        let mut next = 1;
        while used.contains(&next) {
            next += 1;
        }
        Ok(next)
    }

    pub fn get_person_centroids(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT person_id, embedding FROM faces
             WHERE person_id IS NOT NULL AND embedding IS NOT NULL
             ORDER BY person_id, confidence DESC",
        )?;
        let mut person_map: std::collections::HashMap<i64, Vec<Vec<f32>>> = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            let pid: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let mut vec = Vec::with_capacity(blob.len() / 4);
            for chunk in blob.chunks_exact(4) {
                vec.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            Ok((pid, vec))
        })?;

        for r in rows {
            let (pid, emb) = r?;
            if emb.is_empty() {
                continue;
            }
            let list = person_map.entry(pid).or_insert_with(Vec::new);
            if list.len() < 12 {
                list.push(emb);
            }
        }

        let mut results = Vec::with_capacity(person_map.len());
        for (pid, face_embs) in person_map {
            if face_embs.is_empty() {
                continue;
            }
            if face_embs.len() == 1 {
                results.push((pid, face_embs[0].clone()));
                continue;
            }

            // Find medoid (the face with highest total similarity to peers)
            let mut best_medoid_idx = 0;
            let mut best_sim_sum = -1.0f32;
            for (i, e1) in face_embs.iter().enumerate() {
                let mut sum = 0.0f32;
                for (j, e2) in face_embs.iter().enumerate() {
                    if i != j {
                        sum += crate::faces::clustering::CosineFaceClusterer::cosine_similarity(e1, e2);
                    }
                }
                if sum > best_sim_sum {
                    best_sim_sum = sum;
                    best_medoid_idx = i;
                }
            }

            let medoid = &face_embs[best_medoid_idx];
            // Average only faces consistent with the medoid (similarity >= 0.40) to prevent centroid drift
            let mut sum_vec = vec![0.0f32; medoid.len()];
            let mut count = 0.0f32;
            for emb in &face_embs {
                let sim = crate::faces::clustering::CosineFaceClusterer::cosine_similarity(medoid, emb);
                if sim >= 0.40 && emb.len() == sum_vec.len() {
                    for (sv, ev) in sum_vec.iter_mut().zip(emb.iter()) {
                        *sv += *ev;
                    }
                    count += 1.0;
                }
            }

            if count > 0.0 {
                let norm: f32 = sum_vec.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
                for sv in sum_vec.iter_mut() {
                    *sv /= norm;
                }
                results.push((pid, sum_vec));
            } else {
                results.push((pid, medoid.clone()));
            }
        }
        Ok(results)
    }

    pub fn recluster_all_faces(&self, threshold: f32) -> Result<usize> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let mut stmt = tx.prepare(
            "SELECT id, photo_id, embedding FROM faces WHERE embedding IS NOT NULL ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let photo_id: i64 = row.get(1)?;
            let blob: Vec<u8> = row.get(2)?;
            let mut vec = Vec::with_capacity(blob.len() / 4);
            for chunk in blob.chunks_exact(4) {
                vec.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
            Ok((id, photo_id, vec))
        })?;

        let faces: Vec<(i64, i64, Vec<f32>)> = rows.filter_map(|r| r.ok()).collect();
        drop(stmt);

        if faces.is_empty() {
            return Ok(0);
        }

        let effective_threshold = if threshold > 0.15 { threshold } else { 0.50 };

        let raw_clusters = {
            let clusterer = yunet_face::SowGrowClusterer::new(yunet_face::SowGrowConfig {
                sow_threshold: (effective_threshold + 0.03).min(0.95),
                grow_threshold: effective_threshold,
                sort_threshold: 0.50,
                max_recommendations: 50,
                cannot_link: true,
                exemplar_weight: 0.70,
            });
            let items: Vec<yunet_face::FaceClusterItem> = faces
                .iter()
                .map(|(fid, pid, emb)| yunet_face::FaceClusterItem {
                    face_id: *fid,
                    photo_id: *pid,
                    embedding: emb.clone(),
                })
                .collect();
            let result = clusterer.cluster(&items);
            let mut clusters: Vec<Vec<i64>> = result.clusters.into_iter().map(|c| c.face_ids).collect();
            for unassigned in result.unassigned_face_ids {
                clusters.push(vec![unassigned]);
            }
            clusters
        };

        // Precompute quick lookup for face_id -> (photo_id, &embedding)
        let face_map: std::collections::HashMap<i64, (i64, &Vec<f32>)> = faces
            .iter()
            .map(|(fid, pid, emb)| (*fid, (*pid, emb)))
            .collect();

        // Strict Cannot-Link post-processing: guarantee that no photo has multiple faces in the same person cluster
        let mut sanitized_clusters: Vec<Vec<i64>> = Vec::with_capacity(raw_clusters.len());
        for cluster in raw_clusters {
            if cluster.is_empty() {
                continue;
            }
            let mut seen_photos = std::collections::HashSet::new();
            let mut valid_cluster = Vec::with_capacity(cluster.len());
            let mut overflow_faces = Vec::new();

            for fid in cluster {
                if let Some((pid, _)) = face_map.get(&fid) {
                    if seen_photos.insert(*pid) {
                        valid_cluster.push(fid);
                    } else {
                        overflow_faces.push(fid);
                    }
                }
            }
            if !valid_cluster.is_empty() {
                sanitized_clusters.push(valid_cluster);
            }
            for of_fid in overflow_faces {
                sanitized_clusters.push(vec![of_fid]);
            }
        }

        // Compute metadata for each cluster (photo_count, medoid face)
        struct ClusterMeta {
            fids: Vec<i64>,
            photo_count: i64,
            medoid_fid: i64,
        }

        let mut cluster_metas: Vec<ClusterMeta> = Vec::with_capacity(sanitized_clusters.len());

        for fids in sanitized_clusters {
            let mut unique_photos = std::collections::HashSet::new();
            for &fid in &fids {
                if let Some((pid, _)) = face_map.get(&fid) {
                    unique_photos.insert(*pid);
                }
            }
            let photo_count = unique_photos.len() as i64;

            // Medoid calculation: face with maximum average similarity to other faces in the cluster
            let medoid_fid = if fids.len() == 1 {
                fids[0]
            } else {
                let sample_fids = if fids.len() > 32 {
                    &fids[..32]
                } else {
                    &fids[..]
                };

                let mut best_fid = fids[0];
                let mut best_score = -1.0f32;

                for &fid_i in sample_fids {
                    if let Some((_, emb_i)) = face_map.get(&fid_i) {
                        let mut score = 0.0f32;
                        for &fid_j in sample_fids {
                            if fid_i != fid_j {
                                if let Some((_, emb_j)) = face_map.get(&fid_j) {
                                    score += crate::faces::clustering::CosineFaceClusterer::cosine_similarity(emb_i, emb_j);
                                }
                            }
                        }
                        if score > best_score {
                            best_score = score;
                            best_fid = fid_i;
                        }
                    }
                }
                best_fid
            };

            cluster_metas.push(ClusterMeta {
                fids,
                photo_count,
                medoid_fid,
            });
        }

        // Sort clusters by photo count descending, then face count descending
        cluster_metas.sort_by(|a, b| {
            b.photo_count
                .cmp(&a.photo_count)
                .then_with(|| b.fids.len().cmp(&a.fids.len()))
        });

        tx.execute("UPDATE faces SET person_id = NULL", [])?;
        tx.execute("DELETE FROM face_persons", [])?;

        let now = chrono::Utc::now().timestamp_millis();
        let mut created_people = 0usize;

        for (rank, meta) in cluster_metas.into_iter().enumerate() {
            let person_name = format!("Person {}", rank + 1);

            tx.execute(
                "INSERT INTO face_persons (name, cover_face_id, photo_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![person_name, meta.medoid_fid, meta.photo_count, now, now],
            )?;
            let person_id = tx.last_insert_rowid();

            for fid in meta.fids {
                tx.execute(
                    "UPDATE faces SET person_id = ?1 WHERE id = ?2",
                    rusqlite::params![person_id, fid],
                )?;
            }
            created_people += 1;
        }

        let _ = tx.execute(
            "INSERT INTO settings (key, value) VALUES ('face_cluster_threshold', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![effective_threshold.to_string()],
        );

        tx.commit()?;
        Ok(created_people)
    }

    pub fn create_face_person(&self, name: &str, cover_face_id: Option<i64>) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO face_persons (name, cover_face_id, photo_count, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![name, cover_face_id, now, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn save_detected_faces(&self, photo_id: i64, faces: &[FaceInsertData]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().timestamp_millis();

        // Verify that the folder for this photo still has face detection enabled
        let is_folder_face_enabled: bool = tx
            .query_row(
                "SELECT COALESCE(f.face_detect_enabled, 1) FROM photos p JOIN folders f ON p.folder_id = f.id WHERE p.id = ?1",
                params![photo_id],
                |r| r.get::<_, i32>(0),
            )
            .unwrap_or(0) != 0;

        if !is_folder_face_enabled {
            return Ok(());
        }

        tx.execute(
            "UPDATE photos SET face_scanned = 1 WHERE id = ?1",
            params![photo_id],
        )?;

        for f in faces {
            let embedding_bytes = f.embedding.as_ref().map(|emb| {
                let mut bytes = Vec::with_capacity(emb.len() * 4);
                for val in emb {
                    bytes.extend_from_slice(&val.to_le_bytes());
                }
                bytes
            });

            tx.execute(
                "INSERT INTO faces (photo_id, person_id, box_x, box_y, box_w, box_h, confidence, embedding, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    photo_id,
                    f.person_id,
                    f.box_x,
                    f.box_y,
                    f.box_w,
                    f.box_h,
                    f.confidence,
                    embedding_bytes,
                    now
                ],
            )?;

            let face_id = tx.last_insert_rowid();

            if let Some(pid) = f.person_id {
                tx.execute(
                    "UPDATE face_persons SET
                        cover_face_id = COALESCE(cover_face_id, ?1),
                        photo_count = (SELECT COUNT(DISTINCT photo_id) FROM faces WHERE person_id = ?2),
                        updated_at = ?3
                     WHERE id = ?2",
                    params![face_id, pid, now],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn update_face_embeddings(&self, updates: &[(i64, Vec<f32>)]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut stmt = tx.prepare("UPDATE faces SET embedding = ?1 WHERE id = ?2")?;

        for (face_id, emb) in updates {
            let mut bytes = Vec::with_capacity(emb.len() * 4);
            for val in emb {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            stmt.execute(params![bytes, face_id])?;
        }
        drop(stmt);
        tx.commit()?;
        Ok(())
    }

    pub fn get_faces_for_realignment(&self) -> Result<Vec<(i64, i64, String, String, f32, f32, f32, f32)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT f.id, f.photo_id, p.file_path, p.file_hash, f.box_x, f.box_y, f.box_w, f.box_h
             FROM faces f
             JOIN photos p ON f.photo_id = p.id
             ORDER BY f.photo_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f32>(4)?,
                row.get::<_, f32>(5)?,
                row.get::<_, f32>(6)?,
                row.get::<_, f32>(7)?,
            ))
        })?;
        let mut list = Vec::new();
        for r in rows {
            if let Ok(item) = r {
                list.push(item);
            }
        }
        Ok(list)
    }

    pub fn get_face_chip_bytes(&self, face_id: i64) -> Result<Option<Vec<u8>>> {
        let (file_path, box_x, box_y, box_w, box_h) = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT p.file_path, f.box_x, f.box_y, f.box_w, f.box_h
                 FROM faces f
                 JOIN photos p ON f.photo_id = p.id
                 WHERE f.id = ?1",
            )?;
            let mut rows = stmt.query(params![face_id])?;
            if let Some(row) = rows.next()? {
                let path: String = row.get(0)?;
                let x: f32 = row.get(1)?;
                let y: f32 = row.get(2)?;
                let w: f32 = row.get(3)?;
                let h: f32 = row.get(4)?;
                (path, x, y, w, h)
            } else {
                return Ok(None);
            }
        };

        if let Ok(img) = image::open(&file_path) {
            let img_w = img.width() as f32;
            let img_h = img.height() as f32;

            let pad_x = box_w * 0.25;
            let pad_y = box_h * 0.25;

            let crop_x = ((box_x - pad_x).max(0.0) * img_w) as u32;
            let crop_y = ((box_y - pad_y).max(0.0) * img_h) as u32;
            let crop_w = (((box_w + 2.0 * pad_x) * img_w) as u32).min(img.width().saturating_sub(crop_x));
            let crop_h = (((box_h + 2.0 * pad_y) * img_h) as u32).min(img.height().saturating_sub(crop_y));

            if crop_w > 0 && crop_h > 0 {
                let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h);
                let resized = cropped.resize_exact(160, 160, image::imageops::FilterType::Triangle);
                let mut jpeg_bytes = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
                if resized.write_to(&mut cursor, image::ImageFormat::Jpeg).is_ok() {
                    return Ok(Some(jpeg_bytes));
                }
            }
        }

        Ok(None)
    }

    fn map_photo_row(row: &rusqlite::Row) -> Result<Photo> {
        let fav_int: i32 = row.get(12)?;
        let edit_rotation: Option<i32> = row.get(13).ok();
        let has_unsaved: i32 = row.get(14).unwrap_or(0);
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
            edit_rotation,
            has_unsaved_edits: has_unsaved == 1,
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

        // 5. Test face detection DB operations
        let p1_id = db.create_face_person("Alice", None).expect("create person 1");
        let p2_id = db.create_face_person("Bob", None).expect("create person 2");

        // Photo 1 has Alice and Bob
        db.save_detected_faces(
            fetched[0].id,
            &[
                FaceInsertData {
                    box_x: 0.2,
                    box_y: 0.2,
                    box_w: 0.2,
                    box_h: 0.2,
                    confidence: 0.95,
                    embedding: Some(vec![1.0, 0.0, 0.0]),
                    person_id: Some(p1_id),
                },
                FaceInsertData {
                    box_x: 0.6,
                    box_y: 0.2,
                    box_w: 0.2,
                    box_h: 0.2,
                    confidence: 0.91,
                    embedding: Some(vec![0.0, 1.0, 0.0]),
                    person_id: Some(p2_id),
                },
            ],
        ).expect("save faces 1");

        // Photo 2 has Alice only
        db.save_detected_faces(
            fetched[1].id,
            &[
                FaceInsertData {
                    box_x: 0.3,
                    box_y: 0.3,
                    box_w: 0.25,
                    box_h: 0.25,
                    confidence: 0.98,
                    embedding: Some(vec![0.98, 0.02, 0.0]),
                    person_id: Some(p1_id),
                },
            ],
        ).expect("save faces 2");

        // Collections should sort with most popular on top: Alice (2 photos) before Bob (1 photo)
        let collections = db.get_face_collections().expect("get collections");
        assert_eq!(collections.len(), 2);
        assert_eq!(collections[0].name, "Alice");
        assert_eq!(collections[0].photo_count, 2);
        assert_eq!(collections[1].name, "Bob");
        assert_eq!(collections[1].photo_count, 1);

        // Fetch faces for photo 1
        let p1_faces = db.get_faces_for_photo(fetched[0].id).expect("get faces");
        assert_eq!(p1_faces.len(), 2);

        // Photos for Alice
        let alice_photos = db.get_photos_for_person(p1_id).expect("alice photos");
        assert_eq!(alice_photos.len(), 2);

        // Rename Alice to "Alice Smith"
        db.update_person_name(p1_id, "Alice Smith").expect("rename");
        let updated_cols = db.get_face_collections().expect("get cols");
        assert_eq!(updated_cols[0].name, "Alice Smith");

        // 6. Test folder face removal and cluster cleanup
        let deleted_faces = db.remove_folder_faces(folder_id).expect("remove folder faces");
        assert_eq!(deleted_faces, 3);

        // Faces should be deleted from storage
        let faces_after = db.get_faces_for_photo(fetched[0].id).expect("get faces after");
        assert_eq!(faces_after.len(), 0);

        // Face persons should be cleaned up (both Alice and Bob now have 0 photos, so both deleted)
        let collections_after = db.get_face_collections().expect("get cols after");
        assert_eq!(collections_after.len(), 0);

        // Photos should have face_scanned reset to 0
        let (total_faces, total_people, pending) = db.get_face_counts().expect("face counts");
        assert_eq!(total_faces, 0);
        assert_eq!(total_people, 0);
        assert_eq!(pending, 0); // Folder is currently face_detect_enabled = 0

        // 7. Re-enable face detection for the folder
        db.enable_folder_face_detect(folder_id).expect("enable folder face detect");
        let (_, _, pending_re) = db.get_face_counts().expect("face counts re-enabled");
        assert_eq!(pending_re, 2); // Now both photos are pending scan again!
    }
}
