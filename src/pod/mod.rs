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
        let home = boxr_home();
        Self {
            index_file: home.join("pods.json"),
        }
    }

    fn load(&self) -> PodStoreData {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PodStoreData::default()
        }
    }

    fn save(&self, data: &PodStoreData) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        fs::write(&self.index_file, content)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<PodRecord> {
        self.load().pods
    }

    pub fn find(&self, query: &str) -> Option<PodRecord> {
        let data = self.load();
        data.pods
            .into_iter()
            .find(|p| p.id.starts_with(query) || p.name == query)
    }

    pub fn create(&self, name: Option<&str>, ports: Vec<PortMapping>) -> Result<PodRecord> {
        let mut data = self.load();
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
        self.save(&data)?;
        Ok(pod)
    }

    pub fn remove(&self, query: &str) -> Result<PodRecord> {
        let mut data = self.load();
        let c_store = ContainerStore::new();

        if let Some(pos) = data
            .pods
            .iter()
            .position(|p| p.id.starts_with(query) || p.name == query)
        {
            let removed = data.pods.remove(pos);
            self.save(&data)?;

            // Remove all member containers
            for cid in &removed.containers {
                let _ = c_store.remove(cid);
            }
            Ok(removed)
        } else {
            Err(anyhow!("Pod '{}' not found", query))
        }
    }

    pub fn add_container_to_pod(&self, pod_query: &str, container_id: &str) -> Result<()> {
        let mut data = self.load();
        if let Some(p) = data
            .pods
            .iter_mut()
            .find(|p| p.id.starts_with(pod_query) || p.name == pod_query)
        {
            if !p.containers.contains(&container_id.to_string()) {
                p.containers.push(container_id.to_string());
                p.status = "Running".to_string();
                self.save(&data)?;
            }
            Ok(())
        } else {
            Err(anyhow!("Pod '{}' not found", pod_query))
        }
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
