# Enterprise Container Runtimes & Hardening Guide 🏢

Production reference for running Fortune 15 enterprise workloads with `boxr`. This guide covers rootless runtime internals, multi-user privilege dropping, POSIX shared memory, filesystem sticky bits, and compliance hardening.

---

## 1. Privilege-Dropping & Service Users

### The Enterprise Service User Pattern
Production containers rarely run as `root` (UID 0) throughout their entire lifecycle. Enterprise software typically uses one of two privilege models:
1. **Startup as root, drop to service user**: The process initializes as root (to open low ports, configure storage, or read keyrings), then drops privileges to a dedicated service account via `setresuid()` / `setresgid()`.
   - **`apt-get` / `dpkg`**: Starts as root, drops privileges to `_apt` (UID 42, GID 65534) to run sandboxed workers and OpenPGP signature verification (`gpgv`).
   - **`nginx`**: Master process runs as root; worker processes drop to `nginx` / `www-data` (UID 33 or 101).
   - **`postgres`**: Explicitly refuses to start as root (`initdb: cannot be run as root`) and mandates running as `postgres` (UID 999 or 70).
2. **Fixed Non-Root UID**: Containers deployed under Kubernetes/OpenShift with `runAsNonRoot: true`, `runAsUser: 10001`, or `securityContext.allowPrivilegeEscalation: false`.

### The Rootless Umask Trap & GPG Signature Failure
In a rootless container engine, images are extracted by an unprivileged host user (e.g., UID 1000). Standard extraction applies the host process's `umask` (commonly `0022` or `0002`).

#### The Failure Chain
1. In the upstream OCI image, `/tmp` and `/var/tmp` are defined with mode `1777` (world-writable with sticky bit `t`).
2. When extracted under host `umask 0022`, the world-writable bit (`o+w`) is stripped:
   $$\text{Mode: } 01777 \;\&\; \sim 00022 = 01755 \; (\text{rwxr-xr-x})$$
3. On disk, `/tmp` is owned by host UID 1000 (which maps to container root, UID 0).
4. When `apt-get update` runs, it launches `/usr/lib/apt/methods/gpgv` as user `_apt` (UID 42).
5. `gpgv` calls `GetTempFile("apt.XXXXXX.gpg")` which invokes `mkstemp("/tmp/apt.XXXXXX.gpg")`.
6. Because `/tmp` is `0755 root:root`, user `_apt` receives `EACCES (Permission denied)`.
7. `apt`'s verification helper immediately aborts with internal error `111` (`#define EINTERNAL 111`).
8. The error surfaces to users as:
   ```text
   Err:1 http://archive.ubuntu.com/ubuntu resolute InRelease
     Could not execute 'gpgv' to verify signature (is gnupg installed?)
   E: The repository 'http://archive.ubuntu.com/ubuntu resolute InRelease' is not signed.
   ```

#### The Boxr Solution
Boxr enforces three layers of defense to eliminate this failure class:
1. **Runtime Startup Invariant (`src/runtime/linux.rs` & `src/runtime/darwin.rs`)**:
   Before container process execution, Boxr explicitly guarantees that `/tmp` and `/var/tmp` exist with sticky bit `0o1777` permissions:
   ```rust
   let tmp_path = rootfs.join("tmp");
   let _ = fs::create_dir_all(&tmp_path);
   let _ = fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o1777));
   ```
2. **Tar Header Mode Restoration (`src/oci/image.rs`)**:
   During image layer extraction, Boxr preserves the exact tar entry mode from the OCI layer archive, preventing host `umask` from stripping sticky or world-writable bits.
3. **Subordinate UID/GID Delegation (`src/security/mod.rs`)**:
   Boxr queries `/etc/subuid` and `/etc/subgid` and programs 65,536 contiguous mapped user IDs via `newuidmap` / `newgidmap`, ensuring service accounts like `_apt` (UID 42), `www-data` (UID 33), and `nobody` (UID 65534) map to valid host subordinate ranges.

---

## 2. POSIX Shared Memory (`/dev/shm`)

Enterprise workloads across AI/ML, browser automation, and high-throughput databases depend on POSIX shared memory:

| Workload | Subsystem Dependency | Consequence if `/dev/shm` is Missing |
| :--- | :--- | :--- |
| **PyTorch / TensorFlow** | `torch.utils.data.DataLoader(num_workers > 0)` | `RuntimeError: unable to write to /dev/shm` |
| **Headless Chromium** | Playwright / Puppeteer E2E tests | Immediate `Bus error` or crash on browser launch |
| **PostgreSQL** | `shared_buffers` IPC buffer pool | `FATAL: could not create shared memory segment` |
| **Apache Kafka / Java JVM** | High-performance off-heap queues | Fallback to slow disk spooling or OOM |

### Boxr Implementation
In Boxr, `/dev/shm` is mounted inside the container rootfs as an isolated, dedicated `tmpfs` with mode `1777` and standard default sizing (64MB or custom `--shm-size`):
```rust
let shm_path = dev_path.join("shm");
let _ = fs::create_dir_all(&shm_path);
let _ = mount(
    Some("tmpfs"),
    &shm_path,
    Some("tmpfs"),
    MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
    Some("mode=1777,size=67108864"),
);
```

To configure custom shared memory sizing:
```bash
boxr run --shm-size 2g --rm pytorch/pytorch:latest python train.py
```

---

## 3. Standard Pseudo-Filesystems & Essential Devices

Enterprise microservices running as non-root users (`--user 10001:10001`) expect standard Unix character devices to be present and writable:

