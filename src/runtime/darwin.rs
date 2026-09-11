use crate::oci::runtime::Spec;
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::Command;

/// Execute an OCI container bundle on macOS using the Linux VM execution bridge.
/// Since macOS kernel (XNU) cannot natively execute Linux ELF binaries, this bridge
/// mounts the Boxr-generated OCI rootfs and executes the process inside an isolated
/// Linux container environment, streaming stdout/stderr and returning the exit code.
pub fn execute_bundle(bundle_path: &Path, spec: &Spec) -> Result<i32> {
    let rootfs_path = bundle_path.join(&spec.root.path);
    if !rootfs_path.exists() {
        return Err(anyhow!("Rootfs not found at {:?}", rootfs_path));
    }

    let abs_rootfs = rootfs_path
        .canonicalize()
        .context("Failed to canonicalize rootfs path")?;

    let rootfs_str = abs_rootfs
        .to_str()
        .ok_or_else(|| anyhow!("Invalid rootfs path"))?;

    let cmd_binary = spec
        .process
        .args
        .first()
        .ok_or_else(|| anyhow!("Process args cannot be empty"))?;

    let cmd_args = &spec.process.args[1..];

    // Build the command to run inside the Linux VM runner
    // We bind-mount the unpacked rootfs to /boxr-rootfs, prepare /proc and /dev, then chroot
    let mut shell_script = String::new();
    shell_script.push_str("mkdir -p /boxr-rootfs/proc /boxr-rootfs/sys /boxr-rootfs/dev 2>/dev/null || true; ");
    shell_script.push_str("mount -t proc proc /boxr-rootfs/proc 2>/dev/null || true; ");
    shell_script.push_str("mount -t sysfs sysfs /boxr-rootfs/sys 2>/dev/null || true; ");
    shell_script.push_str("mount --bind /dev /boxr-rootfs/dev 2>/dev/null || true; ");
    shell_script.push_str(&format!(
        "cd \"/boxr-rootfs{}\" 2>/dev/null || cd /boxr-rootfs; ",
        spec.process.cwd
    ));

    // Construct the command invocation
    let mut exec_line = format!("chroot /boxr-rootfs {}", cmd_binary);
    for arg in cmd_args {
        exec_line.push_str(&format!(" \"{}\"", arg.replace('"', "\\\"")));
    }
    shell_script.push_str(&exec_line);

    // Build docker runner command
    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("--rm")
        .arg("-i")
        .arg("--privileged")
        .arg("-v")
        .arg(format!("{}:/boxr-rootfs", rootfs_str));

    // Environment variables
    for env_var in &spec.process.env {
        cmd.arg("-e").arg(env_var);
    }

    if let Some(hostname) = &spec.hostname {
        cmd.arg("-h").arg(hostname);
    }

    cmd.arg("alpine").arg("/bin/sh").arg("-c").arg(shell_script);

    let mut child = cmd
        .spawn()
        .context("Failed to spawn container execution bridge (check if Docker Desktop / Linux VM is running)")?;

    let status = child.wait()?;
    let code = status.code().unwrap_or(1);
    Ok(code)
}
