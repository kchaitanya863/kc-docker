# boxr 📦

A fast, lightweight, production-ready **Open Container Initiative (OCI)** compliant container engine and runtime written in **Rust**.

`boxr` implements the core specifications defined by the OCI:
1. **OCI Distribution Specification**: Pulls images, interacts with registries (Docker Hub, GHCR, Quay), handles bearer token authentication, negotiates manifest lists / indexes for multi-platform architectures, and downloads layer blobs with SHA-256 integrity verification.
2. **OCI Image Specification**: Unpacks layered `.tar` / `.tar.gz` archives into an assembled root filesystem (`rootfs`), handling OCI whiteout files (`.wh.<file>` and `.wh..wh..opq` opaque directories).
3. **OCI Runtime Specification**: Produces standardized OCI bundles (`config.json` + `rootfs`) defining process parameters, mounts (`/proc`, `/sys`, `/dev`), Linux namespaces, and resource constraints.
4. **Container Execution**:
   - **Linux**: Direct native execution using Linux namespaces (`CLONE_NEWPID`, `CLONE_NEWNS`, `CLONE_NEWUTS`, `CLONE_NEWIPC`, `CLONE_NEWNET`), `pivot_root`, and mount isolation.
   - **macOS**: Automated execution bridge to run the OCI rootfs and bundle seamlessly on Darwin.

---

## Architecture

```
boxr/
├── src/
│   ├── main.rs                 # CLI entrypoint and command routing
│   ├── cli.rs                  # Clap CLI arguments and subcommands
│   ├── oci/
│   │   ├── mod.rs              # OCI module definitions
│   │   ├── reference.rs        # Image reference parsing (e.g. library/hello-world:latest)
│   │   ├── distribution.rs     # OCI Distribution Spec / Registry v2 HTTP client
│   │   ├── image.rs            # OCI Image Spec: manifests, configs, layer unpacker & whiteouts
│   │   └── runtime.rs          # OCI Runtime Spec: config.json bundle generator
│   ├── storage/
│   │   ├── mod.rs              # Local storage manager (~/.boxr/)
│   │   ├── image_store.rs      # Local image index & content-addressable layer store
│   │   └── container_store.rs  # Container lifecycle & state tracking
│   └── runtime/
│       ├── mod.rs              # Execution runtime trait & platform routing
│       ├── linux.rs            # Native Linux execution (unshare, pivot_root, mounts)
│       └── darwin.rs           # macOS container execution bridge
└── Cargo.toml
```

---

## Quick Start

### Build

```bash
cargo build --release
```

The resulting binary will be at `target/release/boxr`.

### Pulling an OCI Image

```bash
./target/release/boxr pull hello-world
```

### Running a Container

```bash
# Run hello-world
./target/release/boxr run hello-world

# Run with auto-cleanup (--rm) and custom command
./target/release/boxr run --rm alpine /bin/echo "Hello from boxr!"

# Run with a custom container name and environment variables
./target/release/boxr run --name my-app -e APP_ENV=production alpine /bin/sh -c "env"
```

### Managing Images & Containers

```bash
# List local images
./target/release/boxr images

# List containers (active and exited)
./target/release/boxr ps -a

# Remove a container
./target/release/boxr rm <container_id_or_name>

# Remove an image
./target/release/boxr rmi <image_id_or_tag>
```

### Generating an OCI Runtime Specification

```bash
# Outputs a standard OCI config.json to stdout or bundle directory
./target/release/boxr spec
```

---

## Testing

Run the automated test suite:

```bash
cargo test
```

## License

MIT OR Apache-2.0
