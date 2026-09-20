# boxr 📦

A fast, lightweight, production-grade **Open Container Initiative (OCI)** compliant container engine, image builder, compose orchestrator, and runtime written in **Rust**.

`boxr` is designed to be **rootless by default** and built from scratch with custom implementations:
1. **Rootless by Default**: User namespaces (`CLONE_NEWUSER`) with `uid_map` and `gid_map` mapping the unprivileged user to container root (UID 0), capability dropping (dropping `CAP_SYS_ADMIN`, `CAP_SYS_RAWIO`, etc.), and default seccomp profiles without requiring `sudo`/root.
2. **Copy-on-Write / OverlayFS Driver**: Native OverlayFS (`lowerdir`, `upperdir`, `workdir`, `merged`) with custom fast hardlink CoW trees for instant sub-millisecond container startup and near-zero disk usage.
3. **cgroups v2 Resource Controllers**: Memory limits (`--memory`), CPU quota & period (`--cpus`), and task limits (`--pids-limit`) managed via the cgroupfs v2 hierarchy.
4. **Image Export & Import (`save` / `load`)**: Standard multi-layer Docker/OCI tar archives (`boxr save -o image.tar <image>` and `boxr load -i image.tar`).
5. **Registry Authentication & Push**: `boxr login`, `boxr logout`, and `boxr push` using base64 encoded credentials in `~/.boxr/config.json`.
6. **Interactive PTY / Terminal**: Raw mode terminal guards (`-it`), window resize (`TIOCGWINSZ`), and clean terminal state restoration.
7. **Volume Management (`boxr volume`)**: Local named persistent volumes, host directory bind mounts (`-v /host:/container:ro`), volume inspection, and lifecycle management.
8. **Network Management (`boxr network`, `usernet`, & `pasta`)**: Bridge networks, IPAM address allocation, embedded pure-Rust user-mode network stack (`usernet`) with zero external dependencies, native `pasta` integration, and user-space rootless port forwarding proxy.
9. **Dockerfile Builder (`boxr build`)**: Multi-step build engine supporting `FROM`, `RUN`, `COPY`, `ADD`, `WORKDIR`, `ENV`, `CMD`, `ENTRYPOINT`, `EXPOSE`, and `LABEL`.
10. **Compose Orchestrator (`boxr compose`)**: Parsing `docker-compose.yml`, topological dependency graph resolution (`depends_on`), multi-container deployment, teardown, and log streaming.
11. **Daemon REST API (`boxr daemon`)**: Unix Domain Socket server (`boxr.sock`) implementing Docker Engine API endpoints (`/_ping`, `/version`, `/info`, `/containers`, `/images`, `/networks`, `/volumes`).
12. **Container Lifecycle**: Background detached mode (`-d`), `stop`, `start`, `logs`, `exec`, and `inspect`.

---

## Architecture & Comprehensive Documentation

Comprehensive engineering guides, architectural diagrams, and command manuals:
- **[Architecture & Runtime Internals Guide](docs/ARCHITECTURE.md)**: Deep dive into the trampoline, storage hierarchy, and multi-platform hypervisor bridge.
- **[Enterprise Runtimes & Hardening Guide](docs/ENTERPRISE_RUNTIMES.md)**: Privilege dropping, `/tmp` sticky `1777` permissions, POSIX `/dev/shm`, PID 1 init, and cgroups v2.
- **[Rootless Container Isolation](docs/ROOTLESS.md)**: Kernel namespaces, single-threaded trampoline, subordinate ID mapping, and capabilities.
- **[Container Networking & UserNet Stack](docs/NETWORKING.md)**: Pure-Rust embedded TAP network stack, `pasta` integration, and IPAM.
- **[CLI Command Reference Manual](docs/CLI_REFERENCE.md)**: Exhaustive syntax, flags, and options for all 38+ CLI subcommands.
- **[Kubernetes Pods & Compose Orchestration](docs/KUBERNETES_AND_COMPOSE.md)**: Podman-style pods, `play kube`, and Docker Compose DAGs.

