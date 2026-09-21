# Boxr CLI Command Reference 📖

Complete reference manual for all commands, flags, and options supported by `boxr`.

---

## Command Overview

| Category | Commands |
| :--- | :--- |
| **Container Lifecycle** | `run`, `create`, `start`, `stop`, `restart`, `pause`, `unpause`, `kill`, `rm`, `ps`, `wait`, `init`, `cleanup`, `clone` |
| **Container Operations** | `logs`, `exec`, `attach`, `top`, `diff`, `cp`, `rename`, `update`, `commit`, `inspect`, `checkpoint`, `restore`, `runlabel`, `mount`, `unmount`, `exists` |
| **Image Management** | `pull`, `build`, `save`, `load`, `push`, `tag`, `untag`, `import`, `export`, `history`, `images`, `rmi`, `search`, `diff`, `scp`, `sign`, `tree`, `trust`, `mount`, `unmount`, `exists` |
| **Volumes** | `volume create`, `volume ls`, `volume inspect`, `volume rm`, `volume prune`, `volume export`, `volume import`, `volume reload`, `volume rename`, `volume mount`, `volume unmount`, `volume exists` |
| **Networks** | `network create`, `network ls`, `network inspect`, `network connect`, `network disconnect`, `network rm`, `network prune`, `network update`, `network reload`, `network exists` |
| **Pods & Kubernetes** | `pod create`, `pod ps`, `pod inspect`, `pod stop`, `pod start`, `pod rm`, `pod clone`, `pod logs`, `pod exists`, `kube play`, `kube down`, `kube generate`, `kube apply`, `play kube`, `generate kube`, `unshare` |
| **Quadlets** | `quadlet ls`, `quadlet install`, `quadlet print`, `quadlet rm` |
| **Artifacts** | `artifact add`, `artifact extract`, `artifact inspect`, `artifact ls`, `artifact pull`, `artifact push`, `artifact rm` |
| **Virtual Machines** | `machine init`, `machine start`, `machine stop`, `machine ls`, `machine rm`, `machine info`, `machine inspect`, `machine set`, `machine os`, `machine reset`, `machine restart`, `machine ssh`, `machine cp` |
| **Build Farm** | `farm create`, `farm ls`, `farm update`, `farm rm`, `farm build` |
| **Compose** | `compose up`, `compose down`, `compose ps`, `compose logs`, `compose build`, `compose restart`, `compose stop`, `compose start`, `compose rm`, `compose config` |
| **Daemon & Service** | `daemon`, `service install`, `service start`, `service stop`, `service status`, `service uninstall`, `system service` |
| **Secrets** | `secret create`, `secret ls`, `secret inspect`, `secret rm`, `secret exists` |
| **System & Utilities** | `info`, `version`, `auto-update`, `healthcheck run`, `system check`, `system connection`, `system df`, `system prune`, `system migrate`, `system renumber`, `system reset`, `system hyperv-prep`, `generate spec`, `spec`, `builder du`, `builder prune`, `completion`, `alias`, `mount`, `unmount` |

---

## 1. Container Lifecycle

### `boxr run`
Run a command in a new container.

```bash
boxr run [OPTIONS] <IMAGE> [COMMAND]...
```

**Options:**
- `-d, --detach`: Run container in background and print container ID.
- `-i, --interactive`: Keep STDIN open even if not attached.
- `-t, --tty`: Allocate a pseudo-TTY.
- `--name <NAME>`: Assign a custom name to the container.
- `--rm`: Automatically remove the container when it exits.
- `-e, --env <KEY=VAL>`: Set environment variables (can be used multiple times).
- `-p, --publish <PORT>`: Publish a container port to the host (`host_port:container_port` or `ip:host_port:container_port`).
- `-v, --volume <VOL>`: Bind mount a volume or host path (`host_path:container_path[:ro]`).
- `-w, --workdir <DIR>`: Working directory inside the container.
- `--network, --net <MODE>`: Network mode (`auto`, `pasta`, `usernet`, `bridge`, `host`, `none`). Default: `auto`.
- `--memory <LIMIT>`: Memory limit (e.g. `512m`, `1g`, `2048k`).
- `--cpus <COUNT>`: CPU quota limit (e.g. `1.5`, `2.0`).
- `--pids-limit <LIMIT>`: Maximum number of processes permitted.
- `--privileged`: Give extended container privileges.
- `--restart <POLICY>`: Restart policy (`no`, `always`, `on-failure[:max-retries]`).
- `--health-cmd <CMD>`: Command to execute to check container health status.
- `--platform <PLATFORM>`: Target OS/Arch platform (e.g. `linux/amd64`, `linux/arm64`).

