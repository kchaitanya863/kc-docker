use crate::oci::image::ImageConfig;
use crate::storage::boxr_home;
use anyhow::{Result, anyhow};
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
        self.find_with_platform(query, None)
    }

    pub fn find_with_platform(&self, query: &str, platform: Option<&str>) -> Option<ImageRecord> {
        let data = self.load();
        let query_trimmed = query.trim();

        // Normalize query: e.g. "hello-world" -> short name "hello-world", tag "latest"
        let (q_name, q_tag) = if let Some((n, t)) = query_trimmed.split_once(':') {
            (n, Some(t))
        } else {
            (query_trimmed, None)
        };

        data.images.into_iter().find(|img| {
            let id_matches = img.id.starts_with(query_trimmed);
            let img_short = img
                .reference
                .strip_prefix("library/")
                .unwrap_or(&img.reference);
            let name_matches = img.reference == q_name || img_short == q_name;

            let matches = if id_matches {
                true
            } else if let Some(tag) = q_tag {
                name_matches && img.tag == tag
            } else {
                name_matches && (img.tag == "latest" || img.tag == query_trimmed)
            };

            if !matches {
                return false;
            }

            if let Some(target_plat) = platform {
                let (target_os, target_arch) = if let Some((os, arch)) = target_plat.split_once('/')
                {
                    (Some(os), arch)
                } else {
                    (None, target_plat)
                };
                let norm_arch = match target_arch {
                    "x86_64" => "amd64",
                    "aarch64" => "arm64",
                    other => other,
                };
                let img_arch = match img.config.architecture.as_str() {
                    "x86_64" => "amd64",
                    "aarch64" => "arm64",
                    other => other,
                };

                let arch_matches = img_arch == norm_arch;
                let os_matches = if let Some(tos) = target_os {
                    img.config.os.eq_ignore_ascii_case(tos)
                } else {
                    true
                };

                arch_matches && os_matches
            } else {
                true
            }
        })
    }

    pub fn add(&self, record: ImageRecord) -> Result<()> {
        let mut data = self.load();
        // Remove previous entry with same reference, tag, architecture, and os if present
        data.images.retain(|img| {
            !(img.reference == record.reference
                && img.tag == record.tag
                && img.config.architecture == record.config.architecture
                && img.config.os == record.config.os)
        });
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

            let img_short = img
                .reference
                .strip_prefix("library/")
                .unwrap_or(&img.reference);
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

            // Only clean up rootfs directory if NO OTHER image shares this rootfs_path
            let is_shared = data.images.iter().any(|img| {
                img.rootfs_path == removed.rootfs_path
                    || img.manifest_digest == removed.manifest_digest
            });
            if !is_shared {
                let rootfs = PathBuf::from(&removed.rootfs_path);
                if rootfs.exists() {
                    let _ = fs::remove_dir_all(rootfs);
                }
            }

            Ok(removed)
        } else {
            Err(anyhow!("Image not found: {}", query))
        }
    }

    /// Commit a container's current filesystem snapshot into a new image
    pub fn commit_container(
        &self,
        container: &crate::storage::ContainerRecord,
        repo_tag: Option<&str>,
        _message: Option<&str>,
        _author: Option<&str>,
    ) -> Result<ImageRecord> {
        let home = boxr_home();
        let random_id = hex::encode(crate::storage::container_store::rand_id());
        let image_id = format!("sha256:{}", random_id);
        let safe_id = image_id.replace(':', "_");

        let dest_image_dir = home.join("images").join(&safe_id);
        let dest_rootfs = dest_image_dir.join("rootfs");
        fs::create_dir_all(&dest_rootfs)?;

        let container_rootfs = PathBuf::from(&container.bundle_path).join("rootfs");
        crate::storage::overlay::OverlayDriver::create_hardlink_tree(
            &container_rootfs,
            &dest_rootfs,
        )?;

        let full_tag = repo_tag.unwrap_or_else(|| &container.name);
        let (repo, tag) = if let Some((r, t)) = full_tag.split_once(':') {
            (r.to_string(), t.to_string())
        } else {
            (full_tag.to_string(), "latest".to_string())
        };

        let base_config = self
            .find(&container.image)
            .map(|i| i.config)
            .unwrap_or_else(|| crate::oci::image::ImageConfig {
                architecture: std::env::consts::ARCH.to_string(),
                os: "linux".to_string(),
                config: Some(crate::oci::image::ExecutionConfig {
                    cmd: Some(container.command.clone()),
                    ..Default::default()
                }),
                rootfs: None,
            });

        let record = ImageRecord {
            id: random_id[..12].to_string(),
            reference: repo,
            tag,
            manifest_digest: image_id.clone(),
            config_digest: image_id.clone(),
            size_bytes: 1024 * 1024,
            created_at: Utc::now(),
            rootfs_path: dest_rootfs.to_string_lossy().to_string(),
            config: base_config,
        };

        self.add(record.clone())?;
        Ok(record)
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
