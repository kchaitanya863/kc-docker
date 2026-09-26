use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiffChangeType {
    Changed,
    Added,
    Deleted,
}

impl std::fmt::Display for DiffChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffChangeType::Changed => write!(f, "C"),
            DiffChangeType::Added => write!(f, "A"),
            DiffChangeType::Deleted => write!(f, "D"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiffRecord {
    pub change_type: DiffChangeType,
    pub path: String,
}

pub struct FilesystemDiff;

impl FilesystemDiff {
    /// Compare container rootfs (or upperdir) against base rootfs
    pub fn compare(base_rootfs: &Path, container_rootfs: &Path) -> Result<Vec<DiffRecord>> {
        let mut diffs = Vec::new();
        let mut seen_base_files = HashSet::new();

        if !container_rootfs.exists() {
            return Err(anyhow!(
                "Container rootfs not found at {:?}",
                container_rootfs
            ));
        }

        // Walk container rootfs to find Added, Changed, and Whiteout Deleted files
        Self::walk_and_compare(
            container_rootfs,
            container_rootfs,
            base_rootfs,
            &mut diffs,
            &mut seen_base_files,
        )?;

        // Walk base rootfs to detect files that were removed from container
        if base_rootfs.exists() {
            Self::check_deleted_from_base(
                base_rootfs,
                base_rootfs,
                container_rootfs,
                &mut diffs,
                &seen_base_files,
            )?;
        }

        diffs.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(diffs)
    }

    fn walk_and_compare(
        current_dir: &Path,
        root_dir: &Path,
        base_dir: &Path,
        diffs: &mut Vec<DiffRecord>,
        seen_base: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip top-level internal runtime mount points (/proc, /sys, /dev)
            if current_dir == root_dir
                && (file_name == "proc" || file_name == "sys" || file_name == "dev")
            {
                continue;
            }

            // Skip internal runtime scaffolding files injected by boxr hypervisor
            if current_dir == root_dir
                && (file_name == "libboxr_perm.so"
                    || file_name == "libboxr_perm_x86_64.so"
                    || file_name == "boxr-busybox"
                    || file_name == "boxr-init.sh"
                    || file_name == "boxr-run.sh"
                    || file_name == "boxr-exitcode"
                    || file_name == "logs.txt"
                    || file_name.starts_with("boxr-exec-"))
            {
                continue;
            }

            let rel_path = path.strip_prefix(root_dir)?;
            let rel_str = format!("/{}", rel_path.to_string_lossy());

            // Check for OCI whiteout file
            if let Some(deleted_name) = file_name.strip_prefix(".wh.") {
                let deleted_rel = rel_path
                    .parent()
                    .unwrap_or(Path::new(""))
                    .join(deleted_name);
                diffs.push(DiffRecord {
                    change_type: DiffChangeType::Deleted,
                    path: format!("/{}", deleted_rel.to_string_lossy()),
                });
                continue;
            }

            seen_base.insert(rel_path.to_path_buf());
            let base_equiv = base_dir.join(rel_path);

            if !base_equiv.exists() {
                diffs.push(DiffRecord {
                    change_type: DiffChangeType::Added,
                    path: rel_str,
                });
            } else {
                let cont_meta = fs::symlink_metadata(&path)?;
                let base_meta = fs::symlink_metadata(&base_equiv)?;

                let mut is_different = cont_meta.file_type() != base_meta.file_type()
                    || cont_meta.len() != base_meta.len()
                    || cont_meta
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                        != base_meta
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                // If filesystem timestamps or len matched, compare file content if regular files
                if !is_different && path.is_file() && base_equiv.is_file() {
                    if let (Ok(c1), Ok(c2)) = (fs::read(&path), fs::read(&base_equiv)) {
                        if c1 != c2 {
                            is_different = true;
                        }
                    }
                }

                if is_different {
                    diffs.push(DiffRecord {
                        change_type: DiffChangeType::Changed,
                        path: rel_str,
                    });
                }
            }

            if path.is_dir() && !path.is_symlink() {
                Self::walk_and_compare(&path, root_dir, base_dir, diffs, seen_base)?;
            }
        }
        Ok(())
    }

    fn check_deleted_from_base(
        current_dir: &Path,
        base_root: &Path,
        container_root: &Path,
        diffs: &mut Vec<DiffRecord>,
        seen: &HashSet<PathBuf>,
    ) -> Result<()> {
        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if current_dir == base_root
                && (file_name == "proc" || file_name == "sys" || file_name == "dev")
            {
                continue;
            }

            let rel_path = path.strip_prefix(base_root)?;
            if !seen.contains(rel_path) && !container_root.join(rel_path).exists() {
                let rel_str = format!("/{}", rel_path.to_string_lossy());
                if !diffs.iter().any(|d| d.path == rel_str) {
                    diffs.push(DiffRecord {
                        change_type: DiffChangeType::Deleted,
                        path: rel_str,
                    });
                }
            }

            if path.is_dir() && !path.is_symlink() {
                Self::check_deleted_from_base(&path, base_root, container_root, diffs, seen)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_filesystem_diff() -> Result<()> {
        let temp = tempdir().unwrap();
        let base = temp.path().join("base");
        let container = temp.path().join("container");

        fs::create_dir_all(&base)?;
        fs::create_dir_all(&container)?;

        // Common file
        fs::write(base.join("common.txt"), b"v1")?;
        fs::write(container.join("common.txt"), b"v2")?;

        // Added file
        fs::write(container.join("new.txt"), b"new")?;

        // Deleted file
        fs::write(base.join("old.txt"), b"old")?;

        // Nested dev directory (should NOT be skipped like root /dev)
        let nested_dev = container.join("app").join("dev");
        fs::create_dir_all(&nested_dev)?;
        fs::write(nested_dev.join("config.json"), b"nested dev content")?;

        // Injected hypervisor runtime files (should be filtered out)
        fs::write(container.join("libboxr_perm.so"), b"bin")?;
        fs::write(container.join("boxr-run.sh"), b"sh")?;

        let diffs = FilesystemDiff::compare(&base, &container).unwrap();
        assert!(
            !diffs
                .iter()
                .any(|d| d.path == "/libboxr_perm.so" || d.path == "/boxr-run.sh"),
            "Injected runtime scaffolding must be filtered from diff"
        );
        assert!(
            diffs
                .iter()
                .any(|d| d.change_type == DiffChangeType::Changed && d.path == "/common.txt")
        );
        assert!(
            diffs
                .iter()
                .any(|d| d.change_type == DiffChangeType::Added && d.path == "/new.txt")
        );
        assert!(
            diffs
                .iter()
                .any(|d| d.change_type == DiffChangeType::Deleted && d.path == "/old.txt")
        );
        assert!(
            diffs
                .iter()
                .any(|d| d.change_type == DiffChangeType::Added && d.path == "/app/dev/config.json"),
            "Nested directories named dev must not be skipped"
        );

        Ok(())
    }
}
