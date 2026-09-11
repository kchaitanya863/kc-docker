use crate::oci::image::ImageConfig;
use crate::storage::boxr_home;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRecord {
    pub id: String, // Short 12-char ID or full sha256
    pub reference: String,
    pub tag: String,
    pub manifest_digest: String,
    pub config_digest: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
    pub rootfs_path: String,
    pub config: ImageConfig,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ImageStoreData {
    images: Vec<ImageRecord>,
}

pub struct ImageStore {
    index_file: PathBuf,
}

impl ImageStore {
    pub fn new() -> Self {
        let home = boxr_home();
        Self {
            index_file: home.join("images.json"),
        }
    }

    fn load(&self) -> ImageStoreData {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            ImageStoreData::default()
        }
    }

    fn save(&self, data: &ImageStoreData) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        fs::write(&self.index_file, content)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<ImageRecord> {
        self.load().images
    }

    pub fn find(&self, query: &str) -> Option<ImageRecord> {
        let data = self.load();
        let query_trimmed = query.trim();

        // Normalize query: e.g. "hello-world" -> short name "hello-world", tag "latest"
        let (q_name, q_tag) = if let Some((n, t)) = query_trimmed.split_once(':') {
            (n, Some(t))
        } else {
            (query_trimmed, None)
        };

        data.images.into_iter().find(|img| {
            if img.id.starts_with(query_trimmed) {
                return true;
            }

            let img_short = img.reference.strip_prefix("library/").unwrap_or(&img.reference);
            let name_matches = img.reference == q_name || img_short == q_name;

            if let Some(tag) = q_tag {
                name_matches && img.tag == tag
            } else {
                name_matches && (img.tag == "latest" || img.tag == query_trimmed)
            }
        })
    }

    pub fn add(&self, record: ImageRecord) -> Result<()> {
        let mut data = self.load();
        // Remove previous entry with same reference/tag if present
        data.images.retain(|img| !(img.reference == record.reference && img.tag == record.tag));
        data.images.push(record);
        self.save(&data)
    }

    pub fn remove(&self, query: &str) -> Result<ImageRecord> {
        let mut data = self.load();
        let query_trimmed = query.trim();
        let (q_name, q_tag) = if let Some((n, t)) = query_trimmed.split_once(':') {
            (n, Some(t))
        } else {
            (query_trimmed, None)
        };

        let pos = data.images.iter().position(|img| {
            if img.id.starts_with(query_trimmed) {
                return true;
            }

            let img_short = img.reference.strip_prefix("library/").unwrap_or(&img.reference);
            let name_matches = img.reference == q_name || img_short == q_name;

            if let Some(tag) = q_tag {
                name_matches && img.tag == tag
            } else {
                name_matches && (img.tag == "latest" || img.tag == query_trimmed)
            }
        });

        if let Some(index) = pos {
            let removed = data.images.remove(index);
            self.save(&data)?;

            // Clean up rootfs directory
            let rootfs = PathBuf::from(&removed.rootfs_path);
            if rootfs.exists() {
                let _ = fs::remove_dir_all(rootfs);
            }

            Ok(removed)
        } else {
            Err(anyhow!("Image not found: {}", query))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oci::image::{ExecutionConfig, ImageConfig};
    use tempfile::tempdir;

    #[test]
    fn test_image_store_lifecycle() {
        let temp = tempdir().unwrap();
        let store = ImageStore {
            index_file: temp.path().join("images.json"),
        };

        let rec = ImageRecord {
            id: "1234567890ab".to_string(),
            reference: "library/test".to_string(),
            tag: "latest".to_string(),
            manifest_digest: "sha256:1234".to_string(),
            config_digest: "sha256:5678".to_string(),
            size_bytes: 1024,
            created_at: Utc::now(),
            rootfs_path: temp.path().join("rootfs").to_string_lossy().to_string(),
            config: ImageConfig {
                architecture: "arm64".to_string(),
                os: "linux".to_string(),
                config: Some(ExecutionConfig::default()),
                rootfs: None,
            },
        };

        store.add(rec).unwrap();
        assert_eq!(store.list().len(), 1);

        assert!(store.find("test").is_some());
        assert!(store.find("library/test:latest").is_some());
        assert!(store.find("123456").is_some());

        store.remove("test").unwrap();
        assert_eq!(store.list().len(), 0);
    }
}
