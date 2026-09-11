pub mod container_store;
pub mod image_store;

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub use container_store::{ContainerRecord, ContainerStatus, ContainerStore};
pub use image_store::{ImageRecord, ImageStore};

/// Get the base boxr directory (default: ~/.boxr)
pub fn boxr_home() -> PathBuf {
    if let Ok(custom) = std::env::var("BOXR_HOME") {
        return PathBuf::from(custom);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".boxr")
}

pub fn ensure_directories() -> Result<PathBuf> {
    let home = boxr_home();
    fs::create_dir_all(home.join("layers"))?;
    fs::create_dir_all(home.join("images"))?;
    fs::create_dir_all(home.join("containers"))?;
    Ok(home)
}
