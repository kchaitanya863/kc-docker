use crate::storage::boxr_home;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeRecord {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: DateTime<Utc>,
    pub labels: HashMap<String, String>,
    pub scope: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct VolumeStoreData {
    volumes: Vec<VolumeRecord>,
}

pub struct VolumeStore {
    index_file: PathBuf,
    volumes_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountSpec {
    pub source: PathBuf,
    pub destination: String,
    pub read_only: bool,
    pub is_volume: bool,
}

impl VolumeStore {
    pub fn new() -> Self {
        let home = boxr_home();
        let volumes_dir = home.join("volumes");
        let _ = fs::create_dir_all(&volumes_dir);
        Self {
            index_file: home.join("volumes.json"),
            volumes_dir,
        }
    }

    fn load_unlocked(&self) -> VolumeStoreData {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            VolumeStoreData::default()
        }
    }

    fn save_unlocked(&self, data: &VolumeStoreData) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        let rand_suffix = hex::encode(crate::storage::container_store::rand_id());
        let temp_file = self
            .index_file
            .with_extension(format!("tmp.{}", rand_suffix));
        fs::write(&temp_file, content)?;
        fs::rename(&temp_file, &self.index_file)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<VolumeRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            Ok(self.load_unlocked().volumes)
        })
        .unwrap_or_default()
    }

    pub fn find(&self, name: &str) -> Option<VolumeRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            Ok(self
                .load_unlocked()
                .volumes
                .into_iter()
                .find(|v| v.name == name))
        })
        .ok()
        .flatten()
    }

    pub fn create(
        &self,
        name: Option<&str>,
        labels: Option<HashMap<String, String>>,
    ) -> Result<VolumeRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
        let mut data = self.load_unlocked();
        let vol_name = match name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => format!(
                "vol-{}",
                &hex::encode(crate::storage::container_store::rand_id())[..8]
            ),
        };

        if data.volumes.iter().any(|v| v.name == vol_name) {
            return Err(anyhow!("Volume with name '{}' already exists", vol_name));
        }

        let mountpoint = self.volumes_dir.join(&vol_name).join("_data");
        fs::create_dir_all(&mountpoint)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&mountpoint, fs::Permissions::from_mode(0o777));
        }

        let record = VolumeRecord {
            name: vol_name,
            driver: "local".to_string(),
            mountpoint: mountpoint.to_string_lossy().to_string(),
            created_at: Utc::now(),
            labels: labels.unwrap_or_default(),
            scope: "local".to_string(),
        };

        data.volumes.push(record.clone());
        self.save_unlocked(&data)?;
        Ok(record)
        })
    }

    pub fn remove(&self, name: &str) -> Result<VolumeRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
        let mut data = self.load_unlocked();
        if let Some(pos) = data.volumes.iter().position(|v| v.name == name) {
            let removed = data.volumes.remove(pos);
            self.save_unlocked(&data)?;

            let vol_dir = self.volumes_dir.join(&removed.name);
            if vol_dir.exists() {
                let _ = fs::remove_dir_all(vol_dir);
            }
            Ok(removed)
        } else {
            Err(anyhow!("Volume '{}' not found", name))
        }
        })
    }

    pub fn prune(&self) -> Result<Vec<String>> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
        let mut data = self.load_unlocked();
        let pruned: Vec<String> = data.volumes.iter().map(|v| v.name.clone()).collect();
        for name in &pruned {
            let vol_dir = self.volumes_dir.join(name);
            if vol_dir.exists() {
                let _ = fs::remove_dir_all(vol_dir);
            }
        }
        data.volumes.clear();
        self.save_unlocked(&data)?;
        Ok(pruned)
        })
    }

    /// Parse a volume flag string:
    /// e.g. "my-vol:/data:ro", "/host/path:/container/path", "my-vol:/data"
    pub fn resolve_mount(&self, spec_str: &str) -> Result<MountSpec> {
        let trimmed = spec_str.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("Volume specification cannot be empty"));
        }

        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err(anyhow!(
                "Invalid volume format '{}', expected [source:]destination[:mode]",
                spec_str
            ));
        }

        let (source_str, dest_str, read_only) = match parts.len() {
            1 => {
                // Anonymous volume
                ("", parts[0], false)
            }
            2 => (parts[0], parts[1], false),
            3 => {
                let ro = parts[2] == "ro";
                (parts[0], parts[1], ro)
            }
            _ => unreachable!(),
        };

        if dest_str.trim().is_empty() {
            return Err(anyhow!("Volume destination path cannot be empty"));
        }

        if source_str.is_empty() {
            // Create anonymous volume
            let vol = self.create(None, None)?;
            return Ok(MountSpec {
                source: PathBuf::from(vol.mountpoint),
                destination: dest_str.to_string(),
                read_only,
                is_volume: true,
            });
        }

        let is_host_path = source_str.starts_with('/')
            || source_str.starts_with('.')
            || source_str.starts_with('~');

        if is_host_path {
            if source_str.contains("..") {
                return Err(anyhow!(
                    "Path traversal rejected in volume mount: '{}'",
                    source_str
                ));
            }

            let host_path = if source_str.starts_with('~') {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(source_str.replacen('~', &home, 1))
            } else {
                PathBuf::from(source_str)
            };

            let canonical_source = if host_path.exists() {
                host_path.canonicalize()?
            } else {
                fs::create_dir_all(&host_path)?;
                host_path.canonicalize()?
            };

            Ok(MountSpec {
                source: canonical_source,
                destination: dest_str.to_string(),
                read_only,
                is_volume: false,
            })
        } else {
            // Named volume
            let record = match self.find(source_str) {
                Some(v) => v,
                None => self.create(Some(source_str), None)?,
            };
            Ok(MountSpec {
                source: PathBuf::from(record.mountpoint),
                destination: dest_str.to_string(),
                read_only,
                is_volume: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_volume_lifecycle() {
        let temp = tempdir().unwrap();
        let store = VolumeStore {
            index_file: temp.path().join("volumes.json"),
            volumes_dir: temp.path().join("volumes"),
        };

        // Create named volume
        let vol = store.create(Some("test-data"), None).unwrap();
        assert_eq!(vol.name, "test-data");
        assert!(PathBuf::from(&vol.mountpoint).exists());

        // Find
        let found = store.find("test-data").unwrap();
        assert_eq!(found.name, "test-data");

        // List
        assert_eq!(store.list().len(), 1);

        // Resolve mount for named volume
        let mount = store.resolve_mount("test-data:/app/data:ro").unwrap();
        assert_eq!(mount.destination, "/app/data");
        assert!(mount.read_only);
        assert!(mount.is_volume);

        // Remove
        store.remove("test-data").unwrap();
        assert!(store.find("test-data").is_none());
        assert_eq!(store.list().len(), 0);

        // Relative path traversal rejection
        let err = store.resolve_mount("../../../etc:/data");
        assert!(err.is_err());
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("Path traversal rejected")
        );

        // Absolute path traversal rejection
        let err = store.resolve_mount("/foo/../../../etc:/data");
        assert!(err.is_err());
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("Path traversal rejected")
        );
    }
}
