use crate::network::PortMapping;
use crate::oci::runtime::Spec;
use crate::storage::boxr_home;
use crate::volume::MountSpec;
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Ensure native Apple Virtualization runner binary is compiled and codesigned
pub fn ensure_vz_runner() -> Result<PathBuf> {
    let home = boxr_home();
    let bin_dir = home.join("bin");
    let runner_bin = bin_dir.join("boxr-vz");

    if runner_bin.exists() {
        return Ok(runner_bin);
    }

    fs::create_dir_all(&bin_dir)?;

    let vz_source = include_str!("boxr-vz.m");
    let entitlements = include_str!("boxr-vz.entitlements");

    let temp_dir = tempfile::tempdir()?;
    let m_file = temp_dir.path().join("boxr-vz.m");
    let ent_file = temp_dir.path().join("boxr-vz.entitlements");

    fs::write(&m_file, vz_source)?;
    fs::write(&ent_file, entitlements)?;

    let status = Command::new("clang")
        .args([
            "-O3",
            "-fobjc-arc",
            "-framework",
            "Foundation",
            "-framework",
            "Virtualization",
            m_file.to_str().unwrap(),
            "-o",
            runner_bin.to_str().unwrap(),
        ])
        .status()
        .context("Failed to compile native Apple Virtualization runner (clang required)")?;

    if !status.success() {
        return Err(anyhow!("clang failed to compile native boxr-vz runner"));
    }

    let sign_status = Command::new("codesign")
        .args([
            "-s",
            "-",
            "--entitlements",
            ent_file.to_str().unwrap(),
            "-f",
            runner_bin.to_str().unwrap(),
        ])
        .status()
        .context("Failed to codesign boxr-vz with virtualization entitlement")?;

    if !sign_status.success() {
        return Err(anyhow!("codesign failed for boxr-vz"));
    }

    Ok(runner_bin)
}

/// Ensure Linux kernel and initrd exist in ~/.boxr/vm/
pub fn ensure_vm_assets() -> Result<(PathBuf, PathBuf)> {
    let vm_dir = boxr_home().join("vm");
    let kernel_path = vm_dir.join("vmlinux");
    let initrd_path = vm_dir.join("initrd.cpio.gz");

    if kernel_path.exists() && initrd_path.exists() {
        return Ok((kernel_path, initrd_path));
    }

    fs::create_dir_all(&vm_dir)?;

    println!(
        "Initializing native Apple Silicon Linux kernel and micro-VM initrd in ~/.boxr/vm/..."
    );

    let kernel_url =
        "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/aarch64/netboot/vmlinuz-virt";
    let initrd_url =
        "https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/aarch64/netboot/initramfs-virt";

    let temp_kernel = vm_dir.join("vmlinuz-virt.tmp");
    let status_k = Command::new("curl")
        .args(["-fsSL", kernel_url, "-o", temp_kernel.to_str().unwrap()])
        .status()
        .context("Failed to download Linux kernel via curl")?;

    if !status_k.success() {
        return Err(anyhow!("curl failed downloading Linux kernel"));
    }

    let k_bytes = fs::read(&temp_kernel)?;
    let _ = fs::remove_file(&temp_kernel);

    // Decompress zimg payload (gzip starting at offset 51568 or search for gzip magic)
    let gz_magic = [0x1fu8, 0x8bu8];
    let gz_offset = k_bytes
        .windows(2)
        .position(|w| w == gz_magic)
        .ok_or_else(|| anyhow!("Could not find gzip payload in kernel"))?;

    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(&k_bytes[gz_offset..]);
    let mut decompressed_kernel = Vec::new();
    decoder.read_to_end(&mut decompressed_kernel)?;
    fs::write(&kernel_path, decompressed_kernel)?;

    let status_i = Command::new("curl")
        .args(["-fsSL", initrd_url, "-o", initrd_path.to_str().unwrap()])
        .status()
        .context("Failed to download initramfs via curl")?;

    if !status_i.success() {
        return Err(anyhow!("curl failed downloading initramfs"));
    }

    println!("✓ Native Linux micro-VM assets successfully installed.");
    Ok((kernel_path, initrd_path))
}

