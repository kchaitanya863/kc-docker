# boxr 📦

A fast, lightweight, production-grade **Open Container Initiative (OCI)** compliant container engine, image builder, compose orchestrator, and runtime written in **Rust**.

`boxr` delivers complete Docker/Podman feature parity:
1. **OCI Distribution Specification**: Pulls images directly from registries (Docker Hub, GHCR, Quay), handles bearer token authentication, negotiates manifest lists / indexes for multi-platform architectures (`arm64`, `amd64`), and downloads layer blobs with SHA-256 integrity verification.
2. **OCI Image Specification**: Unpacks layered `.tar` / `.tar.gz` archives into an assembled root filesystem (`rootfs`), handling OCI whiteout files (`.wh.<file>` and `.wh..wh..opq` opaque directories).
3. **OCI Runtime Specification**: Produces standardized OCI bundles (`config.json` + `rootfs`) defining process parameters, mounts (`/proc`, `/sys`, `/dev`), Linux namespaces, and resource constraints.
4. **Volume Management (`boxr volume`)**: Local named persistent volumes, host directory bind mounts (`-v /host:/container:ro`), volume inspection, and lifecycle management.
5. **Network Management (`boxr network`)**: Bridge networks, IPAM (subnet IP allocation & gateway tracking), port forwarding (`-p 8080:80`), and container service discovery.
6. **Dockerfile Builder (`boxr build`)**: Multi-step build engine supporting `FROM`, `RUN`, `COPY`, `ADD`, `WORKDIR`, `ENV`, `CMD`, `ENTRYPOINT`, `EXPOSE`, and `LABEL`.
7. **Compose Orchestrator (`boxr compose`)**: Parsing `docker-compose.yml`, topological dependency graph resolution (`depends_on`), multi-container deployment, teardown, and log streaming.
8. **Daemon REST API (`boxr daemon`)**: Unix Domain Socket server (`boxr.sock`) implementing Docker Engine API endpoints (`/_ping`, `/version`, `/info`, `/containers`, `/images`, `/networks`, `/volumes`).
9. **Container Lifecycle**: Detached mode (`-d`), `stop`, `start`, `logs`, `exec`, and `inspect`.
10. **Dual-Target Execution**:
    - **Linux**: Direct native execution using Linux namespaces (`CLONE_NEWPID`, `CLONE_NEWNS`, `CLONE_NEWUTS`, `CLONE_NEWIPC`, `CLONE_NEWNET`), `pivot_root`, and mount isolation.
    - **macOS**: Automated execution bridge to execute the OCI rootfs and bundle seamlessly on Darwin.

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
│   ├── builder/
│   │   └── mod.rs              # Dockerfile parser, step executor, and image builder
│   ├── compose/
│   │   └── mod.rs              # Compose YAML parser, dependency graph, and orchestrator
│   ├── network/
│   │   └── mod.rs              # Bridge networks, IPAM, and port forwarding
│   ├── volume/
│   │   └── mod.rs              # Named volume storage and bind mount resolver
│   ├── daemon/
│   │   └── mod.rs              # Unix domain socket server & Docker-compatible REST API
│   ├── storage/
│   │   ├── mod.rs              # Local storage manager (~/.boxr/)
│   │   ├── image_store.rs      # Local image index & content-addressable layer store
│   │   └── container_store.rs  # Container lifecycle & state tracking
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

### Containers

```bash
# Run hello-world
./target/release/boxr run hello-world

# Run in background with port forwarding, volumes, and auto-cleanup
./target/release/boxr run -d --name web -p 8080:80 -v my-data:/data alpine /bin/sh -c "echo 'ready' > /data/status.txt; sleep 60"

# View logs
./target/release/boxr logs web

# Execute command inside running container
./target/release/boxr exec web /bin/cat /data/status.txt

# Inspect container metadata
./target/release/boxr inspect web

# Stop, start, and remove
./target/release/boxr stop web
./target/release/boxr start web
./target/release/boxr rm web
```

### Images & Building

```bash
# Pull image
./target/release/boxr pull alpine:latest

# List local images
./target/release/boxr images

# Build image from Dockerfile
./target/release/boxr build -t my-app:v1 .

# Remove image
./target/release/boxr rmi my-app:v1
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

---

## Testing

Run unit and integration tests:

```bash
cargo test
```

## License

MIT OR Apache-2.0