```
boxr/
├── src/
│   ├── main.rs                 # CLI entrypoint, unshare, and single-threaded trampolines
│   ├── lib.rs                  # Library crate root and command orchestration
│   ├── cli/                    # Clap CLI arguments, options, and command definitions
│   │   ├── mod.rs              # Top-level Cli and Commands enum
│   │   ├── container.rs        # Container flags (RunArgs, ExecArgs, LogsArgs, PsArgs)
│   │   ├── image.rs            # Image flags (BuildArgs, ImagesArgs, PullArgs, Save/Load)
│   │   ├── system.rs           # System flags (DaemonArgs, EventsArgs, ServiceArgs, Pods)
│   │   └── volumes_networks.rs # VolumeSubcommands and NetworkSubcommands
│   ├── oci/
│   │   ├── mod.rs              # OCI module definitions
│   │   ├── reference.rs        # Image reference parsing (e.g. library/hello-world:latest)
│   │   ├── distribution.rs     # OCI Distribution Spec / Registry v2 HTTP client & ImageDistribution trait
│   │   ├── image.rs            # OCI Image Spec: manifests, configs, layer unpacker & whiteouts
│   │   └── runtime.rs          # OCI Runtime Spec: config.json bundle generator
│   ├── security/
│   │   └── mod.rs              # Rootless user namespaces, UID/GID maps, capabilities, seccomp
│   ├── cgroups/
│   │   └── mod.rs              # cgroups v2 resource limit controllers & ResourceManager trait
│   ├── storage/
│   │   ├── mod.rs              # Local storage manager (~/.boxr/)
│   │   ├── traits.rs           # ISP storage traits (ContainerReader/Writer, ImageReader/Writer)
│   │   ├── overlay.rs          # OverlayFS & Copy-on-Write storage driver
│   │   ├── image_store.rs      # Local image index & content-addressable layer store
│   │   └── container_store.rs  # Container lifecycle & state tracking
│   ├── auth/
│   │   └── mod.rs              # Credential store, tar archiver (save/load), and registry push
│   ├── terminal/
│   │   └── mod.rs              # Raw terminal PTY guard and window size detection
│   ├── builder/
│   │   ├── mod.rs              # Builder module exports and regression contracts
│   │   ├── parser.rs           # Multi-stage Dockerfile parser & Instruction AST
│   │   ├── executor.rs         # Build stage executor and containerized step runner
│   │   ├── cache.rs            # Content-addressed build cache manager
│   │   └── dockerignore.rs     # .dockerignore pattern matcher and wildcard resolver
│   ├── compose/
│   │   └── mod.rs              # Compose YAML parser, dependency graph, and orchestrator
│   ├── network/
│   │   ├── mod.rs              # Bridge networks, IPAM, and NetworkReader/Writer traits
│   │   ├── pasta.rs            # Pasta user-mode tap rootless network driver
│   │   ├── usernet/            # Pure-Rust native user-mode L2/L3/L4 network stack
│   │   │   ├── mod.rs          # UserNet exports and TAP packet loop integration
│   │   │   ├── packets.rs      # Ethernet, ARP, IPv4, UDP, and TCP header serializers & checksums
│   │   │   └── engine.rs       # In-memory ARP, ICMP echo, and TCP NAT proxy engine
│   │   └── rootless.rs         # Rootless user-space TCP port forwarder proxy
│   ├── pod/
│   │   └── mod.rs              # Podman pod lifecycle and namespace sharing (PodReader/Writer)
│   ├── kube/
│   │   └── mod.rs              # Kubernetes Pod YAML play, generate, and unshare
│   ├── health/
│   │   └── mod.rs              # Container healthcheck probes (HealthProbe trait) and restart supervisor
│   ├── stats/
│   │   └── mod.rs              # Real-time resource usage collector (CPU %, RAM, PIDs)
│   ├── events/
│   │   └── mod.rs              # Real-time JSONL lifecycle events recorder (EventSink & EventFilter)
│   ├── system/
│   │   └── mod.rs              # Disk space auditing (df_with_readers) and automated pruning
│   ├── volume/
│   │   └── mod.rs              # Named volume storage and bind mount resolver (VolumeReader/Writer)
│   ├── daemon/
│   │   ├── mod.rs              # Unix domain socket server & router dispatcher
│   │   ├── containers.rs       # Container lifecycle REST endpoints (create, inspect, wait, logs)
│   │   ├── images.rs           # Image management REST endpoints (list, create, tag, inspect)
│   │   ├── system.rs           # Daemon health and system info endpoints (_ping, version, info)
│   │   ├── prune.rs            # Resource cleanup endpoints (containers, images, volumes, networks)
│   │   └── volumes_networks.rs # Volume & network REST CRUD endpoints
│   └── runtime/
│       ├── mod.rs              # Execution runtime traits (ContainerRuntime & ProcessKiller)
│       ├── traits.rs           # Container runtime engine abstractions
│       ├── kill.rs             # Process termination & signal parsing
│       ├── cp.rs               # Container-to-host and host-to-container copy engine
│       ├── diff.rs             # Container filesystem diff engine
│       ├── top.rs              # Process inspection inside container bundles
│       ├── linux.rs            # Native Linux execution (unshare, pivot_root, mounts)
│       └── darwin.rs           # macOS container execution bridge (Virtualization.framework)
└── tests/
    ├── docker_parity_test.rs   # Upstream Docker and Podman CLI parity tests
    ├── e2e_test.rs             # End-to-end container, network, and compose tests
    ├── enterprise_scenarios_test.rs # Enterprise workload and security isolation tests
    ├── integration_test.rs     # Integration test suite
    ├── issues_47_to_111_test.rs # Regression tests for issues #47 to #111
    ├── issues_113_to_162_test.rs# Regression tests for issues #113 to #162
    └── qa_*.rs                 # Edge-case, negative, and breaking test matrices
```

