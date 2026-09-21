//! Extended volume operations (export, import, reload, rename, mount, unmount).

use crate::storage::boxr_home;
use crate::volume::{VolumeRecord, VolumeStore};
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

pub struct VolumeOps;

impl VolumeOps {
    pub fn export_volume(name: &str, output: Option<&Path>) -> Result<PathBuf> {
        let store = VolumeStore::new();
        let vol = store
            .find(name)
            .ok_or_else(|| anyhow!("Volume '{}' not found", name))?;

        let mountpoint = PathBuf::from(&vol.mountpoint);
        if !mountpoint.exists() {
            return Err(anyhow!("Volume mountpoint does not exist on disk"));
        }

        let out_path = output
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(format!("{}.tar", name)));

        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = fs::File::create(&out_path)?;
        let mut tar = tar::Builder::new(file);
        tar.append_dir_all(".", &mountpoint)?;
        tar.finish()?;

        Ok(out_path)
    }

    pub fn import_volume(name: &str, input: &Path) -> Result<VolumeRecord> {
        if !input.exists() {
            return Err(anyhow!("Import file '{}' not found", input.display()));
        }

        let store = VolumeStore::new();
        let vol = if let Some(v) = store.find(name) {
            v
        } else {
            store.create(Some(name), None)?
        };

        let mountpoint = PathBuf::from(&vol.mountpoint);
        fs::create_dir_all(&mountpoint)?;

        let file = fs::File::open(input)?;
        let mut archive = tar::Archive::new(file);
        archive.unpack(&mountpoint)?;

        Ok(vol)
    }

    pub fn reload_volume(name: Option<&str>) -> Result<Vec<VolumeRecord>> {
        let store = VolumeStore::new();
        let vols = store.list();
        if let Some(n) = name {
            let found = vols
                .into_iter()
                .filter(|v| v.name == n)
                .collect::<Vec<_>>();
            if found.is_empty() {
                return Err(anyhow!("Volume '{}' not found", n));
            }
            Ok(found)
        } else {
            Ok(vols)
        }
    }

    pub fn rename_volume(old_name: &str, new_name: &str) -> Result<VolumeRecord> {
        let store = VolumeStore::new();
        let vol = store
            .find(old_name)
            .ok_or_else(|| anyhow!("Volume '{}' not found", old_name))?;

        if store.find(new_name).is_some() {
            return Err(anyhow!("Volume '{}' already exists", new_name));
        }

        let old_dir = PathBuf::from(&vol.mountpoint);
        let new_dir = boxr_home().join("volumes").join(new_name).join("_data");
        if let Some(parent) = new_dir.parent() {
            fs::create_dir_all(parent)?;
        }

        if old_dir.exists() {
            fs::rename(&old_dir, &new_dir)?;
            let _ = fs::remove_dir(old_dir.parent().unwrap_or(&old_dir));
        } else {
            fs::create_dir_all(&new_dir)?;
        }

        store.remove(old_name)?;
        let new_rec = store.create_with_options(
            Some(new_name),
            &vol.driver,
            Some(vol.labels.clone()),
            &vol.scope,
            Some(vol.options.clone()),
        )?;

        Ok(new_rec)
    }

    pub fn mount_volume(name: &str) -> Result<String> {
        let store = VolumeStore::new();
        let vol = store
            .find(name)
            .ok_or_else(|| anyhow!("Volume '{}' not found", name))?;

        let mountpoint = PathBuf::from(&vol.mountpoint);
        fs::create_dir_all(&mountpoint)?;
        let canonical = mountpoint.canonicalize()?;
        Ok(canonical.to_string_lossy().to_string())
    }

    pub fn unmount_volume(name: &str) -> Result<String> {
        let store = VolumeStore::new();
        let _ = store
            .find(name)
            .ok_or_else(|| anyhow!("Volume '{}' not found", name))?;
        Ok(name.to_string())
    }
}