### Device Matrix
| Device | Required Mode | Purpose |
| :--- | :--- | :--- |
| `/dev/null` | `0666` | Discarding output (`> /dev/null 2>&1`) |
| `/dev/zero` | `0666` | Zero-initialized memory mapping |
| `/dev/full` | `0666` | Testing out-of-disk-space error paths |
| `/dev/random` | `0666` | Cryptographic entropy source |
| `/dev/urandom` | `0666` | Non-blocking cryptographic entropy source |
| `/dev/tty` | `0666` | Process controlling terminal |
| `/dev/pts/` | `0620` (`devpts`) | Pseudo-terminal allocation for interactive exec/SSH |

### Standard Symlinks
Boxr provisions standard POSIX symlinks in `/dev`:
- `/dev/fd` -> `/proc/self/fd`
- `/dev/stdin` -> `/proc/self/fd/0`
- `/dev/stdout` -> `/proc/self/fd/1`
- `/dev/stderr` -> `/proc/self/fd/2`
- `/dev/ptmx` -> `pts/ptmx`

---

## 4. Immutable Infrastructure: Read-Only Root Filesystem

Enterprise security benchmarks (CIS Docker Benchmark Section 5.12, PCI-DSS Requirement 2.2, SOC 2 Type II) recommend running containers with read-only root filesystems to prevent tampering and runtime persistence of malware.

### Running with `--read-only`
```bash
boxr run --rm \
  --read-only \
  --tmpfs /tmp:rw,noexec,nosuid \
  --tmpfs /var/run:rw,noexec,nosuid \
  -v /var/log/my-app:/var/log/my-app:rw \
  my-app:production
```

### Execution Behavior
1. Rootfs (`/`) is mounted with `MS_RDONLY | MS_REMOUNT | MS_BIND`.
2. Any write to `/etc`, `/bin`, or rootfs fails immediately with `EROFS (Read-only file system)`.
3. Designated ephemeral paths (`/tmp`, `/var/run`) use volatile tmpfs mounts.
4. Persistent state writes exclusively to explicitly mounted volumes (`-v`).

---

## 5. Zombie Process Reaping & Init Process (`--init`)

### The PID 1 Problem
In Unix, when a child process terminates, it enters a `zombie` (defunct) state until its parent calls `waitpid()` to read its exit status.
- If a parent process dies before its child, the child is re-parented to **PID 1**.
- If PID 1 in a container is a standard application (such as a Node.js script, Python script, or Java application) that does not implement a `waitpid()` reap loop, orphaned zombie processes accumulate indefinitely.
- Once the kernel PID limit (`--pids-limit`) or system PID limit is reached, the container halts and cannot fork new processes.

### Boxr Solution
Boxr supports the `--init` flag, which places an integrated, lightweight signal-forwarding and zombie-reaping init process at PID 1:

```bash
boxr run --init --pids-limit 200 --rm my-multi-process-app
```

```
Container PID Hierarchy with --init:
PID 1: [boxr-init] (Handles SIGCHLD, reaps orphans, forwards SIGTERM)
  └── PID 2: [node server.js] (Application process)
        ├── PID 3: (worker child 1)
        └── PID 4: (worker child 2)
```

---

## 6. Resource Limits & Cgroup v2 Isolation

Boxr interfaces directly with the Linux **cgroupfs v2** unified hierarchy (`/sys/fs/cgroup/`):

### Memory Constraints
```bash
boxr run -m 512m --memory-swap 1g my-app
```
- **Controller**: `memory.max`, `memory.swap.max`
- **JVM Ergonomics**: Modern runtimes (OpenJDK 17+, Go 1.19+, Node 18+) read `memory.max` to automatically set default heap ceilings (`-XX:MaxRAMPercentage`) without requiring hardcoded flags.
- **OOM Kill Handling**: When memory exceeds `memory.max`, the kernel cgroup OOM killer terminates the offending process and records exit code `137` (`128 + SIGKILL`).

### CPU Constraints
```bash
boxr run --cpus 2.5 my-app
```
- **Controller**: `cpu.max` (quota and period)
- A setting of `2.5` sets `cpu.max` to `250000 100000` (allowing 250ms of CPU runtime per 100ms wall-clock period).

### Task Limit (Fork Bomb Protection)
```bash
boxr run --pids-limit 100 my-app
```
- **Controller**: `pids.max`
- Restricts total active tasks (threads + processes) within the cgroup, shielding the host from runaway process spawning.

---

## 7. Enterprise Test Verification Suite

All scenarios documented here are continuously verified via the automated test suite in `tests/enterprise_scenarios_test.rs`:

```bash
cargo test --test enterprise_scenarios_test -- --test-threads=1
```

### Verified Test Matrix
1. `test_enterprise_unprivileged_tmp_and_var_tmp_write`: Non-root (`nobody`, `uid 10001`) write to `/tmp` with `1777` sticky bit.
2. `test_enterprise_posix_shared_memory_dev_shm`: Availability of `/dev/shm` tmpfs for IPC and ML dataloaders.
3. `test_enterprise_device_nodes_permissions`: Access to `/dev/null`, `/dev/zero`, and `/dev/urandom` under non-root UIDs.
4. `test_enterprise_package_manager_priv_drop`: Privilege dropping across package managers (`apk`, `apt-get`).
5. `test_enterprise_process_init_zombie_reaping`: PID 1 orphan reaping with `--init`.
6. `test_enterprise_resource_limits_enforcement`: Enforcement of `--memory` and `--cpus` cgroup v2 controllers.
7. `test_enterprise_readonly_rootfs_with_tmpfs`: Immutable rootfs (`--read-only`) with writable bind mounts.
8. `test_enterprise_env_hygiene`: Confidential environment variable and token isolation.