---

## Quick Start

### Installation

#### Homebrew (macOS & Linux)

```bash
# Add the official tap (central hub for all formulae)
brew tap kchaitanya863/tap

# Install boxr (installs precompiled binary + bash/zsh/fish completions)
brew install boxr

# (Optional) Run daemon as an always-on background service with autostart on login:
brew services start boxr
```

#### Debian / Ubuntu (`.deb`)

Download and install the native `.deb` package from the [latest release](https://github.com/kchaitanya863/homebrew-tap/releases):

```bash
# x86_64 / amd64:
curl -fsSLO https://github.com/kchaitanya863/homebrew-tap/releases/latest/download/boxr_0.1.18_amd64.deb
sudo dpkg -i boxr_*_amd64.deb

# ARM64 / aarch64:
curl -fsSLO https://github.com/kchaitanya863/homebrew-tap/releases/latest/download/boxr_0.1.18_arm64.deb
sudo dpkg -i boxr_*_arm64.deb
```

#### Fedora / RHEL / CentOS (`.rpm` / YUM / DNF)

Download and install the native `.rpm` package from the [latest release](https://github.com/kchaitanya863/homebrew-tap/releases):

```bash
# Install via dnf or rpm:
sudo dnf install https://github.com/kchaitanya863/homebrew-tap/releases/latest/download/boxr-0.1.18-1.x86_64.rpm

# Or via rpm directly:
sudo rpm -ivh boxr-*.rpm
```

#### Windows (Chocolatey & Winget)

Install via Chocolatey:

```powershell
choco install boxr
```

Or extract the precompiled Windows `.zip` package from the [latest release](https://github.com/kchaitanya863/homebrew-tap/releases):

```powershell
Invoke-WebRequest -Uri https://github.com/kchaitanya863/homebrew-tap/releases/latest/download/boxr-windows-x86_64.zip -OutFile boxr.zip
Expand-Archive boxr.zip -DestinationPath C:\ProgramData\boxr
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";C:\ProgramData\boxr\bin", "Machine")
```

#### Automated Shell Installer (curl | sh)

```bash
# Build & install boxr binary and completions
./install.sh

# Or install from GitHub:
curl -fsSL https://raw.githubusercontent.com/kchaitanya863/kc-docker/main/install.sh | sh
```

#### (Optional) Docker Drop-in Alias
If you would like existing `docker` commands to transparently run with `boxr`, you can choose to configure an alias or wrapper:

- **Option A (Recommended wrapper)**: Install a standalone wrapper binary at `~/.boxr/bin/docker`:
  ```bash
  boxr alias --install
  ```
- **Option B (Shell alias)**: Add an alias to your shell profile (`~/.zshrc`, `~/.bashrc`, or `~/.config/fish/config.fish`):
  ```bash
  # Bash / Zsh
  alias docker="boxr"

  # Fish
  alias docker "boxr"
  ```
- **Option C (Windows PowerShell)**: Add an alias in your PowerShell profile:
  ```powershell
  Set-Alias -Name docker -Value boxr
  ```
*If you already have Docker installed side-by-side or prefer explicit names, simply invoke `boxr` directly without creating any alias.*

#### Manual Build

```bash
cargo build --release
```

The resulting binary will be at `target/release/boxr`.

---

## Command Reference

### Containers & Execution

```bash
# Run hello-world
./target/release/boxr run hello-world

# Run with resource constraints and rootless mode
./target/release/boxr run --memory 512m --cpus 1.5 --pids-limit 100 --rm alpine /bin/echo "Resource limits enforced"

# Run with Podman-style pasta rootless network namespace isolation
./target/release/boxr run --network pasta -p 8080:80 nginx:latest

# Run with zero-dependency pure-Rust user-mode network stack (usernet)
./target/release/boxr run --network usernet -p 8080:80 nginx:latest

# Run with isolated loopback-only private network namespace
./target/release/boxr run --network none alpine ifconfig

# Run in background with port forwarding, volumes, and auto-cleanup
./target/release/boxr run -d --name web -p 8080:80 -v my-data:/data alpine /bin/sh -c "echo 'ready' > /data/status.txt; sleep 60"

# View logs (with live following, timestamps, and line tailing)
./target/release/boxr logs -f -t -n 50 web

# Send Unix signals to container process (default: SIGKILL)
./target/release/boxr kill -s SIGHUP web
./target/release/boxr kill web

# Execute command inside running container
./target/release/boxr exec web /bin/cat /data/status.txt

# Inspect container metadata
./target/release/boxr inspect web

# Inspect container filesystem diff (Added, Changed, Deleted files)
./target/release/boxr diff web

# View running container processes
./target/release/boxr top web

# Pause and unpause container
./target/release/boxr pause web
./target/release/boxr unpause web

# Rename container
./target/release/boxr rename web production-web

# Wait for container to exit and print exit code
./target/release/boxr wait production-web

# Copy files between host and container
./target/release/boxr cp web:/app/config.json ./local-config.json
./target/release/boxr cp ./updated-config.json web:/app/config.json

# Dynamically update container resource limits without restarting
./target/release/boxr update --memory 1g --cpus 2.0 --pids-limit 200 web

# Attach local terminal streams to running container
./target/release/boxr attach web

# Stop, start, and remove
./target/release/boxr stop production-web
./target/release/boxr start production-web
./target/release/boxr rm production-web
```

### Images, Commit & Registry Auth

```bash
# Pull image
./target/release/boxr pull alpine:latest

# Build image from Dockerfile
./target/release/boxr build -t my-app:v1 .

# Commit container changes into a new image
./target/release/boxr commit -m "added custom configs" my-container new-app:v1

# Save image to tar archive
./target/release/boxr save -o my-app.tar my-app:v1

# Load image from tar archive
./target/release/boxr load -i my-app.tar

# Log in to registry
./target/release/boxr login -u myuser -p mysecret

# Push image
./target/release/boxr push my-app:v1

# Log out
./target/release/boxr logout
```

### Volumes

```bash
# Create named volume
./target/release/boxr volume create app-db

# List volumes
./target/release/boxr volume ls

# Inspect volume
./target/release/boxr volume inspect app-db

# Remove volume
./target/release/boxr volume rm app-db
```

### Networks

```bash
# Create custom bridge network
./target/release/boxr network create my-net --subnet 172.30.0.0/16

# List networks
./target/release/boxr network ls

# Inspect network and attached endpoints
./target/release/boxr network inspect my-net

# Connect container to network
./target/release/boxr network connect my-net my-container

# Remove network
./target/release/boxr network rm my-net
```

### Compose

```bash
# Start multi-container application
./target/release/boxr compose -f docker-compose.yml up -d

# Check service status
./target/release/boxr compose ps

# Stream logs
./target/release/boxr compose logs

# Stop and remove containers and networks
./target/release/boxr compose down
```

### Podman Pods & Kubernetes Workloads

```bash
# Create a multi-container pod sharing network/IPC
./target/release/boxr pod create --name web-pod -p 8080:80

# List pods
./target/release/boxr pod ps

# Inspect pod configuration
./target/release/boxr pod inspect web-pod

# Play a Kubernetes Pod YAML directly
./target/release/boxr play kube pod.yaml

# Generate a Kubernetes Pod YAML from an existing container or pod
./target/release/boxr generate kube my-container

# Run a command inside a new user namespace
./target/release/boxr unshare whoami

# Remove pod and member containers
./target/release/boxr pod rm web-pod
```

### Daemon REST API & Background Service

```bash
# Start daemon listening on Unix domain socket (or TCP on Windows)
./target/release/boxr daemon --socket ~/.boxr/boxr.sock

# Query Docker Engine API
curl --unix-socket ~/.boxr/boxr.sock http://localhost/_ping
curl --unix-socket ~/.boxr/boxr.sock http://localhost/version
curl --unix-socket ~/.boxr/boxr.sock http://localhost/containers/json

# Manage as an always-on background service with autostart on login:
boxr service install
boxr service start
boxr service status
boxr service stop
boxr service uninstall
```

### Stats & Events Monitoring

```bash
# Display live streaming container resource stats (CPU, Memory, PIDs)
./target/release/boxr stats

# Snapshot stats without streaming
./target/release/boxr stats --no-stream

# Stream real-time container lifecycle events (create, start, die, stop)
./target/release/boxr events
```

### System & Disk Usage

```bash
# Show disk space used by containers, images, volumes, and build cache
./target/release/boxr system df

# Reclaim space by removing stopped containers, unused networks, and build cache
./target/release/boxr system prune

# Comprehensive prune (including all unused images and volumes)
./target/release/boxr system prune --all --volumes
```

### Shell Completions & Docker Drop-in Alias

```bash
# Generate shell autocompletion script (bash, zsh, fish)
./target/release/boxr completion zsh > ~/.zfunc/_boxr
./target/release/boxr completion bash > /etc/bash_completion.d/boxr

# Generate shell alias command
./target/release/boxr alias

# Install Docker drop-in wrapper script in ~/.boxr/bin/docker
./target/release/boxr alias --install
```

---

## Performance Benchmarks vs Docker

Run the automated benchmark suite:

```bash
./benchmark.sh
```

### Benchmark Results (Apple Silicon arm64)

| Benchmark Metric | Boxr (Rust) | Docker | Comparison |
| :--- | :--- | :--- | :--- |
| **Daemon Idle Memory Footprint** | **8.4 MB** | 859.2 MB | **102x lighter** 🍃 |
| **CLI Peak RAM Usage (RSS)** | **8.3 MB** | 29.1 MB | **3.5x less RAM** ⚡ |
| **CLI Binary Size on Disk** | **8.0 MB** (6.2M stripped) | 39.6 MB | **5x to 6.5x smaller** 📦 |
| **Full Suite Disk Footprint** | **8.0 MB** | 2.1 GB | **260x smaller** 📦 |
| **Dockerfile Build (Cold)** | **464.5 ms** | 880.6 ms | **1.90x faster** 🚀 |
| **Dockerfile Build (Cached)** | **161.9 ms** | 182.4 ms | **1.13x faster** 🚀 |
| **Container Startup Latency (Median)** | 261.5 ms | 191.6 ms | ~par (on macOS bridge) |
| **5 Concurrent Containers Spawn** | 546.4 ms | 427.5 ms | sub-second throughput |
| **Volume I/O (10MB Write+Read)** | 300.5 ms | 218.6 ms | near-native I/O |

---

## Testing

Run the full automated test suite (66 unit, integration, and E2E tests):

```bash
cargo test
```

### Test Coverage Highlights
- **OCI Image References**: Positive canonical specs, custom registries/ports, tags with symbols, sha256 digest pins, and negative/malformed inputs.
- **Dockerfile & Multi-Stage**: Named stages (`AS builder`), multi-stage artifact extraction (`COPY --from=`), `.dockerignore` negation (`!exception`) and globbing, line continuations, and malformed instruction detection.
- **Compose Orchestrator**: Multi-service dependency graphs, circular dependency detection (`A -> B -> A`, `A -> B -> C -> A`), and undefined dependency validation.
- **Volumes & Networking**: Host bind mounts, read-only modes, collision rejection, default `boxr0` bridge deletion protection, IPAM sequence allocation, and invalid port boundary checks.
- **Security & Cgroups**: Custom Base64 roundtrip fuzzing across all byte lengths, default seccomp critical syscall blocking, cgroups memory unit multipliers, and Unix signal matrix.
- **E2E & Real Workloads**: Non-existent container error handling, invalid CLI flags, live Redis server with `exec` ping/set/get, and full-stack compose apps.

## License

MIT OR Apache-2.0