### `boxr create`
Create a new container without starting it. Takes the same flags as `boxr run`.

```bash
boxr create --name db -p 5432:5432 postgres:alpine
```

### `boxr start`
Start one or more stopped containers.

```bash
boxr start <CONTAINER>
```

### `boxr stop`
Stop a running container with graceful `SIGTERM` followed by `SIGKILL`.

```bash
boxr stop [-t <TIMEOUT_SECS>] <CONTAINER>
```

### `boxr restart`
Restart a running or stopped container.

```bash
boxr restart [-t <TIMEOUT_SECS>] <CONTAINER>
```

### `boxr pause` / `boxr unpause`
Freeze or unfreeze container processes using cgroups v2 freezer.

```bash
boxr pause <CONTAINER>
boxr unpause <CONTAINER>
```

### `boxr kill`
Send a specific Unix signal to a container's main process (default: `SIGKILL`).

```bash
boxr kill [-s <SIGNAL>] <CONTAINER>
```

### `boxr rm`
Remove one or more containers.

```bash
boxr rm [-f] [-v] <CONTAINER>
```
- `-f, --force`: Force removal of a running container.
- `-v, --volumes`: Remove anonymous volumes associated with the container.

### `boxr ps`
List containers.

```bash
boxr ps [OPTIONS]
```
- `-a, --all`: Show all containers (default shows only running).
- `-q, --quiet`: Only display container IDs.
- `--no-trunc`: Do not truncate output IDs.

### `boxr wait`
Block until a container stops, then return its exit code.

```bash
boxr wait <CONTAINER>
```

### `boxr init` / `boxr container init`
Initialize a container's filesystem and runtime structures without starting execution.

```bash
boxr init <CONTAINER>
boxr container init <CONTAINER>
```

### `boxr container clone`
Clone an existing container configuration, volume mounts, and rootfs into a new container:

```bash
boxr container clone [--run] <SOURCE> <TARGET>
```
- `--run`: Immediately start the cloned container.

### `boxr container cleanup`
Clean up container network bindings, mounts, and temporary storage:

```bash
boxr container cleanup [--all] [--rm] [<CONTAINER>]
```
- `--all`: Clean up all stopped containers.
- `--rm`: Remove container records after cleanup.

### Container Checkpoint Restoration via `boxr start`
Start a container from a pre-recorded checkpoint:

```bash
boxr start --checkpoint <CHECKPOINT_NAME> <CONTAINER>
boxr start --checkpoint-dir <DIR> <CONTAINER>
```

---

## 2. Container Operations

### `boxr container checkpoint`
Checkpoint a running container's state to disk or an exported archive:

```bash
boxr container checkpoint [--export <PATH>] [--keep] [--leave-running] <CONTAINER>
```
- `--export <PATH>`: Pack the checkpoint into a portable `.tar` archive.
- `--keep`: Retain checkpoint state files after restore.
- `-R, --leave-running`: Leave container running after checkpointing.

### `boxr container restore`
Restore a container from a checkpoint directory or tarball:

```bash
boxr container restore [--import <PATH>] [--keep] <CONTAINER>
```
- `--import <PATH>`: Import checkpoint from a portable `.tar` archive.

### `boxr container runlabel`
Execute commands defined in image labels (Podman parity):

```bash
boxr container runlabel <LABEL> <IMAGE>
```

### `boxr mount` / `boxr unmount` (Containers)
Mount a container's root filesystem and print the host directory mountpath, or unmount it:

```bash
# Mount container rootfs
boxr mount <CONTAINER>
boxr container mount <CONTAINER>

# Unmount container rootfs
boxr unmount <CONTAINER>
boxr container unmount <CONTAINER>
```

### `boxr container exists`
Check if a container exists (returns exit status `0` if present, non-zero error if missing):

```bash
boxr container exists <CONTAINER>
```

### `boxr logs`
Fetch the logs of a container.

```bash
boxr logs [OPTIONS] <CONTAINER>
```
- `-f, --follow`: Follow log output stream in real-time.
- `-t, --timestamps`: Prefix output with ISO-8601 timestamps.
- `-n, --tail <LINES>`: Number of lines to show from the end of logs.

### `boxr exec`
Run a command inside an active, running container.

```bash
boxr exec [-i] [-e <KEY=VAL>] <CONTAINER> <COMMAND>...
```

