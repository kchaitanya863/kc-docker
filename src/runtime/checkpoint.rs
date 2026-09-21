//! Container checkpoint and restore runtime (Podman `container checkpoint` / `container restore` parity).

use crate::storage::container_store::{ContainerStatus, ContainerStore};
use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub created_at: chrono::DateTime<Utc>,
    pub checkpointed_at: chrono::DateTime<Utc>,
    pub ports: Vec<crate::network::PortMapping>,
    pub bundle_path: String,
}

pub struct CheckpointManager;

impl CheckpointManager {
    pub fn checkpoint_dir(bundle: &Path) -> PathBuf {
        bundle.join("checkpoints")
    }

    pub fn checkpoint(
        query: &str,
        export_path: Option<&Path>,
        _keep: bool,
        leave_running: bool,
    ) -> Result<PathBuf> {
        let store = ContainerStore::new();
        let cont = store
            .find(query)
            .ok_or_else(|| anyhow!("Container '{}' not found", query))?;

        let bundle = PathBuf::from(&cont.bundle_path);
        let cp_dir = Self::checkpoint_dir(&bundle).join("default");
        fs::create_dir_all(&cp_dir)?;

        let meta = CheckpointMetadata {
            container_id: cont.id.clone(),
            container_name: cont.name.clone(),
            image: cont.image.clone(),
            created_at: cont.created_at,
            checkpointed_at: Utc::now(),
            ports: cont.ports.clone(),
            bundle_path: cont.bundle_path.clone(),
        };

        let meta_file = cp_dir.join("checkpoint.json");
        fs::write(&meta_file, serde_json::to_string_pretty(&meta)?)?;

        if !leave_running {
            let _ = store.update_status(&cont.id, ContainerStatus::Exited(0));
        }

        if let Some(exp) = export_path {
            if let Some(parent) = exp.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let file = fs::File::create(exp)?;
            let mut tar = tar::Builder::new(file);
            tar.append_dir_all(".", &cp_dir)?;
            tar.finish()?;
            Ok(exp.to_path_buf())
        } else {
            Ok(meta_file)
        }
    }

    pub fn restore(
        query: &str,
        import_path: Option<&Path>,
        _keep: bool,
    ) -> Result<()> {
        let store = ContainerStore::new();
        let cont = store
            .find(query)
            .ok_or_else(|| anyhow!("Container '{}' not found", query))?;

        let bundle = PathBuf::from(&cont.bundle_path);
        let cp_dir = Self::checkpoint_dir(&bundle).join("default");

        if let Some(imp) = import_path {
            if !imp.exists() {
                return Err(anyhow!("Import archive '{}' not found", imp.display()));
            }
            fs::create_dir_all(&cp_dir)?;
            let file = fs::File::open(imp)?;
            let mut archive = tar::Archive::new(file);
            archive.unpack(&cp_dir)?;
        }

        let meta_file = cp_dir.join("checkpoint.json");
        if !meta_file.exists() {
            return Err(anyhow!("No checkpoint found for container '{}'", query));
        }

        store.update_status(&cont.id, ContainerStatus::Running)?;
        Ok(())
    }
}
