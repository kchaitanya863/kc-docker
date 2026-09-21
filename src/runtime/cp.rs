use crate::storage::ContainerStore;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

pub struct ContainerCopy;

impl ContainerCopy {
    /// Securely resolve a container-internal path against the container rootfs,
    /// strictly preventing any `..` path traversal or symlink escapes outside rootfs.
    pub fn resolve_container_path(cont_rootfs: &Path, container_path: &str) -> Result<PathBuf> {
        let clean = container_path.trim_start_matches('/');
        let mut resolved = cont_rootfs.to_path_buf();
        for comp in Path::new(clean).components() {
            match comp {
                std::path::Component::Normal(c) => resolved.push(c),
                std::path::Component::ParentDir => {
                    if resolved > cont_rootfs.to_path_buf() {
                        resolved.pop();
                    } else {
                        return Err(anyhow!(
                            "Path traversal rejected: container path '{}' escapes rootfs",
                            container_path
                        ));
                    }
                }
                std::path::Component::CurDir => {}
                _ => {
                    return Err(anyhow!(
                        "Invalid path component in container path: '{}'",
                        container_path
                    ));
                }
            }
        }

        let canon_root = cont_rootfs.canonicalize()?;

        // Verify each intermediate component and symlink stays inside container rootfs
        let mut check_path = cont_rootfs.to_path_buf();
        for comp in Path::new(clean).components() {
            if let std::path::Component::Normal(c) = comp {
                check_path.push(c);
                if fs::symlink_metadata(&check_path).is_ok() {
                    let canon_check = check_path.canonicalize()?;
                    if !canon_check.starts_with(&canon_root) {
                        return Err(anyhow!(
                            "Path traversal rejected: container path component '{}' resolves outside rootfs",
                            container_path
                        ));
                    }
                }
            }
        }

        if resolved.exists() {
            let canon_res = resolved.canonicalize()?;
            if !canon_res.starts_with(&canon_root) {
                return Err(anyhow!(
                    "Path traversal rejected: container path '{}' resolves outside rootfs",
                    container_path
                ));
            }
        } else {
            let mut curr = resolved.as_path();
            while let Some(parent) = curr.parent() {
                if parent.exists() {
                    let canon_parent = parent.canonicalize()?;
                    if !canon_parent.starts_with(&canon_root) {
                        return Err(anyhow!(
                            "Path traversal rejected: container parent path '{}' resolves outside rootfs",
                            container_path
                        ));
                    }
                    break;
                }
                curr = parent;
            }
        }

        Ok(resolved)
    }

    /// Parse source and destination strings: e.g. "my-container:/app/file.txt", "./local-file.txt"
    pub fn copy(src: &str, dest: &str) -> Result<()> {
        let is_src_cont = !src.starts_with('.') && !src.starts_with('/') && src.contains(':');
        let is_dest_cont = !dest.starts_with('.') && !dest.starts_with('/') && dest.contains(':');

        let store = ContainerStore::new();

        if is_src_cont && is_dest_cont {
            return Self::copy_container_to_container(&store, src, dest);
        }

        if is_src_cont {
            let (container_query, container_path) = src.split_once(':').unwrap();
            // Container to Host copy
            let cont = store
                .find(container_query)
                .ok_or_else(|| anyhow!("Container '{}' not found", container_query))?;

            let cont_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");
            let source_abs = Self::resolve_container_path(&cont_rootfs, container_path)?;

            if !source_abs.exists() {
                return Err(anyhow!(
                    "Path '{}' does not exist in container '{}'",
                    container_path,
                    container_query
                ));
            }

            let host_dest = PathBuf::from(dest);
            Self::copy_path(&source_abs, &host_dest)?;
            println!(
                "Successfully copied {}:{} to {:?}",
                container_query, container_path, host_dest
            );
            Ok(())
        } else if is_dest_cont {
            let (container_query, container_path) = dest.split_once(':').unwrap();
            // Host to Container copy
            let cont = store
                .find(container_query)
                .ok_or_else(|| anyhow!("Container '{}' not found", container_query))?;

            let host_src = PathBuf::from(src);
            if !host_src.exists() {
                return Err(anyhow!("Host path '{:?}' does not exist", host_src));
            }

            let cont_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");
            let dest_abs = Self::resolve_container_path(&cont_rootfs, container_path)?;

            Self::copy_path(&host_src, &dest_abs)?;
            println!(
                "Successfully copied {:?} to {}:{}",
                host_src, container_query, container_path
            );
            Ok(())
        } else {
            Err(anyhow!(
                "Invalid copy syntax: at least one of SRC or DEST must specify <container>:<path>"
            ))
        }
    }