### `boxr attach`
Attach terminal standard input, output, and error streams to a running container.

```bash
boxr attach [--no-stdin] <CONTAINER>
```

### `boxr top`
Display running processes of a container.

```bash
boxr top <CONTAINER> [PS_ARGS]...
```

### `boxr diff`
Inspect changes to files and directories on a container's filesystem.
Output prefixes:
- `A`: Added
- `C`: Changed
- `D`: Deleted

```bash
boxr diff <CONTAINER>
```

### `boxr cp`
Copy files or directories between container and local host filesystem.

```bash
# Container to host
boxr cp web:/app/config.json ./config.json

# Host to container
boxr cp ./data.txt web:/data/data.txt
```

### `boxr inspect`
Return comprehensive low-level JSON information on a container or image.

```bash
boxr inspect <TARGET>
```

---

## 3. Image Management

### `boxr pull`
Pull an image from an OCI registry (Docker Hub, GHCR, Quay.io).

```bash
boxr pull [--platform <PLATFORM>] <IMAGE>
```

### `boxr build`
Build an image from a Dockerfile.

```bash
boxr build [-f <FILE>] [-t <TAG>] [--no-cache] <PATH>
```

### `boxr images`
List all locally available container images.

```bash
boxr images
```

### `boxr rmi`
Remove one or more images from local storage.

```bash
boxr rmi <IMAGE>
```

### `boxr save` / `boxr load`
Export and import multi-layer image tarball archives.

```bash
# Export
boxr save -o alpine.tar alpine:latest

# Import
boxr load -i alpine.tar
```

### `boxr login` / `boxr logout`
Authenticate with an OCI container registry.

```bash
boxr login [-u <USER>] [-p <PASS>] [<SERVER>]
boxr logout [<SERVER>]
```

### `boxr push`
Push an image to an OCI container registry.

```bash
boxr push <IMAGE>
```

### `boxr untag` / `boxr image untag`
Remove one or more tags from an image without deleting underlying layer blobs:

```bash
boxr untag <IMAGE> [TAGS...]
boxr image untag <IMAGE> [TAGS...]
```

### `boxr image diff`
Inspect differences between image layers or two images:

```bash
boxr image diff <IMAGE1> [IMAGE2]
```

### `boxr image scp`
Securely copy images between hosts over SSH:

```bash
boxr image scp [--quiet] <SOURCE> <DESTINATION>
```

### `boxr image sign`
Sign an image with cryptographic verification keys:

```bash
boxr image sign [--sign-by <IDENTITY>] <IMAGE>
```

### `boxr image tree`
Print image layer hierarchy tree:

```bash
boxr image tree [--whatrequires] <IMAGE>
```

### `boxr image trust`
Display or modify image trust validation policies:

```bash
# Show current registry trust policy
boxr image trust show [--raw] [REGISTRY]

# Set trust requirement for a registry (accept, reject, signedBy)
boxr image trust set --type accept docker.io
```

### `boxr image mount` / `boxr image unmount`
Mount an image rootfs read-only and return the host mount directory:

```bash
boxr image mount <IMAGE>
boxr image unmount <IMAGE>
```

### `boxr image exists`
Check if an image exists in the local image store:

```bash
boxr image exists <IMAGE>
```

---

## 4. Volumes & Networks

### `boxr volume`
Comprehensive volume management for stateful containers:

```bash
# Create a volume with optional driver, labels, and options
boxr volume create [--driver <DRIVER>] [--opt <KEY=VAL>] [--label <KEY=VAL>] <NAME>

# List volumes (supports --format, -q/--quiet, and --filter)
boxr volume ls [--quiet] [--format <TEMPLATE>] [--filter <KEY=VAL>]

# Inspect volume details
boxr volume inspect [--format <TEMPLATE>] <NAME>

# Remove a volume
boxr volume rm <NAME>

# Prune unused volumes
boxr volume prune [-a, --all] [--filter <KEY=VAL>]

# Export volume data to a tar archive
boxr volume export [--output <PATH.tar>] <NAME>

# Import volume data from a tar archive
boxr volume import <NAME> <PATH.tar>

# Reload volume configuration or storage drivers
boxr volume reload [NAME]

# Rename an existing volume
boxr volume rename <OLD_NAME> <NEW_NAME>

# Mount volume to host path or unmount
boxr volume mount <NAME>
boxr volume unmount <NAME>

# Check volume existence
boxr volume exists <NAME>
```

### `boxr network`
Rootless bridge, host, and user-mode network management:

