use crate::storage::ContainerStore;
use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct ContainerCopy;

impl ContainerCopy {
    /// Parse source and destination strings: e.g. "my-container:/app/file.txt", "./local-file.txt"
    pub fn copy(src: &str, dest: &str) -> Result<()> {
        let store = ContainerStore::new();

        if let Some((container_query, container_path)) = src.split_once(':') {
            // Container to Host copy
            let cont = store.find(container_query)
                .ok_or_else(|| anyhow!("Container '{}' not found", container_query))?;

            let cont_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");
            let clean_cont_path = container_path.trim_start_matches('/');
            let source_abs = cont_rootfs.join(clean_cont_path);

            if !source_abs.exists() {
                return Err(anyhow!("Path '{}' does not exist in container '{}'", container_path, container_query));
            }

            let host_dest = PathBuf::from(dest);
            Self::copy_path(&source_abs, &host_dest)?;
            println!("Successfully copied {}:{} to {:?}", container_query, container_path, host_dest);
            Ok(())
        } else if let Some((container_query, container_path)) = dest.split_once(':') {
            // Host to Container copy
            let cont = store.find(container_query)
                .ok_or_else(|| anyhow!("Container '{}' not found", container_query))?;

            let host_src = PathBuf::from(src);
            if !host_src.exists() {
                return Err(anyhow!("Host path '{:?}' does not exist", host_src));
            }

            let cont_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");
            let clean_cont_path = container_path.trim_start_matches('/');
            let dest_abs = cont_rootfs.join(clean_cont_path);

            Self::copy_path(&host_src, &dest_abs)?;
            println!("Successfully copied {:?} to {}:{}", host_src, container_query, container_path);
            Ok(())
        } else {
            Err(anyhow!("Invalid copy syntax: at least one of SRC or DEST must specify <container>:<path>"))
        }
    }

    fn copy_path(src: &Path, dst: &Path) -> Result<()> {
        let meta = fs::symlink_metadata(src)?;

        if meta.is_dir() {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let file_name = entry.file_name();
                let sub_src = entry.path();
                let sub_dst = dst.join(file_name);
                Self::copy_path(&sub_src, &sub_dst)?;
            }
        } else if meta.file_type().is_symlink() {
            #[cfg(unix)]
            {
                if let Ok(link_target) = fs::read_link(src) {
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if dst.exists() {
                        let _ = fs::remove_file(dst);
                    }
                    let _ = std::os::unix::fs::symlink(link_target, dst);
                }
            }
        } else {
            let target_file = if dst.is_dir() {
                let file_name = src.file_name().unwrap();
                dst.join(file_name)
            } else {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                dst.to_path_buf()
            };

            fs::copy(src, target_file)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_copy_path_recursive() {
        let temp = tempdir().unwrap();
        let src_dir = temp.path().join("src");
        let dst_dir = temp.path().join("dst");

        fs::create_dir_all(src_dir.join("subdir")).unwrap();
        fs::write(src_dir.join("file.txt"), b"test content").unwrap();
        fs::write(src_dir.join("subdir").join("sub.txt"), b"sub content").unwrap();

        ContainerCopy::copy_path(&src_dir, &dst_dir).unwrap();

        assert!(dst_dir.join("file.txt").exists());
        assert!(dst_dir.join("subdir").join("sub.txt").exists());
        assert_eq!(fs::read_to_string(dst_dir.join("file.txt")).unwrap(), "test content");
    }
}
