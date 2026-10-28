use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceScanTimingRecord {
    pub photo_id: i64,
    pub file_name: String,
    pub file_hash: String,
    pub worker_id: usize,
    pub is_thumbnail: bool,
    pub width: u32,
    pub height: u32,
    pub decode_ms: f64,
    pub detect_ms: f64,
    pub faces_detected: usize,
    pub embed_ms: f64,
    pub cluster_ms: f64,
    pub db_save_ms: f64,
    pub total_ms: f64,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceTimingStats {
    pub is_recording: bool,
    pub record_count: usize,
    pub avg_decode_ms: f64,
    pub avg_detect_ms: f64,
    pub avg_embed_ms: f64,
    pub avg_total_ms: f64,
    pub min_total_ms: f64,
    pub max_total_ms: f64,
    pub photos_per_second: f64,
    pub log_file_path: String,
}

pub struct FaceTimingRecorder {
    is_recording: Arc<AtomicBool>,
    records: Arc<Mutex<VecDeque<FaceScanTimingRecord>>>,
    log_file_path: PathBuf,
    max_memory_records: usize,
}

impl FaceTimingRecorder {
    pub fn new(initial_enabled: bool) -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pica");
        let _ = std::fs::create_dir_all(&cache_dir);
        let log_file_path = cache_dir.join("face_scan_timings.csv");

        Self {
            is_recording: Arc::new(AtomicBool::new(initial_enabled)),
            records: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            log_file_path,
            max_memory_records: 1000,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::Relaxed)
    }

    pub fn set_recording(&self, enabled: bool) {
        self.is_recording.store(enabled, Ordering::SeqCst);
    }

    pub fn log_file_path(&self) -> &PathBuf {
        &self.log_file_path
    }

    pub fn record(&self, record: FaceScanTimingRecord) {
        if !self.is_recording.load(Ordering::Relaxed) {
            return;
        }

        // 1. Maintain in-memory ring buffer
        if let Ok(mut records) = self.records.lock() {
            if records.len() >= self.max_memory_records {
                records.pop_front();
            }
            records.push_back(record.clone());
        }

        // 2. Append to persistent CSV log file
        self.append_to_csv(&record);
    }

    fn append_to_csv(&self, rec: &FaceScanTimingRecord) {
        let file_exists = self.log_file_path.exists()
            && self
                .log_file_path
                .metadata()
                .map(|m| m.len() > 0)
                .unwrap_or(false);

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)
        {
            if !file_exists {
                let _ = writeln!(
                    file,
                    "timestamp_ms,photo_id,file_name,file_hash,worker_id,is_thumbnail,width,height,decode_ms,detect_ms,faces_detected,embed_ms,cluster_ms,db_save_ms,total_ms"
                );
            }
            let _ = writeln!(
                file,
                "{},{},\"{}\",{},{},{},{},{},{:.2},{:.2},{},{:.2},{:.2},{:.2},{:.2}",
                rec.timestamp_ms,
                rec.photo_id,
                rec.file_name.replace('"', "\"\""),
                rec.file_hash,
                rec.worker_id,
                rec.is_thumbnail,
                rec.width,
                rec.height,
                rec.decode_ms,
                rec.detect_ms,
                rec.faces_detected,
                rec.embed_ms,
                rec.cluster_ms,
                rec.db_save_ms,
                rec.total_ms,
            );
        }
    }

    pub fn get_stats(&self) -> FaceTimingStats {
        let is_recording = self.is_recording.load(Ordering::Relaxed);
        let records_guard = self.records.lock().unwrap();
        let count = records_guard.len();

        if count == 0 {
            return FaceTimingStats {
                is_recording,
                record_count: 0,
                avg_decode_ms: 0.0,
                avg_detect_ms: 0.0,
                avg_embed_ms: 0.0,
                avg_total_ms: 0.0,
                min_total_ms: 0.0,
                max_total_ms: 0.0,
                photos_per_second: 0.0,
                log_file_path: self.log_file_path.to_string_lossy().to_string(),
            };
        }

        let mut sum_decode = 0.0;
        let mut sum_detect = 0.0;
        let mut sum_embed = 0.0;
        let mut sum_total = 0.0;
        let mut min_total = f64::MAX;
        let mut max_total = 0.0;

        for r in records_guard.iter() {
            sum_decode += r.decode_ms;
            sum_detect += r.detect_ms;
            sum_embed += r.embed_ms;
            sum_total += r.total_ms;
            if r.total_ms < min_total {
                min_total = r.total_ms;
            }
            if r.total_ms > max_total {
                max_total = r.total_ms;
            }
        }

        let n = count as f64;
        let avg_total = sum_total / n;
        let pps = if avg_total > 0.0 {
            1000.0 / avg_total
        } else {
            0.0
        };

        FaceTimingStats {
            is_recording,
            record_count: count,
            avg_decode_ms: (sum_decode / n * 100.0).round() / 100.0,
            avg_detect_ms: (sum_detect / n * 100.0).round() / 100.0,
            avg_embed_ms: (sum_embed / n * 100.0).round() / 100.0,
            avg_total_ms: (avg_total * 100.0).round() / 100.0,
            min_total_ms: (min_total * 100.0).round() / 100.0,
            max_total_ms: (max_total * 100.0).round() / 100.0,
            photos_per_second: (pps * 10.0).round() / 10.0,
            log_file_path: self.log_file_path.to_string_lossy().to_string(),
        }
    }

    pub fn get_records(&self, limit: Option<usize>) -> Vec<FaceScanTimingRecord> {
        let guard = self.records.lock().unwrap();
        let limit = limit.unwrap_or(usize::MAX);
        guard.iter().rev().take(limit).cloned().collect()
    }

    pub fn clear(&self) {
        if let Ok(mut records) = self.records.lock() {
            records.clear();
        }
        let _ = std::fs::remove_file(&self.log_file_path);
    }

    pub fn export_csv(&self) -> String {
        if self.log_file_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&self.log_file_path) {
                if !content.trim().is_empty() {
                    return content;
                }
            }
        }

        // Fallback: build CSV from memory
        let mut out = String::from("timestamp_ms,photo_id,file_name,file_hash,worker_id,is_thumbnail,width,height,decode_ms,detect_ms,faces_detected,embed_ms,cluster_ms,db_save_ms,total_ms\n");
        if let Ok(records) = self.records.lock() {
            for rec in records.iter() {
                out.push_str(&format!(
                    "{},{},\"{}\",{},{},{},{},{},{:.2},{:.2},{},{:.2},{:.2},{:.2},{:.2}\n",
                    rec.timestamp_ms,
                    rec.photo_id,
                    rec.file_name.replace('"', "\"\""),
                    rec.file_hash,
                    rec.worker_id,
                    rec.is_thumbnail,
                    rec.width,
                    rec.height,
                    rec.decode_ms,
                    rec.detect_ms,
                    rec.faces_detected,
                    rec.embed_ms,
                    rec.cluster_ms,
                    rec.db_save_ms,
                    rec.total_ms,
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_recorder() {
        let recorder = FaceTimingRecorder::new(true);
        assert!(recorder.is_recording());

        recorder.record(FaceScanTimingRecord {
            photo_id: 1,
            file_name: "test.jpg".to_string(),
            file_hash: "hash1".to_string(),
            worker_id: 0,
            is_thumbnail: true,
            width: 256,
            height: 256,
            decode_ms: 1.5,
            detect_ms: 15.0,
            faces_detected: 1,
            embed_ms: 20.0,
            cluster_ms: 0.5,
            db_save_ms: 2.0,
            total_ms: 39.0,
            timestamp_ms: 1000,
        });

        let stats = recorder.get_stats();
        assert_eq!(stats.record_count, 1);
        assert_eq!(stats.avg_detect_ms, 15.0);
        assert_eq!(stats.avg_total_ms, 39.0);

        let records = recorder.get_records(Some(10));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].photo_id, 1);

        recorder.clear();
        let stats_after = recorder.get_stats();
        assert_eq!(stats_after.record_count, 0);
    }
}