```bash
# Create custom isolated bridge network
boxr network create [--subnet <CIDR>] [--gateway <IP>] [--internal] <NAME>

# List networks (supports -q/--quiet and --format)
boxr network ls [--quiet] [--format <TEMPLATE>]

# Inspect network configuration and connected container endpoints
boxr network inspect [--format <TEMPLATE>] <NAME>

# Connect / disconnect container to network
boxr network connect <NETWORK> <CONTAINER>
boxr network disconnect <NETWORK> <CONTAINER>

# Remove network
boxr network rm <NAME>

# Prune unused networks
boxr network prune

# Dynamically update network DNS servers and labels
boxr network update [--dns-add <IP>] [--dns-drop <IP>] [--label-add <KEY=VAL>] [--label-drop <KEY>] <NAME>

# Reload container port forwarders and network rules
boxr network reload <CONTAINER...>

# Check network existence
boxr network exists <NAME>
```

---

## 5. Pods & Kubernetes

### `boxr pod`
Manage Podman-compatible pods sharing network, IPC, UTS, and PID namespaces:

```bash
# Create a pod with shared port forwarding
boxr pod create --name web-pod -p 8080:80

# List pods
boxr pod ps [--quiet] [--format <TEMPLATE>]

# Inspect pod details and member containers
boxr pod inspect <POD>

# Stop / Start / Restart / Kill
boxr pod stop <POD>
boxr pod start <POD>
boxr pod restart <POD>
boxr pod kill <POD>

# Pause / Unpause
boxr pod pause <POD>
boxr pod unpause <POD>

# Top and live stats across pod containers
boxr pod top <POD>
boxr pod stats [--no-stream] <POD>

# Fetch aggregated logs across all containers in a pod
boxr pod logs [-f] [-t, --timestamps] <POD>

# Clone a pod and its member containers
boxr pod clone <SOURCE_POD> <TARGET_POD>

# Prune stopped pods
boxr pod prune

# Check if a pod exists
boxr pod exists <POD>

# Delete pod and member containers
boxr pod rm [-f] <POD>
```

### `boxr kube` (Modern Kubernetes Engine)
Manage Kubernetes resources with full multi-document YAML support:

```bash
# Play Kubernetes resources (Pod, Deployment with replicas, Service, Volume/PersistentVolumeClaim)
boxr kube play <FILE.yaml>
# Alternatively via legacy syntax:
boxr play kube <FILE.yaml>

# Tear down resources provisioned from Kubernetes YAML
boxr kube down <FILE.yaml>
boxr play kube --down <FILE.yaml>

# Generate Kubernetes YAML from existing container or pod
boxr kube generate <CONTAINER_OR_POD>
boxr generate kube <CONTAINER_OR_POD>

# Apply Kubernetes resources declaratively
boxr kube apply <FILE.yaml>
```

### `boxr unshare`
Run any command inside a clean rootless user namespace:

```bash
boxr unshare id
boxr unshare whoami
```

---

## 6. Quadlets (`boxr quadlet`)

Manage Podman Quadlet systemd unit files (`.container`, `.kube`, `.volume`, `.network`, `.artifact`):

```bash
# List all installed Quadlet unit files
boxr quadlet ls

# Install a Quadlet file into ~/.boxr/quadlets
boxr quadlet install <PATH_TO_UNIT_FILE>

# Print contents of an installed Quadlet file
boxr quadlet print <UNIT_NAME>

# Remove an installed Quadlet file
boxr quadlet rm <UNIT_NAME>
```

---

## 7. OCI Artifacts (`boxr artifact`)

Content-addressable OCI artifact management backed by SHA-256 digests:

```bash
# Add an artifact file to the artifact store
boxr artifact add --type <MIME_TYPE> <NAME> <FILE_PATH>

# Extract an artifact to a local destination directory
boxr artifact extract <NAME_OR_DIGEST> [DEST_DIR]

# Inspect detailed metadata for an artifact
boxr artifact inspect <NAME_OR_DIGEST>

# List all stored artifacts
boxr artifact ls

# Pull artifact from an OCI registry (stub)
boxr artifact pull <REGISTRY_REF>

# Push artifact to an OCI registry (stub)
boxr artifact push <REGISTRY_REF>

# Remove an artifact from the store
boxr artifact rm <NAME_OR_DIGEST>
```

---

## 8. Virtual Machines (`boxr machine`)

Manage native hypervisor virtual machines (macOS `Virtualization.framework` `boxr-vz` / Windows Hyper-V):

