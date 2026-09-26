//! OCI Artifact store management (Podman `artifact` parity).

use crate::storage::boxr_home;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub name: String,
    pub digest: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub file_path: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ArtifactIndex {
    artifacts: Vec<ArtifactRecord>,
}

pub struct ArtifactStore {
    index_file: PathBuf,
    artifacts_dir: PathBuf,
}

impl ArtifactStore {
    pub fn new() -> Self {
        Self::with_home(boxr_home())
    }

    pub fn with_home(home: PathBuf) -> Self {
        let artifacts_dir = home.join("artifacts");
        let _ = fs::create_dir_all(&artifacts_dir);
        Self {
            index_file: home.join("artifacts.json"),
            artifacts_dir,
        }
    }

    fn load(&self) -> ArtifactIndex {
        if let Ok(content) = fs::read_to_string(&self.index_file) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            ArtifactIndex::default()
        }
    }

    fn save(&self, data: &ArtifactIndex) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        let rand_suffix = hex::encode(crate::storage::container_store::rand_id());
        let temp_file = self
            .index_file
            .with_extension(format!("tmp.{}", rand_suffix));
        fs::write(&temp_file, content)?;
        fs::rename(&temp_file, &self.index_file)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<ArtifactRecord> {
        self.load().artifacts
    }

    pub fn find(&self, query: &str) -> Option<ArtifactRecord> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        self.load()
            .artifacts
            .into_iter()
            .find(|a| a.name == q || a.digest == q || a.digest.starts_with(q))
    }

    pub fn add(&self, name: &str, file: &Path, media_type: &str) -> Result<ArtifactRecord> {
        if !file.exists() {
            return Err(anyhow!(
                "Artifact source file '{}' not found",
                file.display()
            ));
        }

        let content = fs::read(file)?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let digest = format!("sha256:{}", hex::encode(hasher.finalize()));

        let target_file = self.artifacts_dir.join(&digest.replace(':', "_"));
        fs::write(&target_file, &content)?;

        let mut data = self.load();
        data.artifacts.retain(|a| a.name != name);

        let record = ArtifactRecord {
            name: name.to_string(),
            digest,
            media_type: media_type.to_string(),
            size_bytes: content.len() as u64,
            created_at: Utc::now(),
            file_path: target_file.to_string_lossy().to_string(),
        };

        data.artifacts.push(record.clone());
        self.save(&data)?;
        Ok(record)
    }

    pub fn extract(&self, query: &str, dest: &Path) -> Result<PathBuf> {
        let artifact = self
            .find(query)
            .ok_or_else(|| anyhow!("Artifact '{}' not found", query))?;

        let src = PathBuf::from(&artifact.file_path);
        if !src.exists() {
            return Err(anyhow!("Artifact file data missing on disk"));
        }

        fs::create_dir_all(dest)?;
        let target = dest.join(&artifact.name);
        fs::copy(&src, &target)?;
        Ok(target)
    }

    pub fn remove(&self, query: &str) -> Result<ArtifactRecord> {
        let mut data = self.load();
        let pos = data
            .artifacts
            .iter()
            .position(|a| a.name == query || a.digest == query || a.digest.starts_with(query))
            .ok_or_else(|| anyhow!("Artifact '{}' not found", query))?;

        let removed = data.artifacts.remove(pos);
        let _ = fs::remove_file(&removed.file_path);
        self.save(&data)?;
        Ok(removed)
    }
}
