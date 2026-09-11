#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::execute_bundle;

#[cfg(target_os = "macos")]
pub mod darwin;
#[cfg(target_os = "macos")]
pub use darwin::execute_bundle;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn execute_bundle(_bundle_path: &std::path::Path, _spec: &crate::oci::runtime::Spec) -> anyhow::Result<i32> {
    anyhow::bail!("Container execution is only supported on Linux and macOS");
}
