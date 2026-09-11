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
8. **Network Management (`boxr network`)**: Bridge networks, IPAM address allocation, user-space rootless port forwarding proxy, and container DNS resolution.
9. **Dockerfile Builder (`boxr build`)**: Multi-step build engine supporting `FROM`, `RUN`, `COPY`, `ADD`, `WORKDIR`, `ENV`, `CMD`, `ENTRYPOINT`, `EXPOSE`, and `LABEL`.
10. **Compose Orchestrator (`boxr compose`)**: Parsing `docker-compose.yml`, topological dependency graph resolution (`depends_on`), multi-container deployment, teardown, and log streaming.
11. **Daemon REST API (`boxr daemon`)**: Unix Domain Socket server (`boxr.sock`) implementing Docker Engine API endpoints (`/_ping`, `/version`, `/info`, `/containers`, `/images`, `/networks`, `/volumes`).
12. **Container Lifecycle**: Background detached mode (`-d`), `stop`, `start`, `logs`, `exec`, and `inspect`.

---

## Architecture

```
boxr/
├── src/
│   ├── main.rs                 # CLI entrypoint and command routing
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
│   ├── volume/
│   │   └── mod.rs              # Named volume storage and bind mount resolver
│   ├── daemon/
│   │   └── mod.rs              # Unix domain socket server & Docker-compatible REST API
│   └── runtime/
│       ├── mod.rs              # Execution runtime trait & platform routing
│       ├── linux.rs            # Native Linux execution (unshare, pivot_root, mounts)
│       └── darwin.rs           # macOS container execution bridge
└── tests/
    └── integration_test.rs     # Integration test suite
```

---

## Quick Start

### Build

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

# Run in background with port forwarding, volumes, and auto-cleanup
./target/release/boxr run -d --name web -p 8080:80 -v my-data:/data alpine /bin/sh -c "echo 'ready' > /data/status.txt; sleep 60"

# View logs
./target/release/boxr logs web

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

### Daemon REST API

```bash
# Start daemon listening on Unix domain socket
./target/release/boxr daemon --socket ~/.boxr/boxr.sock

# Query Docker Engine API
curl --unix-socket ~/.boxr/boxr.sock http://localhost/_ping
curl --unix-socket ~/.boxr/boxr.sock http://localhost/version
curl --unix-socket ~/.boxr/boxr.sock http://localhost/containers/json
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

## Testing

Run unit and integration tests:

```bash
cargo test
```

## License

MIT OR Apache-2.0
