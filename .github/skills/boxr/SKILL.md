---
name: boxr
description: Complete knowledge base, architecture, runtime internals, guardrails, and distribution workflows for Boxr — the fast, rootless, zero-dependency OCI container engine in Rust.
---

# Boxr Container Engine Skill

Use this skill when developing, debugging, benchmarking, or packaging `boxr` (the Rust-based OCI container engine).

---

## 1. Runtime Execution Models

### Linux Execution (`src/runtime/linux.rs`)
- **100% Native Kernel Syscalls**: Operates directly with no daemon or hypervisor layer.
- **Rootless by Default**:
  - Unshares `CLONE_NEWUSER` first when running as an unprivileged user (`libc::getuid() != 0`).
  - Writes single-entry UID/GID mappings (`uid_map`, `gid_map`) mapping unprivileged user to container root (UID 0).
  - Unshares `CLONE_NEWNS`, `CLONE_NEWPID`, `CLONE_NEWIPC`, `CLONE_NEWUTS`.
  - Mounts container rootfs, mounts internal `/proc`, `/sys`, `/dev`.
  - Executes `pivot_root` and drops dangerous capabilities (`CAP_SYS_ADMIN`, `CAP_SYS_RAWIO`).
- **Resource Limits**: Linux cgroups v2 (`/sys/fs/cgroup/boxr/<id>/`) controlling `memory.max`, `cpu.max`, `pids.max`.

### macOS Execution (`src/runtime/darwin.rs` & `src/runtime/boxr-vz.m`)
- **Zero Docker Dependency**: Uses Apple's native `Virtualization.framework` (`VZVirtualMachine`).
- **Architecture**:
  - `boxr-vz`: High-performance Objective-C hypervisor runner (76KB), codesigned with `com.apple.security.virtualization`.
  - Kernel: Raw ARM64/x86_64 Image (`~/.boxr/vm/vmlinux`) and micro-initrd (`~/.boxr/vm/initrd.cpio.gz`).
  - Boot Time: ~120–150ms boot to user container execution.
  - Filesystem Sharing: Native Apple `virtiofs` (`VZVirtioFileSystemDeviceConfiguration`) with tag `boxr_rootfs`.
  - Entropy: `VZVirtioEntropyDeviceConfiguration` for instant, non-blocking `/dev/urandom` and `getrandom()`.
  - Network: `VZNATNetworkDeviceAttachment` + `ip link set lo up` for local host and socket communication.
- **Background Supervision & `boxr exec`**:
  - Background (`-d`) containers launch main process with `MAIN_PID=$!` while running a guest supervisor loop.
  - `boxr exec` writes unique `/boxr-exec-<id>.sh` with `0o777` permissions; the guest supervisor executes it and writes `/boxr-exec-<id>.done` and `/boxr-exec-<id>.log`.
  - Process exit codes are mirrored back to host via `<rootfs>/boxr-exitcode`.
  - Stdio redirection: Daemon processes redirect stdio to `Stdio::null()` so host CLI commands (`output()`) do not hang waiting for open pipe descriptors.

---

## 2. Multi-Architecture & OS Support

- **`linux/arm64` (`aarch64`)**: Native execution on Apple Silicon and ARM Linux.
- **`linux/amd64` (`x86_64`)**: Rosetta 2 translation via macOS hypervisor; static `musl` binaries on Linux.
- **Image Index Resolution**: `ImageStore::find_with_platform` caches images by both tag and architecture so multi-arch images do not overwrite each other.
- **Windows OCI Images**: Safely downloaded and extracted; informs user that Windows container execution requires a native Windows host.

---

## 3. Operational Guardrails (`src/guardrails/mod.rs`)

1. **Log Rotation (`LogRotator`)**:
   - Size-based log rotation (`max-size`, `max-files`) for container `logs.txt` and system `events.jsonl` (e.g. 5MB/10MB limits with backup shifting `.1`, `.2`, `.3`).
2. **Host Port Collision Guard (`PortCollisionGuard`)**:
   - Inspects existing running containers to prevent two containers from binding the same host IP/port and protocol.
3. **Circular Dependency Detection (`ComposeProject::dependency_order`)**:
   - Depth-first cycle detection in `docker-compose.yml` (`depends_on`), blocking circular service deadlock with descriptive error messages.
4. **Disk Space Protection (`DiskGuard`)**:
   - Proactive `statvfs` check (`f_bavail * f_frsize`) ensuring available space + 100MB margin before downloading layers.
5. **Orphan & Zombie Reaper (`ProcessReaper`)**:
   - Self-healing recovery checking `vm.pid` via `libc::kill(pid, 0) == 0`. Transitions abandoned containers to `Status::Exited(137)`.
6. **Graceful Stop Supervisor**:
   - Sends `SIGTERM` first, polls for clean exit up to timeout, then escalates to `SIGKILL` to prevent unkillable hanging processes.

---

## 4. Background Service Management (`src/service/mod.rs`)

- **`boxr daemon`**: Unix domain socket server (`~/.boxr/boxr.sock`) on Unix, TCP socket (`127.0.0.1:2375`) on Windows implementing Docker Engine REST API (`/_ping`, `/version`, `/info`, `/containers`, `/images`).
- **Autostart on Login / Boot**:
  - **macOS**: `~/Library/LaunchAgents/com.boxr.daemon.plist` managed via `launchctl` and `boxr service [install|start|stop|status|uninstall]`.
  - **Linux**: `~/.config/systemd/user/boxr.service` managed via `systemctl --user`.
  - **Windows**: Windows Service via `sc.exe create boxr` or `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
  - **Homebrew Services**: `brew services start boxr` via native `service do ... end` block.

---

## 5. Release & Packaging Pipeline

- **Dual-Repository Architecture**:
  - **Source Repo**: `kchaitanya863/kc-docker` (Private — source code, CI workflow, internal tests).
  - **Distribution Tap**: `kchaitanya863/homebrew-tap` (Public — formulae, precompiled release archives, SHA-256 hashes).
- **Automated CI/CD Workflow (`.github/workflows/ci.yml`)**:
  - Builds static `musl` binaries for Linux (`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`) with `rustls-tls` to avoid glibc version mismatches across Ubuntu/Debian/Alpine/Fedora.
  - Builds universal binaries for macOS (`aarch64-apple-darwin`, `x86_64-apple-darwin`).
  - Job `update-brew-tap` automatically pushes release assets to `kchaitanya863/homebrew-tap` and bumps `Formula/boxr.rb`.
- **User Installation**:
  ```bash
  brew tap kchaitanya863/tap
  brew install boxr
  brew services start boxr
  ```
