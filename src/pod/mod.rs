use crate::network::PortMapping;
use crate::storage::{ContainerStore, boxr_home};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodRecord {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub status: String,
    pub infra_container_id: String,
    pub containers: Vec<String>,
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PodStoreData {
    pods: Vec<PodRecord>,
}

pub struct PodStore {
    index_file: PathBuf,
}

impl PodStore {
    pub fn new() -> Self {
        Self::with_home(boxr_home())
    }

    pub fn with_home(home: PathBuf) -> Self {
        Self {
            index_file: home.join("pods.json"),
        }
    }

    fn load_unlocked(&self) -> PodStoreData {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PodStoreData::default()
        }
    }

    fn save_unlocked(&self, data: &PodStoreData) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        let rand_suffix = hex::encode(crate::storage::container_store::rand_id());
        let temp_file = self
            .index_file
            .with_extension(format!("tmp.{}", rand_suffix));
        fs::write(&temp_file, content)?;
        fs::rename(&temp_file, &self.index_file)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<PodRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            Ok(self.load_unlocked().pods)
        })
        .unwrap_or_default()
    }

    pub fn find(&self, query: &str) -> Option<PodRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            Ok(self
                .load_unlocked()
                .pods
                .into_iter()
                .find(|p| p.id.starts_with(query) || p.name == query))
        })
        .ok()
        .flatten()
    }

    pub fn create(&self, name: Option<&str>, ports: Vec<PortMapping>) -> Result<PodRecord> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            let mut data = self.load_unlocked();
            let random_id = hex::encode(crate::storage::container_store::rand_id());
            let pod_name = name
                .map(|n| n.trim().to_string())
                .unwrap_or_else(|| format!("pod-{}", &random_id[..6]));

            if data.pods.iter().any(|p| p.name == pod_name) {
                return Err(anyhow!("Pod name '{}' already exists", pod_name));
            }

            let pod = PodRecord {
                id: random_id[..12].to_string(),
                name: pod_name,
                created_at: Utc::now(),
                status: "Created".to_string(),
                infra_container_id: format!("infra-{}", &random_id[..6]),
                containers: Vec::new(),
                ports,
            };

            data.pods.push(pod.clone());
            self.save_unlocked(&data)?;
            Ok(pod)
        })
    }

    pub fn remove(&self, query: &str) -> Result<PodRecord> {
        self.remove_with_force(query, false)
    }

    pub fn remove_with_force(&self, query: &str, force: bool) -> Result<PodRecord> {
        let home = self
            .index_file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(boxr_home);
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            let mut data = self.load_unlocked();
            let c_store = ContainerStore::with_home(home.clone());

            if let Some(pos) = data
                .pods
                .iter()
                .position(|p| p.id.starts_with(query) || p.name == query)
            {
                if !force {
                    for cid in &data.pods[pos].containers {
                        if let Some(c) = c_store.find(cid) {
                            if matches!(c.status, crate::storage::ContainerStatus::Running) {
                                return Err(anyhow!(
                                    "conflict: cannot remove running pod {}. Stop the pod or use force",
                                    data.pods[pos].name
                                ));
                            }
                        }
                    }
                }

                let removed = data.pods.remove(pos);
                self.save_unlocked(&data)?;

                // Remove all member containers
                for cid in &removed.containers {
                    let _ = c_store.remove(cid);
                }
                Ok(removed)
            } else {
                Err(anyhow!("Pod '{}' not found", query))
            }
        })
    }

    pub fn update_status(&self, pod_query: &str, status: &str) -> Result<()> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            let mut data = self.load_unlocked();
            if let Some(p) = data
                .pods
                .iter_mut()
                .find(|p| p.id.starts_with(pod_query) || p.name == pod_query)
            {
                p.status = status.to_string();
                self.save_unlocked(&data)?;
                Ok(())
            } else {
                Err(anyhow!("Pod '{}' not found", pod_query))
            }
        })
    }

    pub fn add_container_to_pod(&self, pod_query: &str, container_id: &str) -> Result<()> {
        crate::storage::index_lock::with_index_lock(&self.index_file, || {
            let mut data = self.load_unlocked();
            if let Some(p) = data
                .pods
                .iter_mut()
                .find(|p| p.id.starts_with(pod_query) || p.name == pod_query)
            {
                if !p.containers.contains(&container_id.to_string()) {
                    p.containers.push(container_id.to_string());
                    p.status = "Running".to_string();
                    self.save_unlocked(&data)?;
                }
                Ok(())
            } else {
                Err(anyhow!("Pod '{}' not found", pod_query))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pod_store_lifecycle() {
        let temp = tempdir().unwrap();
        let store = PodStore {
            index_file: temp.path().join("pods.json"),
        };

        let pod = store.create(Some("test-pod"), Vec::new()).unwrap();
        assert_eq!(pod.name, "test-pod");
        assert_eq!(store.list().len(), 1);

        assert!(store.find("test-pod").is_some());
        assert!(store.find(&pod.id).is_some());

        store.add_container_to_pod("test-pod", "cont123").unwrap();
        let updated = store.find("test-pod").unwrap();
        assert_eq!(updated.containers, vec!["cont123".to_string()]);
        assert_eq!(updated.status, "Running");

        store.remove("test-pod").unwrap();
        assert_eq!(store.list().len(), 0);
    }
}
