# Architecture & Internals: `boxr` 📦

`boxr` is an Open Container Initiative (OCI) compliant container runtime and engine built entirely in Rust. It provides full Docker and Podman CLI parity with rootless-by-default execution, sub-second container spin-up, and zero-daemon footprint.

---

## 1. High-Level Design Principles

- **Zero Unnecessary Daemons**: Unlike Docker's client-server architecture (`docker` -> `dockerd` -> `containerd` -> `containerd-shim` -> `runc`), `boxr` operates primarily as a direct, in-process container engine.
- **Rootless by Default**: Containers execute using Linux unprivileged user namespaces (`CLONE_NEWUSER`) mapping host user IDs without requiring `sudo` or setuid binaries.
- **Copy-on-Write Layering**: Layers are mounted or cloned using kernel OverlayFS on Linux, or native Apple APFS `clonefile` copy-on-write on macOS, minimizing disk consumption and startup latency.
- **Full Specification Conformance**:
  - [OCI Image Format Specification v1.0.2](https://github.com/opencontainers/image-spec)
  - [OCI Runtime Specification v1.0.2](https://github.com/opencontainers/runtime-spec)
  - [OCI Distribution Specification v1.0.1](https://github.com/opencontainers/distribution-spec)

---

## 2. Core Subsystems

```
+-------------------------------------------------------------------------+
|                                 boxr CLI                                |
|  run, pull, build, compose, volume, network, pod, play, stats, events    |
+-------------------------------------------------------------------------+
       |                         |                        |
       v                         v                        v
+---------------+      +-------------------+      +-----------------------+
|  OCI Engine   |      |  Storage Layer    |      |  Security & Isolation |
| - Registry v2 |      | - ImageStore      |      | - Rootless UserNS     |
| - Layer Tar   |      | - ContainerStore  |      | - Seccomp Filter      |
| - Manifest    |      | - Overlay / CoW   |      | - Capabilities Whitelist
| - RuntimeSpec |      | - VolumeStore     |      | - cgroups v2 Limits   |
+---------------+      +-------------------+      +-----------------------+
                                 |
                                 v
+-------------------------------------------------------------------------+
|                           Execution Runtime                             |
|  - Linux: In-process unshare(), pivot_root(), mount(), execvp()         |
|  - macOS: Hybrid Darwin container execution bridge via VM runner        |
+-------------------------------------------------------------------------+
```

### 2.1. OCI Distribution Spec Client (`src/oci/distribution.rs`)
- Handles Registry V2 HTTP API interactions (Docker Hub, GitHub Packages `ghcr.io`, Quay.io).
- Negotiates multi-platform Image Index / Manifest Lists (`application/vnd.oci.image.index.v1+json`) and resolves to host CPU architecture (`arm64`, `amd64`).
- Implements Bearer Token authentication via registry challenge mechanisms (`Www-Authenticate`).
- Verifies SHA-256 digests for all layer tarballs during download.

### 2.2. Image & Layer Engine (`src/oci/image.rs`, `src/storage/overlay.rs`)
- Unpacks layered `.tar` / `.tar.gz` filesystem archives into assembled rootfs trees.
- Correctly interprets OCI whiteout markers:
  - Explicit whiteouts (`.wh.<name>`) indicate file deletion in preceding layers.
  - Opaque directory whiteouts (`.wh..wh..opq`) indicate parent directory contents are hidden.
- Employs copy-on-write storage drivers:
  - **Linux**: Kernel `overlay` filesystem with `lowerdir`, `upperdir`, `workdir`, and `merged` mount points.
  - **macOS**: Native APFS `clonefile(2)` system call for instant copy-on-write clones.

### 2.3. Runtime Spec & Sandboxing (`src/oci/runtime.rs`, `src/security/mod.rs`)
- Synthesizes standardized OCI `config.json` specifications for container bundles.
- Mounts standard pseudo-filesystems: `/proc` (procfs), `/dev` (tmpfs), `/dev/pts` (devpts), `/dev/shm` (tmpfs), and `/sys` (read-only sysfs).
- Drops sensitive capabilities (`CAP_SYS_ADMIN`, `CAP_SYS_RAWIO`) while retaining container safe defaults (`CAP_CHOWN`, `CAP_NET_BIND_SERVICE`).
- Enforces default Seccomp BPF filters blocking high-risk syscalls (`reboot`, `swapon`, `kexec_load`, `ptrace`, `bpf`).

### 2.4. Resource Management (`src/cgroups/mod.rs`)
- Integrates directly with Linux cgroups v2 hierarchy (`/sys/fs/cgroup`).
- Sets hard limits for:
  - Memory: `memory.max`
  - CPU: `cpu.max` (quota and period)
  - Processes: `pids.max`
  - Freezer: `cgroup.freeze` for instant pause/unpause without killing processes.

### 2.5. Networking & IPAM (`src/network/mod.rs`, `src/network/rootless.rs`)
- Manages software bridge networks with sequential IPAM address allocation.
- Implements user-space TCP/UDP rootless port forwarders without requiring root privileges.
- Injects synthetic `/etc/hosts` mappings for container-to-container DNS resolution.

### 2.6. Dockerfile Builder (`src/builder/mod.rs`)
- Full parser for Dockerfiles supporting `FROM`, `RUN`, `COPY`, `ADD`, `WORKDIR`, `ENV`, `CMD`, `ENTRYPOINT`, `EXPOSE`, `LABEL`, and `HEALTHCHECK`.
- Supports multi-stage builds (`FROM ... AS builder` and `COPY --from=builder ...`).
- Deterministic content-addressed step caching via SHA-256 step hashes.
- `.dockerignore` file evaluation with wildcard globbing and exception negations (`!pattern`).

### 2.7. Multi-Container Orchestration (`src/compose/mod.rs`)
- Parses `docker-compose.yml` declarations (services, networks, volumes, environment).
- Performs topological sorting via depth-first cycle detection on service `depends_on` graphs.
- Supports lifecycle control: `boxr compose up`, `boxr compose down`, `boxr compose ps`, and `boxr compose logs`.

### 2.8. Podman Pod & Kubernetes Interop (`src/pod/mod.rs`, `src/kube/mod.rs`)
- Implements Podman-style pod abstractions (`boxr pod create`, `boxr pod ps`, `boxr pod rm`).
- Translates container/pod specs directly to/from Kubernetes Pod manifests (`boxr play kube` and `boxr generate kube`).
- Supports running arbitrary commands in clean user namespaces (`boxr unshare`).

---

## 3. Storage Hierarchy

All state and content-addressable storage is maintained under `~/.boxr`:

```
~/.boxr/
├── bin/                 # Installed binaries & drop-in docker symlinks
│   ├── boxr
│   └── docker
├── completions/         # Shell completion scripts
│   ├── boxr.bash
│   ├── _boxr
│   └── boxr.fish
├── images/              # Content-addressable unpacked image root filesystems
│   └── sha256_<digest>/
│       └── rootfs/
├── layers/              # Raw downloaded layer tar archives
│   └── sha256_<digest>.tar
├── containers/          # Container runtime bundles
│   └── <container_id>/
│       ├── config.json  # OCI Runtime Spec
│       ├── rootfs/      # Copy-on-Write root filesystem
│       └── logs.txt     # Stdout/stderr log stream
├── volumes/             # Persistent named volumes
│   └── <volume_name>/
│       └── _data/
├── buildcache/          # Step-by-step layer build cache
│   └── <step_hash>/
├── images.json          # Local image catalog
├── containers.json      # Container lifecycle index
├── networks.json        # Network topology & IPAM state
├── volumes.json         # Volume registry
├── pods.json            # Pod membership index
├── config.json          # Registry credentials (base64 encoded)
└── events.jsonl         # Real-time lifecycle event stream
```

---

## 4. Platform Differences: Linux vs macOS

| Feature | Linux | macOS (Darwin) |
| :--- | :--- | :--- |
| **Execution Model** | Native in-process (`clone(CLONE_NEWUSER \| CLONE_NEWPID...)`) | Hybrid execution bridge via lightweight Linux VM runner |
| **Filesystem Isolation** | Native `pivot_root()` + private mount namespace | APFS `clonefile` CoW + isolated host directory bind |
| **Layer Storage** | Kernel `overlay` filesystem driver | APFS native block cloning with hardlink fallback |
| **Resource Limits** | Direct `/sys/fs/cgroup/user.slice` controllers | Cgroup resource parameters passed to VM runner |
| **Process Supervision** | Linux `waitpid()` on container PID 1 | Subprocess wait with direct stdio streaming |
