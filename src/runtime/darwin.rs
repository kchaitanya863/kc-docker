use crate::network::PortMapping;
use crate::oci::runtime::Spec;
use crate::volume::MountSpec;
use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn find_real_docker_bin() -> String {
    // Check known Docker binary locations avoiding ~/.boxr/bin/docker loop
    let known_paths = [
        "/usr/local/bin/docker",
        "/opt/homebrew/bin/docker",
        "/Applications/Docker.app/Contents/Resources/bin/docker",
        "/Users/knaragam/.docker/bin/docker",
    ];
    for p in known_paths {
        if let Ok(meta) = std::fs::metadata(p) {
            if meta.is_file() {
                return p.to_string();
            }
        }
    }
    "docker".to_string()
}

/// Execute an OCI container bundle on macOS using the Linux VM execution bridge.
pub fn execute_bundle(
    bundle_path: &Path,
    spec: &Spec,
    mounts: &[MountSpec],
    ports: &[PortMapping],
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

    let rootfs_str = abs_rootfs
        .to_str()
        .ok_or_else(|| anyhow!("Invalid rootfs path"))?;

    let cmd_binary = spec
        .process
        .args
        .first()
        .ok_or_else(|| anyhow!("Process args cannot be empty"))?;

    let cmd_args = &spec.process.args[1..];

    // Build the shell setup script inside the container runner
    let mut shell_script = String::new();
    shell_script.push_str("mkdir -p /boxr-rootfs/proc /boxr-rootfs/sys /boxr-rootfs/dev /boxr-rootfs/tmp /boxr-rootfs/data 2>/dev/null || true; ");
    shell_script.push_str("chmod 1777 /boxr-rootfs/tmp /boxr-rootfs/data 2>/dev/null || true; ");
    shell_script.push_str("mount -t proc proc /boxr-rootfs/proc 2>/dev/null || true; ");
    shell_script.push_str("mount -t sysfs sysfs /boxr-rootfs/sys 2>/dev/null || true; ");
    shell_script.push_str("mount --bind /dev /boxr-rootfs/dev 2>/dev/null || true; ");

    // Prepare mounts inside /boxr-rootfs
    for (idx, m) in mounts.iter().enumerate() {
        let container_mount = format!("/boxr-rootfs{}", m.destination);
        let host_mount_in_runner = format!("/boxr-mounts/m{}", idx);
        shell_script.push_str(&format!(
            "mkdir -p \"{}\" 2>/dev/null || true; mount --bind \"{}\" \"{}\" 2>/dev/null || true; ",
            container_mount, host_mount_in_runner, container_mount
        ));
    }

    shell_script.push_str(&format!(
        "cd \"/boxr-rootfs{}\" 2>/dev/null || cd /boxr-rootfs; ",
        spec.process.cwd
    ));

    // Construct command invocation
    let exec_line = if rootfs_path.join("bin/sh").exists() {
        let mut inner = format!("exec {}", cmd_binary);
        for arg in cmd_args {
            inner.push_str(&format!(
                " \\\"{}\\\"",
                arg.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
        format!("chroot /boxr-rootfs /bin/sh -c \"{}\"", inner)
    } else {
        let mut inner = format!("chroot /boxr-rootfs {}", cmd_binary);
        for arg in cmd_args {
            inner.push_str(&format!(" \"{}\"", arg.replace('"', "\\\"")));
        }
        inner
    };
    shell_script.push_str(&exec_line);

    // Build docker runner command
    let docker_bin = find_real_docker_bin();
    let mut cmd = Command::new(&docker_bin);
    cmd.arg("run");

    let runner_name = format!(
        "boxr-runner-{}",
        bundle_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("run")
    );
    cmd.arg("--name").arg(&runner_name);

    if detach {
        cmd.arg("-d");
    } else {
        cmd.arg("--rm").arg("-i");
    }

    cmd.arg("--privileged");
    cmd.arg("-v").arg(format!("{}:/boxr-rootfs", rootfs_str));

    // Bind mount volume specs into runner
    for (idx, m) in mounts.iter().enumerate() {
        let src = m.source.to_string_lossy();
        cmd.arg("-v").arg(format!(
            "{}:/boxr-mounts/m{}{}",
            src,
            idx,
            if m.read_only { ":ro" } else { "" }
        ));
    }

    // Port forwardings
    for p in ports {
        if let Some(ip) = &p.host_ip {
            cmd.arg("-p").arg(format!(
                "{}:{}:{}/{}",
                ip, p.host_port, p.container_port, p.protocol
            ));
        } else {
            cmd.arg("-p").arg(format!(
                "{}:{}/{}",
                p.host_port, p.container_port, p.protocol
            ));
        }
    }

    // Environment variables
    for env_var in &spec.process.env {
        cmd.arg("-e").arg(env_var);
    }

    if let Some(hostname) = &spec.hostname {
        cmd.arg("-h").arg(hostname);
    }

    cmd.arg("alpine").arg("/bin/sh").arg("-c").arg(shell_script);

    let log_path = bundle_path.join("logs.txt");

    if detach {
        let output = cmd.output()?;
        let mut log_file = File::create(log_path)?;
        log_file.write_all(&output.stdout)?;
        log_file.write_all(&output.stderr)?;
        return Ok(if output.status.success() { 0 } else { 1 });
    }

    if spec.root.readonly {
        let status = cmd.status()?;
        return Ok(status.code().unwrap_or(0));
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut log_file = File::create(log_path)?;

    if let Some(out) = stdout {
        let reader = BufReader::new(out);
        for line in reader.lines() {
            if let Ok(l) = line {
                println!("{}", l);
                let _ = writeln!(log_file, "{}", l);
            }
        }
    }

    if let Some(err) = stderr {
        let reader = BufReader::new(err);
        for line in reader.lines() {
            if let Ok(l) = line {
                eprintln!("{}", l);
                let _ = writeln!(log_file, "{}", l);
            }
        }
    }

    let status = child.wait()?;
    Ok(status.code().unwrap_or(0))
}

/// Execute a command in an existing container bundle
pub fn exec_in_bundle(bundle_path: &Path, command: &[String], env: &[String]) -> Result<i32> {
    let docker_bin = find_real_docker_bin();
    let runner_name = format!(
        "boxr-runner-{}",
        bundle_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("run")
    );
    let binary = &command[0];
    let args = &command[1..];

    // Try executing directly in running container runner first
    let rootfs_path = bundle_path.join("rootfs");
    let has_sh = rootfs_path.join("bin/sh").exists();

    let mut check_cmd = Command::new(&docker_bin);
    check_cmd.args(["ps", "-q", "-f", &format!("name={}", runner_name)]);
    if let Ok(output) = check_cmd.output() {
        if !output.stdout.is_empty() {
            let mut exec_cmd = Command::new(&docker_bin);
            exec_cmd.args(["exec", "-i"]);
            for e in env {
                exec_cmd.arg("-e").arg(e);
            }
            exec_cmd.arg(&runner_name);
            if has_sh {
                let mut inner = format!("exec {}", binary);
                for a in args {
                    inner.push_str(&format!(
                        " \\\"{}\\\"",
                        a.replace('\\', "\\\\").replace('"', "\\\"")
                    ));
                }
                exec_cmd.args(["chroot", "/boxr-rootfs", "/bin/sh", "-c", &inner]);
            } else {
                exec_cmd.arg("chroot").arg("/boxr-rootfs").arg(binary);
                for a in args {
                    exec_cmd.arg(a);
                }
            }
            if let Ok(status) = exec_cmd.status() {
                return Ok(status.code().unwrap_or(0));
            }
        }
    }

    // Fallback: spawn standalone runner
    let abs_rootfs = rootfs_path.canonicalize()?;
    let rootfs_str = abs_rootfs.to_str().ok_or_else(|| anyhow!("Invalid path"))?;

    let shell_script = if has_sh {
        let mut inner = format!("exec {}", binary);
        for a in args {
            inner.push_str(&format!(
                " \\\"{}\\\"",
                a.replace('\\', "\\\\").replace('"', "\\\"")
            ));
        }
        format!(
            "mkdir -p /boxr-rootfs/proc /boxr-rootfs/dev; mount -t proc proc /boxr-rootfs/proc 2>/dev/null || true; mount --bind /dev /boxr-rootfs/dev 2>/dev/null || true; chroot /boxr-rootfs /bin/sh -c \"{}\"",
            inner
        )
    } else {
        let mut inner = format!("chroot /boxr-rootfs {}", binary);
        for a in args {
            inner.push_str(&format!(" \"{}\"", a.replace('"', "\\\"")));
        }
        inner
    };

    let mut cmd = Command::new(&docker_bin);
    cmd.arg("run")
        .arg("--rm")
        .arg("-i")
        .arg("--privileged")
        .arg("-v")
        .arg(format!("{}:/boxr-rootfs", rootfs_str));

    for e in env {
        cmd.arg("-e").arg(e);
    }

    cmd.arg("alpine").arg("/bin/sh").arg("-c").arg(shell_script);
    let status = cmd.status()?;
    Ok(status.code().unwrap_or(0))
}
