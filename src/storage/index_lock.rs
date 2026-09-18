use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::path::Path;

/// Run `f` while holding an exclusive lock on the JSON index file.
pub fn with_index_lock<T>(index_file: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    if let Some(parent) = index_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let lock_path = index_file.with_extension("lock");
    let _lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("Failed to open index lock {:?}", lock_path))?;

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = _lock_file.as_raw_fd();
        loop {
            let res = unsafe { libc::flock(fd, libc::LOCK_EX) };
            if res == 0 {
                break;
            }
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(anyhow::anyhow!(
                "Failed to acquire exclusive lock on {:?}: {}",
                lock_path,
                err
            ));
        }

        struct FlockGuard(i32);
        impl Drop for FlockGuard {
            fn drop(&mut self) {
                unsafe {
                    libc::flock(self.0, libc::LOCK_UN);
                }
            }
        }
        let _guard = FlockGuard(fd);
        f()
    }

    #[cfg(not(unix))]
    f()
}
