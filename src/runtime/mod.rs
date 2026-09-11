pub mod cp;
pub mod diff;
pub mod kill;
pub mod top;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::{exec_in_bundle, execute_bundle};

#[cfg(target_os = "macos")]
pub mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::{exec_in_bundle, execute_bundle};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn execute_bundle(
    _bundle_path: &Path,
    _spec: &Spec,
    _mounts: &[MountSpec],
    _ports: &[PortMapping],
    _detach: bool,
) -> Result<i32> {
    anyhow::bail!("Container execution is only supported on Linux and macOS");
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn exec_in_bundle(_bundle_path: &Path, _command: &[String], _env: &[String]) -> Result<i32> {
    anyhow::bail!("Container execution is only supported on Linux and macOS");
}