/// Execute an OCI container bundle on macOS using Apple's native Virtualization.framework.
pub fn execute_bundle(
    bundle_path: &Path,
    spec: &Spec,
    mounts: &[MountSpec],
    _ports: &[PortMapping],
    detach: bool,
) -> Result<i32> {
    let raw_rootfs = PathBuf::from(&spec.root.path);
    let rootfs_path = if raw_rootfs.is_absolute() {
        raw_rootfs
    } else {
        bundle_path.join(&spec.root.path)
    };

    if !rootfs_path.exists() {
        return Err(anyhow!("Rootfs not found at {:?}", rootfs_path));
    }

    let abs_rootfs = rootfs_path
        .canonicalize()
        .context("Failed to canonicalize rootfs path")?;

    let cmd_binary = spec
        .process
        .args
        .first()
        .ok_or_else(|| anyhow!("Process args cannot be empty"))?;

    let cmd_args = &spec.process.args[1..];

    if let Some(ann) = &spec.annotations {
        if let Some(platform) = ann.get("boxr.platform") {
            if platform.starts_with("windows") {
                return Err(anyhow!(
                    "Windows container execution requires a native Windows host (Windows Server or Windows 10/11 with Containers feature enabled). The OCI image was successfully downloaded and stored locally."
                ));
            }
        }
    }

    let runner_bin = ensure_vz_runner()?;
    let (kernel_path, initrd_path) = ensure_vm_assets()?;

    let mut cmd = Command::new(&runner_bin);
    cmd.arg("--bundle").arg(bundle_path);
    cmd.arg("--rootfs").arg(&abs_rootfs);
    cmd.arg("--kernel").arg(&kernel_path);
    cmd.arg("--initrd").arg(&initrd_path);

    // Build the runner script inside the container's rootfs
    let mut run_script = String::new();
    run_script.push_str("#!/bin/sh\n");
    run_script.push_str("mount -t proc proc /proc 2>/dev/null || true\n");
    run_script.push_str("mount -t sysfs sysfs /sys 2>/dev/null || true\n");
    run_script.push_str("mount -t devtmpfs devtmpfs /dev 2>/dev/null || true\n");
    run_script.push_str("ip link set lo up 2>/dev/null || ifconfig lo up 2>/dev/null || true\n");
    run_script.push_str("if [ ! -s /etc/resolv.conf ]; then printf 'nameserver 1.1.1.1\\nnameserver 8.8.8.8\\n' > /etc/resolv.conf 2>/dev/null || true; fi\n");
    run_script
        .push_str("mkdir -p /tmp /data 2>/dev/null; chmod 1777 /tmp /data 2>/dev/null || true\n");

    // Volume mounts
    for (idx, m) in mounts.iter().enumerate() {
        let tag = format!("m{}", idx);
        cmd.arg("--mount")
            .arg(format!("{}={}", tag, m.source.display()));
        run_script.push_str(&format!(
            "mkdir -p \"{}\" 2>/dev/null; mount -t virtiofs \"{}\" \"{}\" 2>/dev/null || true\n",
            m.destination, tag, m.destination
        ));
    }

    // Environment variables
    for env_var in &spec.process.env {
        if let Some((k, v)) = env_var.split_once('=') {
            run_script.push_str(&format!(
                "export {}=\"{}\"\n",
                k,
                v.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
    }

    if let Some(hostname) = &spec.hostname {
        run_script.push_str(&format!("hostname \"{}\" 2>/dev/null || true\n", hostname));
    }

    // Working directory
    let cwd = if spec.process.cwd.is_empty() {
        "/"
    } else {
        &spec.process.cwd
    };
    run_script.push_str(&format!("cd \"{}\" 2>/dev/null || cd /\n", cwd));

    fs::create_dir_all(bundle_path)?;

    // Construct command invocation
    let mut cmd_line = format!("{}", cmd_binary);
    for arg in cmd_args {
        let cleaned = arg.trim_matches('"');
        cmd_line.push_str(&format!(
            " \"{}\"",
            cleaned.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }

    if detach {
        run_script.push_str(&format!("{} 2>&1 | tee /logs.txt &\n", cmd_line));
        run_script.push_str("sleep 0.1\n");
        run_script.push_str("MAIN_PID=$!\n");
        run_script.push_str("while kill -0 $MAIN_PID 2>/dev/null; do\n");
        run_script.push_str("  for f in /boxr-exec-*.sh; do\n");
        run_script.push_str("    [ -f \"$f\" ] || continue\n");
        run_script.push_str("    ID=$(echo \"$f\" | sed 's/.*boxr-exec-//; s/\\.sh//')\n");
        run_script.push_str("    /bin/sh \"$f\" > \"/boxr-exec-${ID}.log\" 2>&1\n");
        run_script.push_str("    echo $? > \"/boxr-exec-${ID}.done\"\n");
        run_script.push_str("    rm -f \"$f\"\n");
        run_script.push_str("  done\n");
        run_script.push_str("  sleep 0.05 2>/dev/null || sleep 1\n");
        run_script.push_str("done\n");
        run_script.push_str("wait $MAIN_PID 2>/dev/null\n");
        run_script.push_str("exit $?\n");
    } else {
        run_script.push_str(&format!("exec {}\n", cmd_line));
    }

    let run_script_path = rootfs_path.join("boxr-run.sh");
    fs::write(&run_script_path, run_script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&run_script_path, fs::Permissions::from_mode(0o777));
    }

    if detach {
        cmd.arg("--detach");
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        let _child = cmd.spawn()?;
        // Give background VM a brief moment to boot
        std::thread::sleep(std::time::Duration::from_millis(200));
        return Ok(0);
    }

    let status = cmd.status()?;
    Ok(status.code().unwrap_or(0))
}

/// Execute a command in an existing container bundle
pub fn exec_in_bundle(bundle_path: &Path, command: &[String], env: &[String]) -> Result<i32> {
    let rootfs_path = bundle_path.join("rootfs");
    let pid_file = bundle_path.join("vm.pid");

    let is_running = if let Ok(pid_str) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            unsafe { libc::kill(pid, 0) == 0 }
        } else {
            false
        }
    } else {
        false
    };

    if is_running {
        let mut exec_script = String::new();
        exec_script.push_str("#!/bin/sh\n");
        exec_script.push_str(
            "export PATH=\"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH\"\n",
        );
        for e in env {
            if let Some((k, v)) = e.split_once('=') {
                exec_script.push_str(&format!(
                    "export {}=\"{}\"\n",
                    k,
                    v.replace('\\', "\\\\").replace('"', "\\\"")
                ));
            }
        }
        let binary = &command[0];
        let args = &command[1..];
        let mut cmd_line = format!("exec {}", binary);
        for a in args {
            let cleaned = a.trim_matches('"');
            cmd_line.push_str(&format!(
                " \"{}\"",
                cleaned.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
        exec_script.push_str(&format!("{}\n", cmd_line));

        let exec_id = hex::encode(crate::storage::container_store::rand_id());
        let exec_script_path = rootfs_path.join(format!("boxr-exec-{}.sh", exec_id));
        let exec_done_path = rootfs_path.join(format!("boxr-exec-{}.done", exec_id));
        let exec_log_path = rootfs_path.join(format!("boxr-exec-{}.log", exec_id));

        let _ = fs::remove_file(&exec_done_path);
        let _ = fs::remove_file(&exec_log_path);
        fs::write(&exec_script_path, &exec_script)?;

        let start = std::time::Instant::now();
        while !exec_done_path.exists() {
            if start.elapsed() > std::time::Duration::from_secs(30) {
                let _ = fs::remove_file(&exec_script_path);
                return Err(anyhow!("Exec timed out after 30 seconds"));
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }

        if exec_log_path.exists() {
            let out = fs::read_to_string(&exec_log_path)?;
            print!("{}", out);
            let _ = fs::remove_file(&exec_log_path);
        }

        let exit_code = if let Ok(c) = fs::read_to_string(&exec_done_path) {
            c.trim().parse::<i32>().unwrap_or(0)
        } else {
            0
        };
        let _ = fs::remove_file(&exec_done_path);
        return Ok(exit_code);
    }

    let runner_bin = ensure_vz_runner()?;
    let (kernel_path, initrd_path) = ensure_vm_assets()?;

    let binary = &command[0];
    let args = &command[1..];

    let mut run_script = String::new();
    run_script.push_str("#!/bin/sh\n");
    run_script.push_str("mount -t proc proc /proc 2>/dev/null || true\n");
    run_script.push_str("mount -t sysfs sysfs /sys 2>/dev/null || true\n");
    run_script.push_str("mount -t devtmpfs devtmpfs /dev 2>/dev/null || true\n");

    for e in env {
        if let Some((k, v)) = e.split_once('=') {
            run_script.push_str(&format!(
                "export {}=\"{}\"\n",
                k,
                v.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
    }

    let mut cmd_line = format!("exec {}", binary);
    for a in args {
        let cleaned = a.trim_matches('"');
        cmd_line.push_str(&format!(
            " \"{}\"",
            cleaned.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    run_script.push_str(&format!("{}\n", cmd_line));

    let run_script_path = rootfs_path.join("boxr-run.sh");
    fs::write(&run_script_path, run_script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&run_script_path, fs::Permissions::from_mode(0o777));
    }

    let status = Command::new(&runner_bin)
        .arg("--bundle")
        .arg(bundle_path)
        .arg("--rootfs")
        .arg(&rootfs_path)
        .arg("--kernel")
        .arg(&kernel_path)
        .arg("--initrd")
        .arg(&initrd_path)
        .status()?;

    Ok(status.code().unwrap_or(0))
}
