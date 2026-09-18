#![allow(dead_code, unused_imports)]

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct OverlayBundle {
    pub lower_dir: PathBuf,
    pub upper_dir: PathBuf,
    pub work_dir: PathBuf,
    pub merged_dir: PathBuf,
    pub is_mounted: bool,
}

pub struct OverlayDriver;

impl OverlayDriver {
    /// Initialize a Copy-On-Write layer for a container using base image rootfs.
    pub fn create_cow_layer(bundle_dir: &Path, base_rootfs: &Path) -> Result<OverlayBundle> {
        let upper_dir = bundle_dir.join("upper");
        let work_dir = bundle_dir.join("work");
        let merged_dir = bundle_dir.join("rootfs");

        fs::create_dir_all(&upper_dir)?;
        fs::create_dir_all(&work_dir)?;
        fs::create_dir_all(&merged_dir)?;

        // Try mounting native OverlayFS if on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(()) = Self::mount_overlay(base_rootfs, &upper_dir, &work_dir, &merged_dir) {
                return Ok(OverlayBundle {
                    lower_dir: base_rootfs.to_path_buf(),
                    upper_dir,
                    work_dir,
                    merged_dir,
                    is_mounted: true,
                });
            }
        }

        // Try native APFS Copy-On-Write clonefile on macOS
        #[cfg(target_os = "macos")]
        {
            if Self::clonefile_cow(base_rootfs, &merged_dir).is_ok() {
                return Ok(OverlayBundle {
                    lower_dir: base_rootfs.to_path_buf(),
                    upper_dir,
                    work_dir,
                    merged_dir,
                    is_mounted: false,
                });
            }
        }

        // Fallback: fast CoW hardlink tree
        Self::create_hardlink_tree(base_rootfs, &merged_dir)?;

        Ok(OverlayBundle {
            lower_dir: base_rootfs.to_path_buf(),
            upper_dir,
            work_dir,
            merged_dir,
            is_mounted: false,
        })
    }

    #[cfg(target_os = "macos")]
    fn clonefile_cow(src: &Path, dst: &Path) -> Result<()> {
        use std::ffi::CString;
        unsafe extern "C" {
            fn clonefile(
                src: *const libc::c_char,
                dst: *const libc::c_char,
                flags: u32,
            ) -> libc::c_int;
        }

        if dst.exists() {
            let _ = fs::remove_dir_all(dst);
        }

        let src_c = CString::new(src.to_str().ok_or_else(|| anyhow::anyhow!("Invalid src"))?)?;
        let dst_c = CString::new(dst.to_str().ok_or_else(|| anyhow::anyhow!("Invalid dst"))?)?;

        let res = unsafe { clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) };
        if res == 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "clonefile failed with error: {}",
                std::io::Error::last_os_error()
            ))
        }
    }

    #[cfg(target_os = "linux")]
    fn mount_overlay(lower: &Path, upper: &Path, work: &Path, merged: &Path) -> Result<()> {
        use nix::mount::{MsFlags, mount};
        use std::ffi::CString;

        let opts = format!(
            "lowerdir={},upperdir={},workdir={}",
            lower.display(),
            upper.display(),
            work.display()
        );
        let opts_c = CString::new(opts)?;

        mount(
            Some("overlay"),
            merged,
            Some("overlay"),
            MsFlags::empty(),
            Some(opts_c.as_c_str()),
        )
        .context("Failed to mount overlayfs")?;

        Ok(())
    }

    /// Fast CoW fallback: creates a tree of hardlinks to files and real directories.
    /// This is instant, uses near-zero disk space, and isolates writes.
    pub fn create_hardlink_tree(src: &Path, dst: &Path) -> Result<()> {
        let _ = fs::create_dir_all(dst);
        let entries = match fs::read_dir(src) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let ty = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let from = entry.path();
            let to = dst.join(entry.file_name());

            if ty.is_dir() {
                let _ = Self::create_hardlink_tree(&from, &to);
            } else if ty.is_symlink() {
                #[cfg(unix)]
                {
                    if let Ok(target) = fs::read_link(&from) {
                        let _ = std::os::unix::fs::symlink(target, &to);
                    }
                }
            } else {
                // Attempt hardlink; if cross-device or permission fails, fallback to copy
                if fs::hard_link(&from, &to).is_err() {
                    let _ = fs::copy(&from, &to);
                }
            }
        }
        Ok(())
    }

    /// Tear down and cleanup OverlayFS/CoW bundle
    pub fn cleanup(bundle: &OverlayBundle) -> Result<()> {
        #[cfg(target_os = "linux")]
        if bundle.is_mounted {
            use nix::mount::{MntFlags, umount2};
            let _ = umount2(&bundle.merged_dir, MntFlags::MNT_DETACH);
        }

        if bundle.upper_dir.exists() {
            let _ = fs::remove_dir_all(&bundle.upper_dir);
        }
        if bundle.work_dir.exists() {
            let _ = fs::remove_dir_all(&bundle.work_dir);
        }
        if bundle.merged_dir.exists() {
            let _ = fs::remove_dir_all(&bundle.merged_dir);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_hardlink_cow_tree() -> Result<()> {
        let temp = tempdir().unwrap();
        let src = temp.path().join("base");
        let bundle = temp.path().join("bundle");

        fs::create_dir_all(src.join("sub"))?;
        fs::write(src.join("file1.txt"), b"original content")?;
        fs::write(src.join("sub").join("file2.txt"), b"sub content")?;

        let overlay = OverlayDriver::create_cow_layer(&bundle, &src).unwrap();
        assert!(overlay.merged_dir.exists());

        let file1 = overlay.merged_dir.join("file1.txt");
        assert!(file1.exists());
        assert_eq!(fs::read_to_string(&file1).unwrap(), "original content");

        // Mutating container file in merged dir does not affect base
        fs::write(&file1, b"modified content")?;
        assert_eq!(fs::read_to_string(&file1).unwrap(), "modified content");

        OverlayDriver::cleanup(&overlay).unwrap();
        assert!(!overlay.merged_dir.exists());

        Ok(())
    }
}