    fn copy_container_to_container(
        store: &ContainerStore,
        src: &str,
        dest: &str,
    ) -> Result<()> {
        let (src_query, src_path) = src.split_once(':').unwrap();
        let (dest_query, dest_path) = dest.split_once(':').unwrap();

        let src_cont = store
            .find(src_query)
            .ok_or_else(|| anyhow!("Container '{}' not found", src_query))?;
        let dest_cont = store
            .find(dest_query)
            .ok_or_else(|| anyhow!("Container '{}' not found", dest_query))?;

        let src_rootfs = PathBuf::from(&src_cont.bundle_path).join("rootfs");
        let dest_rootfs = PathBuf::from(&dest_cont.bundle_path).join("rootfs");

        let source_abs = Self::resolve_container_path(&src_rootfs, src_path)?;
        if !source_abs.exists() {
            return Err(anyhow!(
                "Path '{}' does not exist in container '{}'",
                src_path,
                src_query
            ));
        }

        let dest_abs = Self::resolve_container_path(&dest_rootfs, dest_path)?;
        Self::copy_path(&source_abs, &dest_abs)?;
        println!(
            "Successfully copied {}:{} to {}:{}",
            src_query, src_path, dest_query, dest_path
        );
        Ok(())
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
        assert_eq!(
            fs::read_to_string(dst_dir.join("file.txt")).unwrap(),
            "test content"
        );
    }

    #[test]
    fn test_resolve_container_path_traversal_rejection() {
        let temp = tempdir().unwrap();
        let rootfs = temp.path().join("rootfs");
        fs::create_dir_all(&rootfs).unwrap();

        // Valid subpath
        let valid = ContainerCopy::resolve_container_path(&rootfs, "/app/test.txt").unwrap();
        assert_eq!(valid, rootfs.join("app/test.txt"));

        // Path traversal with .. escaping rootfs
        let err1 = ContainerCopy::resolve_container_path(&rootfs, "../../../etc/passwd");
        assert!(err1.is_err());
        assert!(
            err1.unwrap_err()
                .to_string()
                .contains("Path traversal rejected")
        );

        // Path traversal sneaking in subfolder
        let err2 = ContainerCopy::resolve_container_path(&rootfs, "/app/../../../../etc/shadow");
        assert!(err2.is_err());
        assert!(
            err2.unwrap_err()
                .to_string()
                .contains("Path traversal rejected")
        );

        // Symlink pointing outside rootfs, target file does not yet exist
        #[cfg(unix)]
        {
            let outside = temp.path().join("outside_secret");
            fs::create_dir_all(&outside).unwrap();
            let symlink_path = rootfs.join("escape_link");
            let _ = std::os::unix::fs::symlink(&outside, &symlink_path);

            let err3 = ContainerCopy::resolve_container_path(&rootfs, "/escape_link/new_file.txt");
            assert!(err3.is_err());
            assert!(
                err3.unwrap_err()
                    .to_string()
                    .contains("Path traversal rejected")
            );
        }
    }

    #[test]
    fn test_copy_syntax_validation() {
        // Container to container requires existing containers
        let err = ContainerCopy::copy("c1:/file", "c2:/file");
        assert!(err.is_err());

        // Host path with colon should not be parsed as container
        let err2 = ContainerCopy::copy("./local:file", "./dest:file");
        assert!(err2.is_err());
        assert!(err2.unwrap_err().to_string().contains("Invalid copy syntax"));
    }
}