```bash
# Initialize a new virtual machine
boxr machine init [--now] [--rootful] [NAME]

# Start / Stop / Restart machine
boxr machine start [NAME]
boxr machine stop [NAME]
boxr machine restart [NAME]

# List machines and status
boxr machine ls

# Inspect machine configuration and socket paths
boxr machine inspect [NAME]

# Configure machine settings (e.g. rootful mode)
boxr machine set --rootful [NAME]

# Machine OS updates
boxr machine os check [NAME]
boxr machine os apply [NAME]

# Machine SSH shell or command execution
boxr machine ssh [NAME] [COMMAND...]

# Copy files between host and virtual machine
boxr machine cp <SOURCE> <DESTINATION>

# Reset virtual machine state
boxr machine reset [-f, --force]

# Remove virtual machine
boxr machine rm [-f] [NAME]
```

---

## 9. Secrets (`boxr secret`)

Manage encrypted and isolated application secrets:

```bash
# Create a secret from string, file, or stdin
boxr secret create [--file <FILE_OR_DASH>] <NAME>

# List secrets
boxr secret ls [--quiet] [--format <TEMPLATE>]

# Inspect secret metadata (data payload is never exposed in inspect)
boxr secret inspect <NAME>

# Remove secret
boxr secret rm [-f] <NAME...>

# Check if a secret exists
boxr secret exists <NAME>
```

---

## 10. Docker Compose

```bash
# Start multi-container stack
boxr compose [-f <COMPOSE_FILE>] up [-d] [--build]

# Service status
boxr compose ps

# Follow logs
boxr compose logs

# Build services
boxr compose build [--no-cache]

# Restart services
boxr compose restart

# Stop services
boxr compose stop

# Start services
boxr compose start

# Validate and view compose config
boxr compose config

# Stop and remove services
boxr compose down [-v] [--remove-orphans]
```

---

## 11. System & Utilities

### `boxr auto-update`
Inspect and apply automatic container updates based on registry image digests:

```bash
boxr auto-update [--dry-run]
```

### `boxr healthcheck`
Execute configured container health checks:

```bash
boxr healthcheck run <CONTAINER>
```

### `boxr generate spec` / `boxr spec`
Generate standardized container specifications:

```bash
# Generate Podman Specgen JSON (SpecGenerator)
boxr generate spec <CONTAINER>

# Generate standard OCI runtime spec (config.json)
boxr spec
```

### `boxr system` Operations
Comprehensive system maintenance and daemon management:

```bash
# Check system prerequisites and health
boxr system check

# Manage remote daemon connections
boxr system connection ls
boxr system connection add <NAME> <URI>
boxr system connection rm <NAME>
boxr system connection default <NAME>

# Audit disk usage across containers, images, volumes, and build cache
boxr system df [-v] [--format <TEMPLATE>]

# Reclaim unused storage
boxr system prune [-a, --all] [--volumes]

# Migrate storage between format versions
boxr system migrate

# Renumber container state files and locks
boxr system renumber

# Complete storage reset
boxr system reset [-f, --force]

# Run background API service daemon
boxr system service [--time <SECONDS>]

# Prepare Hyper-V virtualization prerequisites
boxr system hyperv-prep
```

### `boxr farm`
Farm out multi-architecture container image builds across remote nodes (Podman parity):

```bash
# Create a build farm
boxr farm create <FARM> [<CONNECTIONS>...]

# List configured build farms
boxr farm ls
boxr farm list

# Update an existing build farm
boxr farm update [--add <CONN>] [--remove <CONN>] [--default] <FARM>

# Remove one or all build farms
boxr farm rm <FARM>
boxr farm rm --all

# Build multi-architecture images and create manifest list
boxr farm build [--farm <FARM>] -t <IMAGE:TAG> [--platforms <P1,P2...>] [-f <FILE>] [<CONTEXT>]
```

### `boxr builder`
Manage image build cache:

```bash
# Display disk space consumed by build cache layers
boxr builder du

# Prune build cache
boxr builder prune [-a, --all] [-f, --force]
```

### `boxr service`
Manage the Boxr daemon as a native system service:

```bash
boxr service install
boxr service start
boxr service status
boxr service stop
boxr service uninstall
```

### `boxr alias`
Install optional Docker drop-in wrapper at `~/.boxr/bin/docker`:

```bash
boxr alias --install
```

### `boxr completion`
Generate shell autocompletions:

```bash
boxr completion bash
boxr completion zsh --install
boxr completion fish
```
