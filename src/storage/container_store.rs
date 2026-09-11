use crate::storage::boxr_home;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

pub fn rand_id() -> [u8; 6] {
    let mut bytes = [0u8; 6];
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let combined = now ^ ((pid as u128) << 32);
    let mut hasher = Sha256::new();
    hasher.update(combined.to_le_bytes());
    let hash = hasher.finalize();
    bytes.copy_from_slice(&hash[..6]);
    bytes
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerStatus {
    Created,
    Running,
    Exited(i32),
    Failed(String),
}

impl std::fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerStatus::Created => write!(f, "Created"),
            ContainerStatus::Running => write!(f, "Up"),
            ContainerStatus::Exited(code) => write!(f, "Exited ({})", code),
            ContainerStatus::Failed(err) => write!(f, "Failed: {}", err),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerRecord {
    pub id: String,
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub status: ContainerStatus,
    pub bundle_path: String,
    #[serde(default)]
    pub restart_policy: crate::health::RestartPolicy,
    #[serde(default = "default_health_status")]
    pub health_status: crate::health::HealthStatus,
    #[serde(default)]
    pub restart_count: u32,
}

fn default_health_status() -> crate::health::HealthStatus {
    crate::health::HealthStatus::None
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ContainerStoreData {
    containers: Vec<ContainerRecord>,
}

pub struct ContainerStore {
    index_file: PathBuf,
}

impl ContainerStore {
    pub fn new() -> Self {
        let home = boxr_home();
        Self {
            index_file: home.join("containers.json"),
        }
    }

    fn load(&self) -> ContainerStoreData {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            ContainerStoreData::default()
        }
    }

    fn save(&self, data: &ContainerStoreData) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        fs::write(&self.index_file, content)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<ContainerRecord> {
        self.load().containers
    }

    #[allow(dead_code)]
    pub fn find(&self, query: &str) -> Option<ContainerRecord> {
        let data = self.load();
        data.containers.into_iter().find(|c| {
            c.id.starts_with(query) || c.name == query
        })
    }

    pub fn add(&self, record: ContainerRecord) -> Result<()> {
        let mut data = self.load();
        data.containers.retain(|c| c.id != record.id);
        data.containers.push(record);
        self.save(&data)
    }

    pub fn update_status(&self, id_or_name: &str, status: ContainerStatus) -> Result<()> {
        let mut data = self.load();
        if let Some(c) = data.containers.iter_mut().find(|c| c.id == id_or_name || c.id.starts_with(id_or_name) || c.name == id_or_name) {
            c.status = status;
            self.save(&data)?;
            Ok(())
        } else {
            Err(anyhow!("Container not found: {}", id_or_name))
        }
    }

    pub fn remove(&self, query: &str) -> Result<ContainerRecord> {
        let mut data = self.load();
        let pos = data.containers.iter().position(|c| {
            c.id.starts_with(query) || c.name == query
        });

        if let Some(index) = pos {
            let removed = data.containers.remove(index);
            self.save(&data)?;

            // Clean up bundle folder
            let bundle = PathBuf::from(&removed.bundle_path);
            if bundle.exists() {
                let _ = fs::remove_dir_all(bundle);
            }

            Ok(removed)
        } else {
            Err(anyhow!("Container not found: {}", query))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_container_store_lifecycle() {
        let temp = tempdir().unwrap();
        let store = ContainerStore {
            index_file: temp.path().join("containers.json"),
        };

        let rec = ContainerRecord {
            id: "aabbccddeeff".to_string(),
            name: "test-box".to_string(),
            image: "alpine:latest".to_string(),
            command: vec!["/bin/sh".to_string()],
            created_at: Utc::now(),
            status: ContainerStatus::Running,
            bundle_path: temp.path().join("bundle").to_string_lossy().to_string(),
            restart_policy: crate::health::RestartPolicy::No,
            health_status: crate::health::HealthStatus::None,
            restart_count: 0,
        };

        store.add(rec).unwrap();
        assert_eq!(store.list().len(), 1);

        assert!(store.find("aabbcc").is_some());
        assert!(store.find("test-box").is_some());

        store.update_status("aabbcc", ContainerStatus::Exited(0)).unwrap();
        let updated = store.find("test-box").unwrap();
        assert_eq!(updated.status, ContainerStatus::Exited(0));

        store.remove("test-box").unwrap();
        assert_eq!(store.list().len(), 0);
    }
}
