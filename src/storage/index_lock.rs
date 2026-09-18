use anyhow::{Context, Result};
use std::fs::OpenOptions;
use std::path::Path;

/// Run `f` while holding an exclusive lock on the JSON index file.
pub fn with_index_lock<T>(index_file: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let lock_path = index_file.with_extension("lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("Failed to open index lock {:?}", lock_path))?;

    #[cfg(unix)]
    {
        use nix::fcntl::{FlockArg, flock};
        use std::os::unix::io::AsRawFd;

        flock(lock_file.as_raw_fd(), FlockArg::LockExclusive)
            .with_context(|| format!("Failed to acquire lock {:?}", lock_path))?;
        let result = f();
        let _ = flock(lock_file.as_raw_fd(), FlockArg::Unlock);
        return result;
    }

    #[cfg(not(unix))]
    f()
}
