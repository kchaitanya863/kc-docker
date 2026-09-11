use crate::network::PortMapping;
use crate::oci::runtime::Spec;
use crate::volume::MountSpec;
use anyhow::{Context, Result, anyhow};
use nix::mount::{MsFlags, mount};
use nix::sched::{CloneFlags, unshare};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, chdir, fork, pivot_root, sethostname};
use std::ffi::CString;
use std::fs;
use std::path::Path;

/// Native Linux execution of an OCI bundle using Linux namespaces and pivot_root.
pub fn execute_bundle(
    bundle_path: &Path,
    spec: &Spec,
    mounts: &[MountSpec],
    _ports: &[PortMapping],
    _detach: bool,
) -> Result<i32> {
    let rootfs = bundle_path.join(&spec.root.path);
    if !rootfs.exists() {
        return Err(anyhow!("Rootfs does not exist at {:?}", rootfs));
    }

    let abs_rootfs = rootfs.canonicalize()?;

    // Unshare namespaces
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
            if let Err(err) = run_container_child(&abs_rootfs, spec, mounts) {
                eprintln!("Container child failed: {:?}", err);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
    }
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
            std::env::set_var(k, v);
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
        sethostname(hostname)?;
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
    let _ = mount(
        Some("sysfs"),
        &sys_path,
        Some("sysfs"),
        MsFlags::MS_RDONLY,
        None::<&str>,
    );

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
    let _ = chdir(Path::new(&spec.process.cwd));

    // Clear environment and populate with spec environment
    for (k, _) in std::env::vars() {
        std::env::remove_var(k);
    }
    for e in &spec.process.env {
        if let Some((k, v)) = e.split_once('=') {
            std::env::set_var(k, v);
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
