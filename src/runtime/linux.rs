use crate::oci::runtime::Spec;
use anyhow::{anyhow, Context, Result};
use nix::mount::{mount, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{chdir, fork, pivot_root, sethostname, ForkResult};
use std::ffi::CString;
use std::fs;
use std::path::Path;

/// Native Linux execution of an OCI bundle using Linux namespaces and pivot_root.
pub fn execute_bundle(bundle_path: &Path, spec: &Spec) -> Result<i32> {
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
        ForkResult::Parent { child } => {
            match waitpid(child, None)? {
                WaitStatus::Exited(_, code) => Ok(code),
                WaitStatus::Signaled(_, sig, _) => Ok(128 + sig as i32),
                _ => Ok(1),
            }
        }
        ForkResult::Child => {
            if let Err(err) = run_container_child(&abs_rootfs, spec) {
                eprintln!("Container child failed: {:?}", err);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
    }
}

fn run_container_child(rootfs: &Path, spec: &Spec) -> Result<()> {
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
