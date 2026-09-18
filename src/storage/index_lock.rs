use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::path::Path;

/// Run `f` while holding an exclusive lock on the JSON index file.
pub fn with_index_lock<T>(index_file: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
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
        unsafe {
            libc::flock(fd, libc::LOCK_EX);
        }
        let result = f();
        unsafe {
            libc::flock(fd, libc::LOCK_UN);
        }
        return result;
    }

    #[cfg(not(unix))]
    f()
}
