use crate::builder::BuildCache;
use crate::network::NetworkStore;
use crate::storage::{boxr_home, ContainerStatus, ContainerStore, ImageStore};
use crate::volume::VolumeStore;
use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Default)]
pub struct DiskUsageRow {
    pub item_type: String,
    pub total: usize,
    pub active: usize,
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
}

pub struct SystemManager;

impl SystemManager {
    pub fn df() -> Result<Vec<DiskUsageRow>> {
        let i_store = ImageStore::new();
        let c_store = ContainerStore::new();
        let v_store = VolumeStore::new();

        let images = i_store.list();
        let containers = c_store.list();
        let volumes = v_store.list();

        // 1. Containers
        let total_containers = containers.len();
        let active_containers = containers.iter().filter(|c| matches!(c.status, ContainerStatus::Running)).count();
        let mut total_container_size = 0u64;
        let mut reclaimable_container_size = 0u64;

        for c in &containers {
            let path = std::path::PathBuf::from(&c.bundle_path);
            let size = dir_size(&path);
            total_container_size += size;
            if !matches!(c.status, ContainerStatus::Running) {
                reclaimable_container_size += size;
            }
        }

        // 2. Images
        let used_image_names: HashSet<String> = containers.iter().map(|c| c.image.clone()).collect();
        let total_images = images.len();
        let mut active_images = 0;
        let mut total_image_size = 0u64;
        let mut reclaimable_image_size = 0u64;

        for img in &images {
            let tag = format!("{}:{}", img.reference, img.tag);
            let is_used = used_image_names.contains(&tag) || used_image_names.contains(&img.reference) || used_image_names.contains(&img.id);
            total_image_size += img.size_bytes as u64;

            if is_used {
                active_images += 1;
            } else {
                reclaimable_image_size += img.size_bytes as u64;
            }
        }

        // 3. Volumes
        let total_volumes = volumes.len();
        let mut total_volume_size = 0u64;
        for v in &volumes {
            let path = std::path::PathBuf::from(&v.mountpoint);
            total_volume_size += dir_size(&path);
        }

        // 4. Build Cache
        let cache_dir = boxr_home().join("buildcache");
        let mut cache_entries = 0;
        let mut cache_size = 0u64;
        if cache_dir.exists() {
            if let Ok(entries) = fs::read_dir(&cache_dir) {
                for entry in entries.flatten() {
                    cache_entries += 1;
                    cache_size += dir_size(&entry.path());
                }
            }
        }

        Ok(vec![
            DiskUsageRow {
                item_type: "Images".to_string(),
                total: total_images,
                active: active_images,
                size_bytes: total_image_size,
                reclaimable_bytes: reclaimable_image_size,
            },
            DiskUsageRow {
                item_type: "Containers".to_string(),
                total: total_containers,
                active: active_containers,
                size_bytes: total_container_size,
                reclaimable_bytes: reclaimable_container_size,
            },
            DiskUsageRow {
                item_type: "Local Volumes".to_string(),
                total: total_volumes,
                active: total_volumes, // active tracking
                size_bytes: total_volume_size,
                reclaimable_bytes: 0,
            },
            DiskUsageRow {
                item_type: "Build Cache".to_string(),
                total: cache_entries,
                active: 0,
                size_bytes: cache_size,
                reclaimable_bytes: cache_size,
            },
        ])
    }

    pub fn print_df() -> Result<()> {
        let rows = Self::df()?;
        println!("{:<16} {:<10} {:<10} {:<16} {:<20}", "TYPE", "TOTAL", "ACTIVE", "SIZE", "RECLAIMABLE");

        for r in rows {
            let size_str = format_bytes(r.size_bytes);
            let reclaim_str = if r.size_bytes > 0 && r.reclaimable_bytes > 0 {
                let pct = (r.reclaimable_bytes as f64 / r.size_bytes as f64) * 100.0;
                format!("{} ({:.0}%)", format_bytes(r.reclaimable_bytes), pct)
            } else {
                format_bytes(r.reclaimable_bytes)
            };

            println!("{:<16} {:<10} {:<10} {:<16} {:<20}",
                r.item_type, r.total, r.active, size_str, reclaim_str
            );
        }

        Ok(())
    }

    pub fn prune(all_images: bool, prune_volumes: bool) -> Result<u64> {
        let mut reclaimed = 0u64;

        // 1. Prune stopped containers
        let c_store = ContainerStore::new();
        let containers = c_store.list();
        println!("Deleted Containers:");
        for c in containers {
            if !matches!(c.status, ContainerStatus::Running) {
                let size = dir_size(&std::path::PathBuf::from(&c.bundle_path));
                reclaimed += size;
                let _ = c_store.remove(&c.id);
                println!("{}", c.id);
            }
        }

        // 2. Prune images if all_images is requested
        if all_images {
            let i_store = ImageStore::new();
            let images = i_store.list();
            let remaining_containers = c_store.list();
            let used_images: HashSet<String> = remaining_containers.iter().map(|c| c.image.clone()).collect();

            println!("Deleted Images:");
            for img in images {
                let tag = format!("{}:{}", img.reference, img.tag);
                if !used_images.contains(&tag) && !used_images.contains(&img.reference) {
                    reclaimed += img.size_bytes as u64;
                    let _ = i_store.remove(&img.id);
                    println!("deleted: sha256:{}", img.id);
                }
            }
        }

        // 3. Prune build cache
        let cache_entries = BuildCache::prune()?;
        if cache_entries > 0 {
            println!("Total reclaimed build cache entries: {}", cache_entries);
        }

        // 4. Prune unused networks
        let n_store = NetworkStore::new();
        for net in n_store.list() {
            if net.name != NetworkStore::DEFAULT_NETWORK && net.containers.is_empty() {
                let _ = n_store.remove(&net.name);
            }
        }

        // 5. Prune volumes if requested
        if prune_volumes {
            let v_store = VolumeStore::new();
            let pruned_vols = v_store.prune()?;
            for v in pruned_vols {
                println!("Deleted Volume: {}", v);
            }
        }

        println!("Total reclaimed space: {}", format_bytes(reclaimed));
        Ok(reclaimed)
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(2048), "2.00KB");
        assert_eq!(format_bytes(10 * 1024 * 1024), "10.00MB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.00GB");
    }

    #[test]
    fn test_system_df_runs() {
        let rows = SystemManager::df().unwrap();
        assert_eq!(rows.len(), 4);
    }
}
