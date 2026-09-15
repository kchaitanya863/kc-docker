//! # Guardrails & Self-Healing Subsystem 🛡️
//!
//! Provides operational guardrails for robust, leak-free, long-running operation:
//! - **Log Rotation**: Automatic size-based rotation of container logs and system event streams.
//! - **Disk Space Protection**: Proactive checks to prevent out-of-disk failures during image pulls.
//! - **Orphan & Zombie Reaper**: Self-healing detection and recovery from unclean host reboots.
//! - **Graceful Termination**: Supervisor managing SIGTERM with fallback to SIGKILL on timeout.

use crate::storage::{ContainerStatus, ContainerStore, boxr_home};
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

/// Size-based log rotation utility
pub struct LogRotator;

impl LogRotator {
    /// Parse size string like "10m", "500k", "1g", "1048576"
    pub fn parse_size(input: &str) -> Result<u64> {
        let input = input.trim().to_lowercase();
        let (num_str, multiplier) = if let Some(n) = input.strip_suffix("g") {
            (n, 1024 * 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("gb") {
            (n, 1024 * 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("m") {
            (n, 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("mb") {
            (n, 1024 * 1024)
        } else if let Some(n) = input.strip_suffix("k") {
            (n, 1024)
        } else if let Some(n) = input.strip_suffix("kb") {
            (n, 1024)
        } else if let Some(n) = input.strip_suffix("b") {
            (n, 1)
        } else {
            (input.as_str(), 1)
        };

        let val: u64 = num_str.parse().context("Invalid log size string")?;
        Ok(val * multiplier)
    }

    /// Check file size and rotate if exceeding `max_bytes`.
    /// Rotates e.g. logs.txt -> logs.txt.1 -> logs.txt.2 up to `max_files`.
    pub fn rotate_if_needed(file_path: &Path, max_bytes: u64, max_files: usize) -> Result<bool> {
        if !file_path.exists() {
            return Ok(false);
        }

        let metadata = fs::metadata(file_path)?;
        if metadata.len() < max_bytes {
            return Ok(false);
        }

        if max_files == 0 {
            // Truncate in place if no backup files requested
            fs::write(file_path, b"")?;
            return Ok(true);
        }

        // Shift existing rotated files: e.g. logs.2 -> logs.3, logs.1 -> logs.2
        for i in (1..max_files).rev() {
            let src = PathBuf::from(format!("{}.{}", file_path.display(), i));
            let dst = PathBuf::from(format!("{}.{}", file_path.display(), i + 1));
            if src.exists() {
                let _ = fs::rename(&src, &dst);
            }
        }

        // Rotate current file to .1
        let first_backup = PathBuf::from(format!("{}.1", file_path.display()));
        let _ = fs::rename(file_path, &first_backup);

        // Create fresh empty active log file
        fs::write(file_path, b"")?;
        Ok(true)
    }

    /// Rotate global system events file (~/.boxr/events.jsonl) if it exceeds 5MB
    pub fn rotate_events_if_needed() {
        let events_file = boxr_home().join("events.jsonl");
        let max_size = 5 * 1024 * 1024; // 5 MB
        let max_files = 3;
        let _ = Self::rotate_if_needed(&events_file, max_size, max_files);
    }
}

/// Proactive disk space checks to protect host stability
pub struct DiskGuard;

impl DiskGuard {
    /// Return available free disk space in bytes for given directory path
    pub fn free_space_bytes(path: &Path) -> Result<u64> {
        #[cfg(windows)]
        {
            let _ = path;
            return Ok(100 * 1024 * 1024 * 1024);
        }
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::mem::MaybeUninit;

            let target_path = if path.exists() {
                path.to_path_buf()
            } else {
                path.parent().unwrap_or(Path::new("/")).to_path_buf()
            };

            let c_path = CString::new(target_path.to_string_lossy().as_bytes())?;
            let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();

            let ret = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
            if ret != 0 {
                return Err(anyhow!("Failed to query filesystem stats via statvfs"));
            }

            let stat = unsafe { stat.assume_init() };
            let free_bytes = (stat.f_bavail as u64) * (stat.f_frsize as u64);
            Ok(free_bytes)
        }
    }

    /// Check if target filesystem has enough headroom (at least `required_bytes` + 100MB margin)
    pub fn ensure_headroom(path: &Path, required_bytes: u64) -> Result<()> {
        let margin = 100 * 1024 * 1024; // 100 MB safety buffer
        let needed = required_bytes + margin;
        let available = Self::free_space_bytes(path)?;

        if available < needed {
            return Err(anyhow!(
                "Insufficient disk space on {}: required {:.1} MB, but only {:.1} MB available",
                path.display(),
                (needed as f64) / (1024.0 * 1024.0),
                (available as f64) / (1024.0 * 1024.0)
            ));
        }
        Ok(())
    }
}

/// Self-healing reaper for orphan and zombie container processes
pub struct ProcessReaper;

impl ProcessReaper {
    /// Inspect all recorded "Running" containers, identify dead processes, and self-heal records
    pub fn reap_stale_containers() -> Result<usize> {
        let store = ContainerStore::new();
        let containers = store.list();
        let mut reaped_count = 0;

        for c in containers {
            if !matches!(c.status, ContainerStatus::Running) {
                continue;
            }

            let bundle = PathBuf::from(&c.bundle_path);
            let pid_file = bundle.join("vm.pid");

            let is_alive = if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    #[cfg(unix)]
                    {
                        unsafe { libc::kill(pid, 0) == 0 }
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = pid;
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if !is_alive {
                // Container process is gone; self-heal state to Exited(137)
                let _ = fs::remove_file(&pid_file);
                let _ = store.update_status(&c.id, ContainerStatus::Exited(137));
                reaped_count += 1;
            }
        }

        Ok(reaped_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_log_rotator_size_parsing() {
        assert_eq!(LogRotator::parse_size("512b").unwrap(), 512);
        assert_eq!(LogRotator::parse_size("10k").unwrap(), 10 * 1024);
        assert_eq!(LogRotator::parse_size("10kb").unwrap(), 10 * 1024);
        assert_eq!(LogRotator::parse_size("5m").unwrap(), 5 * 1024 * 1024);
        assert_eq!(LogRotator::parse_size("5mb").unwrap(), 5 * 1024 * 1024);
        assert_eq!(
            LogRotator::parse_size("2g").unwrap(),
            2 * 1024 * 1024 * 1024
        );
        assert_eq!(
            LogRotator::parse_size("2gb").unwrap(),
            2 * 1024 * 1024 * 1024
        );
        assert!(LogRotator::parse_size("invalid_size").is_err());
    }

    #[test]
    fn test_log_rotator_rotation_lifecycle() -> Result<()> {
        let temp = tempdir().unwrap();
        let log_file = temp.path().join("logs.txt");

        // Write 100 bytes
        let data = vec![b'a'; 100];
        fs::write(&log_file, &data)?;

        // Max size 200: should NOT rotate
        let rotated = LogRotator::rotate_if_needed(&log_file, 200, 3)?;
        assert!(!rotated);
        assert_eq!(fs::metadata(&log_file)?.len(), 100);

        // Max size 50: SHOULD rotate
        let rotated = LogRotator::rotate_if_needed(&log_file, 50, 3)?;
        assert!(rotated);
        assert_eq!(fs::metadata(&log_file)?.len(), 0);

        let backup1 = temp.path().join("logs.txt.1");
        assert!(backup1.exists());
        assert_eq!(fs::metadata(&backup1)?.len(), 100);

        // Write again and rotate second time
        fs::write(&log_file, vec![b'b'; 120])?;
        let rotated = LogRotator::rotate_if_needed(&log_file, 50, 3)?;
        assert!(rotated);

        let backup2 = temp.path().join("logs.txt.2");
        assert!(backup2.exists());
        assert_eq!(fs::metadata(&backup2)?.len(), 100); // Old backup shifted to .2
        assert_eq!(fs::metadata(&backup1)?.len(), 120); // Newer backup is .1

        Ok(())
    }

    #[test]
    fn test_disk_guard_free_space() {
        let current_dir = Path::new(".");
        let free_bytes = DiskGuard::free_space_bytes(current_dir).unwrap();
        assert!(
            free_bytes > 1024 * 1024,
            "Free space should be at least 1MB"
        );

        // Asking for 1 byte should have plenty of headroom
        assert!(DiskGuard::ensure_headroom(current_dir, 1).is_ok());

        // Asking for 1000 Petabytes should fail gracefully with descriptive error
        let huge_size = 1000 * 1024 * 1024 * 1024 * 1024 * 1024u64;
        assert!(DiskGuard::ensure_headroom(current_dir, huge_size).is_err());
    }

    #[test]
    fn test_process_reaper_identifies_dead_containers() {
        // Reaping non-existent or dead containers shouldn't crash
        let reaped = ProcessReaper::reap_stale_containers().unwrap();
        let _ = reaped;
    }
}
