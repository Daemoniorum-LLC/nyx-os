//! Journal storage utilities

use anyhow::Result;
use std::path::Path;

/// Calculate disk usage of journal directory
pub fn disk_usage(dir: &Path) -> Result<DiskUsage> {
    let mut total_size = 0u64;
    let mut file_count = 0u64;
    let mut compressed_size = 0u64;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_file() {
            total_size += metadata.len();
            file_count += 1;

            if entry.path().extension().and_then(|e| e.to_str()) == Some("gz") {
                compressed_size += metadata.len();
            }
        }
    }

    Ok(DiskUsage {
        total_size,
        file_count,
        compressed_size,
        current_size: total_size - compressed_size,
    })
}

#[derive(Debug, Clone)]
pub struct DiskUsage {
    pub total_size: u64,
    pub file_count: u64,
    pub compressed_size: u64,
    pub current_size: u64,
}

impl DiskUsage {
    pub fn format_size(bytes: u64) -> String {
        if bytes >= 1024 * 1024 * 1024 {
            format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        } else if bytes >= 1024 * 1024 {
            format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{:.2} KB", bytes as f64 / 1024.0)
        } else {
            format!("{} B", bytes)
        }
    }
}

/// Vacuum journal (remove all archives)
pub fn vacuum(dir: &Path) -> Result<u64> {
    let mut freed = 0u64;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("gz") {
            freed += entry.metadata()?.len();
            std::fs::remove_file(&path)?;
        }
    }

    Ok(freed)
}

/// Verify journal integrity
pub fn verify(dir: &Path) -> Result<VerifyResult> {
    let mut result = VerifyResult::default();

    // Check current journal
    let current = dir.join("current.journal");
    if current.exists() {
        match verify_journal_file(&current) {
            Ok(count) => result.valid_entries += count,
            Err(_) => result.corrupted_files += 1,
        }
    }

    // Check archives
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("gz") {
            match verify_archive(&path) {
                Ok(count) => {
                    result.valid_entries += count;
                    result.valid_archives += 1;
                }
                Err(_) => result.corrupted_files += 1,
            }
        }
    }

    Ok(result)
}

fn verify_journal_file(path: &Path) -> Result<u64> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        let line = line?;
        // Try to parse as JSON
        if serde_json::from_str::<serde_json::Value>(&line).is_ok() {
            count += 1;
        }
    }

    Ok(count)
}

fn verify_archive(path: &Path) -> Result<u64> {
    use std::io::{BufRead, BufReader};
    use flate2::read::GzDecoder;

    let file = std::fs::File::open(path)?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);
    let mut count = 0;

    for line in reader.lines() {
        let line = line?;
        if serde_json::from_str::<serde_json::Value>(&line).is_ok() {
            count += 1;
        }
    }

    Ok(count)
}

#[derive(Debug, Clone, Default)]
pub struct VerifyResult {
    pub valid_entries: u64,
    pub valid_archives: u64,
    pub corrupted_files: u64,
}
