use crate::network::PortMapping;
use crate::oci::runtime::Spec;
use crate::volume::MountSpec;
use anyhow::{Context, Result, anyhow};
use nix::mount::{MsFlags, mount};
use nix::sched::{CloneFlags, unshare};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, chdir, fork, pivot_root};
use std::ffi::CString;
use std::fs;
use std::path::Path;

/// Native Linux execution of an OCI bundle using Linux namespaces and pivot_root.
pub fn execute_bundle(
    bundle_path: &Path,
    _spec: &Spec,
    mounts: &[MountSpec],
    _ports: &[PortMapping],
    detach: bool,
) -> Result<i32> {
    if !mounts.is_empty() {
        let mounts_json = serde_json::to_string(mounts)?;
        let _ = fs::write(bundle_path.join("mounts.json"), mounts_json);
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("boxr"));

    if detach {
        let log_file = fs::File::create(bundle_path.join("logs.txt"))?;
        let err_file = log_file.try_clone()?;
        let child = std::process::Command::new(&exe)
            .arg("__internal-trampoline")
            .arg(bundle_path)
            .stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(err_file)
            .spawn()
            .context("Failed to spawn container trampoline process")?;

        let _ = fs::write(bundle_path.join("pid"), child.id().to_string());
        Ok(0)
    } else {
        let status = std::process::Command::new(&exe)
            .arg("__internal-trampoline")
            .arg(bundle_path)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .context("Failed to execute container trampoline process")?;

        Ok(status.code().unwrap_or(1))
    }
}

