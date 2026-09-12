# boxr 📦

[![CI & Automated Release](https://github.com/kchaitanya863/kc-docker/actions/workflows/ci.yml/badge.svg)](https://github.com/kchaitanya863/kc-docker/actions/workflows/ci.yml)

A fast, lightweight, production-grade **Open Container Initiative (OCI)** compliant container engine, image builder, compose orchestrator, and runtime written in **Rust**.

`boxr` is designed to be **rootless by default** and built from scratch with custom implementations:
1. **Rootless by Default**: User namespaces (`CLONE_NEWUSER`) with `uid_map` and `gid_map` mapping the unprivileged user to container root (UID 0), capability dropping (dropping `CAP_SYS_ADMIN`, `CAP_SYS_RAWIO`, etc.), and default seccomp profiles without requiring `sudo`/root.
2. **Copy-on-Write / OverlayFS Driver**: Native OverlayFS (`lowerdir`, `upperdir`, `workdir`, `merged`) with custom fast hardlink CoW trees for instant sub-millisecond container startup and near-zero disk usage.
3. **cgroups v2 Resource Controllers**: Memory limits (`--memory`), CPU quota & period (`--cpus`), and task limits (`--pids-limit`) managed via the cgroupfs v2 hierarchy.
4. **Image Export & Import (`save` / `load`)**: Standard multi-layer Docker/OCI tar archives (`boxr save -o image.tar <image>` and `boxr load -i image.tar`).
5. **Registry Authentication & Push**: `boxr login`, `boxr logout`, and `boxr push` using base64 encoded credentials in `~/.boxr/config.json`.
6. **Interactive PTY / Terminal**: Raw mode terminal guards (`-it`), window resize (`TIOCGWINSZ`), and clean terminal state restoration.
7. **Volume Management (`boxr volume`)**: Local named persistent volumes, host directory bind mounts (`-v /host:/container:ro`), volume inspection, and lifecycle management.
8. **Network Management (`boxr network`)**: Bridge networks, IPAM address allocation, user-space rootless port forwarding proxy, and container DNS resolution.
9. **Dockerfile Builder (`boxr build`)**: Multi-step build engine supporting `FROM`, `RUN`, `COPY`, `ADD`, `WORKDIR`, `ENV`, `CMD`, `ENTRYPOINT`, `EXPOSE`, and `LABEL`.
10. **Compose Orchestrator (`boxr compose`)**: Parsing `docker-compose.yml`, topological dependency graph resolution (`depends_on`), multi-container deployment, teardown, and log streaming.
11. **Daemon REST API (`boxr daemon`)**: Unix Domain Socket server (`boxr.sock`) implementing Docker Engine API endpoints (`/_ping`, `/version`, `/info`, `/containers`, `/images`, `/networks`, `/volumes`).
12. **Container Lifecycle**: Background detached mode (`-d`), `stop`, `start`, `logs`, `exec`, and `inspect`.

---

## Architecture & Documentation

For detailed architectural diagrams, subsystem designs, and storage hierarchies, see the [Architecture & Internals Guide](docs/ARCHITECTURE.md).

```
boxr/
├── src/
│   ├── main.rs                 # CLI entrypoint and command routing
│   ├── lib.rs                  # Library crate root and command handlers
│   ├── cli.rs                  # Clap CLI arguments, options, and subcommands
│   ├── oci/
│   │   ├── mod.rs              # OCI module definitions
│   │   ├── reference.rs        # Image reference parsing (e.g. library/hello-world:latest)
│   │   ├── distribution.rs     # OCI Distribution Spec / Registry v2 HTTP client
│   │   ├── image.rs            # OCI Image Spec: manifests, configs, layer unpacker & whiteouts
│   │   └── runtime.rs          # OCI Runtime Spec: config.json bundle generator
│   ├── security/
│   │   └── mod.rs              # Rootless user namespaces, UID/GID maps, capabilities, seccomp
│   ├── cgroups/
│   │   └── mod.rs              # cgroups v2 resource limit controllers (memory, cpus, pids)
│   ├── storage/
│   │   ├── mod.rs              # Local storage manager (~/.boxr/)
│   │   ├── overlay.rs          # OverlayFS & Copy-on-Write storage driver
│   │   ├── image_store.rs      # Local image index & content-addressable layer store
│   │   └── container_store.rs  # Container lifecycle & state tracking
│   ├── auth/
│   │   └── mod.rs              # Credential store, tar archiver (save/load), and registry push
│   ├── terminal/
│   │   └── mod.rs              # Raw terminal PTY guard and window size detection
│   ├── builder/
│   │   └── mod.rs              # Dockerfile parser, step executor, and image builder
│   ├── compose/
│   │   └── mod.rs              # Compose YAML parser, dependency graph, and orchestrator
│   ├── network/
│   │   ├── mod.rs              # Bridge networks, IPAM, and port forwarding
│   │   └── rootless.rs         # Rootless user-space TCP port forwarder proxy
│   ├── pod/
│   │   └── mod.rs              # Podman pod lifecycle and namespace sharing
│   ├── kube/
│   │   └── mod.rs              # Kubernetes Pod YAML play, generate, and unshare
│   ├── health/
│   │   └── mod.rs              # Container healthcheck probes and restart policies
│   ├── stats/
│   │   └── mod.rs              # Real-time resource usage collector (CPU %, RAM, PIDs)
│   ├── events/
│   │   └── mod.rs              # Real-time JSONL lifecycle events recorder and streamer
│   ├── system/
│   │   └── mod.rs              # Disk space auditing (df) and automated pruning
│   ├── volume/
│   │   └── mod.rs              # Named volume storage and bind mount resolver
│   ├── daemon/
│   │   └── mod.rs              # Unix domain socket server & Docker-compatible REST API
│   └── runtime/
│       ├── mod.rs              # Execution runtime trait & platform routing
│       ├── linux.rs            # Native Linux execution (unshare, pivot_root, mounts)
│       └── darwin.rs           # macOS container execution bridge
└── tests/
    ├── e2e_test.rs             # End-to-end workload and integration test suite
    ├── integration_test.rs     # Integration test suite
    └── qa_*.rs                 # Exhaustive edge case and breaking test suites
```

---

## Quick Start

### Installation

#### Automated Installer (Recommended)

```bash
# Build & install boxr, Docker drop-in wrapper, and completions
./install.sh

# Or install from GitHub:
curl -fsSL https://raw.githubusercontent.com/kchaitanya863/kc-docker/main/install.sh | sh
```

#### Precompiled Release (v0.1.2)

```bash
# Download and extract the precompiled release tarball
tar -xzvf boxr-linux-x86_64.tar.gz
```

The resulting binary will be at `bin/boxr`.

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
./bin/boxr run hello-world

# Run with resource constraints and rootless mode
./bin/boxr run --memory 512m --cpus 1.5 --pids-limit 100 --rm alpine /bin/echo "Resource limits enforced"

# Run in background with port forwarding, volumes, and auto-cleanup
./bin/boxr run -d --name web -p 8080:80 -v my-data:/data alpine /bin/sh -c "echo 'ready' > /data/status.txt; sleep 60"

# View logs (with live following, timestamps, and line tailing)
./bin/boxr logs -f -t -n 50 web

# Send Unix signals to container process (default: SIGKILL)
./bin/boxr kill -s SIGHUP web
./bin/boxr kill web

# Execute command inside running container
./bin/boxr exec web /bin/cat /data/status.txt

# Inspect container metadata
./bin/boxr inspect web

# Inspect container filesystem diff (Added, Changed, Deleted files)
./bin/boxr diff web

# View running container processes
./bin/boxr top web

# Pause and unpause container
./bin/boxr pause web
./bin/boxr unpause web

# Rename container
./bin/boxr rename web production-web

# Wait for container to exit and print exit code
./bin/boxr wait production-web

# Copy files between host and container
./bin/boxr cp web:/app/config.json ./local-config.json
./bin/boxr cp ./updated-config.json web:/app/config.json

# Dynamically update container resource limits without restarting
./bin/boxr update --memory 1g --cpus 2.0 --pids-limit 200 web

# Attach local terminal streams to running container
./bin/boxr attach web

# Stop, start, and remove
./bin/boxr stop production-web
./bin/boxr start production-web
./bin/boxr rm production-web
```

### Images, Commit & Registry Auth

```bash
# Pull image
./bin/boxr pull alpine:latest

# Build image from Dockerfile
./bin/boxr build -t my-app:v1 .

# Commit container changes into a new image
./bin/boxr commit -m "added custom configs" my-container new-app:v1

# Save image to tar archive
./bin/boxr save -o my-app.tar my-app:v1

# Load image from tar archive
./bin/boxr load -i my-app.tar

# Log in to registry
./bin/boxr login -u myuser -p mysecret

# Push image
./bin/boxr push my-app:v1

# Log out
./bin/boxr logout
```

### Volumes

```bash
# Create named volume
./bin/boxr volume create app-db

# List volumes
./bin/boxr volume ls

# Inspect volume
./bin/boxr volume inspect app-db

# Remove volume
./bin/boxr volume rm app-db
```

### Networks

```bash
# Create custom bridge network
./bin/boxr network create my-net --subnet 172.30.0.0/16

# List networks
./bin/boxr network ls

# Inspect network and attached endpoints
./bin/boxr network inspect my-net

# Connect container to network
./bin/boxr network connect my-net my-container

# Remove network
./bin/boxr network rm my-net
```

### Compose

```bash
# Start multi-container application
./bin/boxr compose -f docker-compose.yml up -d

# Check service status
./bin/boxr compose ps

# Stream logs
./bin/boxr compose logs

# Stop and remove containers and networks
./bin/boxr compose down
```

### Podman Pods & Kubernetes Workloads

```bash
# Create a multi-container pod sharing network/IPC
./bin/boxr pod create --name web-pod -p 8080:80

# List pods
./bin/boxr pod ps

# Inspect pod configuration
./bin/boxr pod inspect web-pod

# Play a Kubernetes Pod YAML directly
./bin/boxr play kube pod.yaml

# Generate a Kubernetes Pod YAML from an existing container or pod
./bin/boxr generate kube my-container

# Run a command inside a new user namespace
./bin/boxr unshare whoami

# Remove pod and member containers
./bin/boxr pod rm web-pod
```

### Daemon REST API

```bash
# Start daemon listening on Unix domain socket
./bin/boxr daemon --socket ~/.boxr/boxr.sock

# Query Docker Engine API
curl --unix-socket ~/.boxr/boxr.sock http://localhost/_ping
curl --unix-socket ~/.boxr/boxr.sock http://localhost/version
curl --unix-socket ~/.boxr/boxr.sock http://localhost/containers/json
```

### Stats & Events Monitoring

```bash
# Display live streaming container resource stats (CPU, Memory, PIDs)
./bin/boxr stats

# Snapshot stats without streaming
./bin/boxr stats --no-stream

# Stream real-time container lifecycle events (create, start, die, stop)
./bin/boxr events
```

### System & Disk Usage

```bash
# Show disk space used by containers, images, volumes, and build cache
./bin/boxr system df

# Reclaim space by removing stopped containers, unused networks, and build cache
./bin/boxr system prune

# Comprehensive prune (including all unused images and volumes)
./bin/boxr system prune --all --volumes
```

### Shell Completions & Docker Drop-in Alias

```bash
# Generate shell autocompletion script (bash, zsh, fish)
./bin/boxr completion zsh > ~/.zfunc/_boxr
./bin/boxr completion bash > /etc/bash_completion.d/boxr

# Generate shell alias command
./bin/boxr alias

# Install Docker drop-in wrapper script in ~/.boxr/bin/docker
./bin/boxr alias --install
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