/// Single-threaded trampoline entry point invoked before Tokio runtime initialization.
/// Solves Linux kernel EINVAL when calling unshare(CLONE_NEWUSER) in multi-threaded processes.
pub fn run_trampoline(args: &[String]) -> Result<i32> {
    if args.is_empty() {
        return Err(anyhow!("No bundle path specified for trampoline"));
    }
    let bundle_path = Path::new(&args[0]);
    let spec_path = bundle_path.join("config.json");
    if !spec_path.exists() {
        return Err(anyhow!("config.json not found in bundle {:?}", bundle_path));
    }
    let spec_content = fs::read_to_string(&spec_path)?;
    let spec: Spec = serde_json::from_str(&spec_content)?;

    let mounts: Vec<MountSpec> = if bundle_path.join("mounts.json").exists() {
        let content = fs::read_to_string(bundle_path.join("mounts.json"))?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    let raw_rootfs = std::path::PathBuf::from(&spec.root.path);
    let rootfs = if raw_rootfs.is_absolute() {
        raw_rootfs
    } else {
        bundle_path.join(&spec.root.path)
    };

    if !rootfs.exists() {
        return Err(anyhow!("Rootfs does not exist at {:?}", rootfs));
    }

    let abs_rootfs = rootfs.canonicalize()?;

    // Check if we are non-root (unprivileged rootless mode)
    let is_rootless = unsafe { libc::getuid() != 0 };

    if is_rootless {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let (mut parent_sock, mut child_sock) = UnixStream::pair()
            .context("Failed to create UnixStream pair for user namespace sync")?;

        match unsafe { fork() }? {
            ForkResult::Parent { child } => {
                drop(child_sock);
                // 1. Wait for child to unshare user namespace
                let mut sync_buf = [0u8; 5];
                parent_sock
                    .read_exact(&mut sync_buf)
                    .context("Failed to read sync from container child (unshare failed)")?;

                // 2. Configure UID and GID mapping for child PID in parent namespace
                crate::security::RootlessUserConfig::setup_child_mappings(child.as_raw())
                    .context("Failed to setup rootless UID/GID mappings")?;

                // 3. Notify child that mapping is complete
                parent_sock
                    .write_all(b"done")
                    .context("Failed to write sync to container child")?;
                drop(parent_sock);

                // 4. Wait for child container process
                match waitpid(child, None)? {
                    WaitStatus::Exited(_, code) => Ok(code),
                    WaitStatus::Signaled(_, sig, _) => Ok(128 + sig as i32),
                    _ => Ok(1),
                }
            }
            ForkResult::Child => {
                drop(parent_sock);
                // 1. Unshare user namespace cleanly in single-threaded child
                if let Err(e) = unshare(CloneFlags::CLONE_NEWUSER) {
                    eprintln!("Failed to unshare user namespace: {:?}", e);
                    std::process::exit(1);
                }

                // 2. Notify parent that user namespace has been created
                if let Err(e) = child_sock.write_all(b"ready") {
                    eprintln!("Failed to notify parent: {:?}", e);
                    std::process::exit(1);
                }

                // 3. Wait for parent to write UID/GID mappings
                let mut ready_buf = [0u8; 4];
                if let Err(e) = child_sock.read_exact(&mut ready_buf) {
                    eprintln!("Failed to read mapping confirmation: {:?}", e);
                    std::process::exit(1);
                }
                drop(child_sock);

                // 4. Child is now root in user namespace. Unshare remaining namespaces
                let flags = CloneFlags::CLONE_NEWPID
                    | CloneFlags::CLONE_NEWNS
                    | CloneFlags::CLONE_NEWUTS
                    | CloneFlags::CLONE_NEWIPC;

                if let Err(e) = unshare(flags) {
                    eprintln!("Failed to unshare container namespaces: {:?}", e);
                    std::process::exit(1);
                }

                // 5. Fork so grandchild becomes PID 1 inside new PID namespace
                match unsafe { fork() } {
                    Ok(ForkResult::Parent { child: grandchild }) => {
                        match waitpid(grandchild, None) {
                            Ok(WaitStatus::Exited(_, code)) => std::process::exit(code),
                            Ok(WaitStatus::Signaled(_, sig, _)) => {
                                std::process::exit(128 + sig as i32)
                            }
                            _ => std::process::exit(1),
                        }
                    }
                    Ok(ForkResult::Child) => {
                        if let Err(err) = run_container_child(&abs_rootfs, &spec, &mounts) {
                            eprintln!("Container child failed: {:?}", err);
                            std::process::exit(1);
                        }
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("Fork failed: {:?}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    } else {
        // Unshare remaining namespaces: PID, Mount, UTS, IPC
        let flags = CloneFlags::CLONE_NEWPID
            | CloneFlags::CLONE_NEWNS
            | CloneFlags::CLONE_NEWUTS
            | CloneFlags::CLONE_NEWIPC;

        unshare(flags).context("Failed to unshare namespaces (requires root or CAP_SYS_ADMIN)")?;

        // Fork: child will become PID 1 inside the new PID namespace
        match unsafe { fork() }? {
            ForkResult::Parent { child } => match waitpid(child, None)? {
                WaitStatus::Exited(_, code) => Ok(code),
                WaitStatus::Signaled(_, sig, _) => Ok(128 + sig as i32),
                _ => Ok(1),
            },
            ForkResult::Child => {
                if let Err(err) = run_container_child(&abs_rootfs, &spec, &mounts) {
                    eprintln!("Container child failed: {:?}", err);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
        }
    }
}

/// Run an unshare command in a new user namespace (boxr unshare)
pub fn run_unshare_cli(args: &[String]) -> Result<i32> {
    let default_cmd = vec![
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
    ];
    let cmd = if args.is_empty() {
        &default_cmd
    } else {
        args
    };

    let is_rootless = unsafe { libc::getuid() != 0 };
    if is_rootless {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let (mut parent_sock, mut child_sock) = UnixStream::pair()?;
        match unsafe { fork() }? {
            ForkResult::Parent { child } => {
                drop(child_sock);
                let mut buf = [0u8; 5];
                parent_sock.read_exact(&mut buf)?;
                crate::security::RootlessUserConfig::setup_child_mappings(child.as_raw())?;
                parent_sock.write_all(b"done")?;
                drop(parent_sock);

                match waitpid(child, None)? {
                    WaitStatus::Exited(_, code) => Ok(code),
                    WaitStatus::Signaled(_, sig, _) => Ok(128 + sig as i32),
                    _ => Ok(1),
                }
            }
            ForkResult::Child => {
                drop(parent_sock);
                unshare(CloneFlags::CLONE_NEWUSER)?;
                child_sock.write_all(b"ready")?;
                let mut buf = [0u8; 4];
                child_sock.read_exact(&mut buf)?;
                drop(child_sock);

                let binary_c = CString::new(cmd[0].as_str())?;
                let args_c: Vec<CString> = cmd
                    .iter()
                    .map(|s| CString::new(s.as_str()).unwrap())
                    .collect();
                nix::unistd::execvp(&binary_c, &args_c)?;
                std::process::exit(0);
            }
        }
    } else {
        let mut child = std::process::Command::new(&cmd[0]);
        if cmd.len() > 1 {
            child.args(&cmd[1..]);
        }
        let status = child.status()?;
        Ok(status.code().unwrap_or(0))
    }
}

pub fn run_trampoline_exec(args: &[String]) -> Result<i32> {
    if args.is_empty() {
        return Err(anyhow!("Missing bundle path for exec"));
    }
    let bundle_path = Path::new(&args[0]);
    let cmd = &args[1..];
    exec_in_bundle(bundle_path, cmd, &[])
}

pub fn exec_in_bundle(bundle_path: &Path, command: &[String], env: &[String]) -> Result<i32> {
    let rootfs = bundle_path.join("rootfs");
    let abs_rootfs = rootfs.canonicalize()?;

    let flags = CloneFlags::CLONE_NEWNS;
    let _ = unshare(flags);

    let binary = &command[0];
    let binary_c = CString::new(binary.as_str())?;
    let args_c: Vec<CString> = command
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();

    for e in env {
        if let Some((k, v)) = e.split_once('=') {
            unsafe {
                std::env::set_var(k, v);
            }
        }
    }

    let _ = nix::unistd::chroot(&abs_rootfs);
    let _ = chdir("/");
    let _ = nix::unistd::execvp(&binary_c, &args_c);
    Ok(0)
}

fn run_container_child(rootfs: &Path, spec: &Spec, mounts: &[MountSpec]) -> Result<()> {
    // Set hostname
    if let Some(hostname) = &spec.hostname {
        let host_c = CString::new(hostname.as_str())?;
        unsafe {
            libc::sethostname(host_c.as_ptr(), host_c.as_bytes().len());
        }
    }

    // Ensure root filesystem is private
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;

    // Bind mount rootfs to itself so it is an active mountpoint (requirement for pivot_root)
    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )?;

    // Mount proc, sysfs, dev inside rootfs
    let proc_path = rootfs.join("proc");
    let _ = fs::create_dir_all(&proc_path);
    let _ = mount(
        Some("proc"),
        &proc_path,
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    );

    let sys_path = rootfs.join("sys");
    let _ = fs::create_dir_all(&sys_path);
    if mount(
        Some("sysfs"),
        &sys_path,
        Some("sysfs"),
        MsFlags::MS_RDONLY,
        None::<&str>,
    )
    .is_err()
    {
        // In restricted unprivileged user namespaces, kernel disallows mounting new sysfs
        // Fallback: mount tmpfs on /sys
        let _ = mount(
            Some("tmpfs"),
            &sys_path,
            Some("tmpfs"),
            MsFlags::MS_RDONLY,
            Some("mode=755"),
        );
    }

    let dev_path = rootfs.join("dev");
    let _ = fs::create_dir_all(&dev_path);
    let _ = mount(
        Some("tmpfs"),
        &dev_path,
        Some("tmpfs"),
        MsFlags::MS_NOSUID | MsFlags::MS_STRICTATIME,
        Some("mode=755"),
    );

    // Mount external volumes/binds
    for m in mounts {
        let target = rootfs.join(m.destination.trim_start_matches('/'));
        let _ = fs::create_dir_all(&target);
        let mut flags = MsFlags::MS_BIND | MsFlags::MS_REC;
        if m.read_only {
            flags |= MsFlags::MS_RDONLY;
        }
        let _ = mount(Some(&m.source), &target, None::<&str>, flags, None::<&str>);
    }

    // Setup pivot_root
    let oldroot_path = rootfs.join(".oldroot");
    let _ = fs::create_dir_all(&oldroot_path);

    if let Err(_e) = pivot_root(rootfs, &oldroot_path) {
        // Fallback to chroot if pivot_root is unsupported
        nix::unistd::chroot(rootfs)?;
        chdir("/")?;
    } else {
        chdir("/")?;
        let _ = nix::mount::umount2("/.oldroot", nix::mount::MntFlags::MNT_DETACH);
        let _ = fs::remove_dir("/.oldroot");
    }

    // Change to requested working directory
    let cwd = if spec.process.cwd.is_empty() {
        Path::new("/")
    } else {
        Path::new(&spec.process.cwd)
    };
    let _ = chdir(cwd);

    // Clear environment and populate with spec environment
    for (k, _) in std::env::vars() {
        unsafe {
            std::env::remove_var(k);
        }
    }
    let mut has_path = false;
    for e in &spec.process.env {
        if let Some((k, v)) = e.split_once('=') {
            if k == "PATH" {
                has_path = true;
            }
            unsafe {
                std::env::set_var(k, v);
            }
        }
    }
    if !has_path {
        unsafe {
            std::env::set_var(
                "PATH",
                "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            );
        }
    }

    // Execute container binary
    let binary = &spec.process.args[0];
    let binary_c = CString::new(binary.as_str())?;
    let args_c: Vec<CString> = spec
        .process
        .args
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();

    nix::unistd::execvp(&binary_c, &args_c)?;
    Ok(())
}
