# Boxr CLI Command Reference 📖

Complete reference manual for all commands, flags, and options supported by `boxr`.

---

## Command Overview

| Category | Commands |
| :--- | :--- |
| **Container Lifecycle** | `run`, `create`, `start`, `stop`, `restart`, `pause`, `unpause`, `kill`, `rm`, `ps`, `wait` |
| **Container Operations** | `logs`, `exec`, `attach`, `top`, `diff`, `cp`, `rename`, `update`, `commit`, `inspect` |
| **Image Management** | `pull`, `build`, `save`, `load`, `push`, `tag`, `import`, `export`, `history`, `images`, `rmi`, `search` |
| **Volumes** | `volume create`, `volume ls`, `volume inspect`, `volume rm` |
| **Networks** | `network create`, `network ls`, `network inspect`, `network connect`, `network disconnect`, `network rm` |
| **Pods & Kubernetes** | `pod create`, `pod ps`, `pod inspect`, `pod stop`, `pod start`, `pod rm`, `play kube`, `generate kube`, `unshare` |
| **Compose** | `compose up`, `compose down`, `compose ps`, `compose logs` |
| **Daemon & Service** | `daemon`, `service install`, `service start`, `service stop`, `service status`, `service uninstall` |
| **System & Utilities** | `info`, `version`, `system df`, `system prune`, `completion`, `alias`, `spec` |

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

---

## 2. Container Operations

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

---

## 4. Volumes & Networks

### `boxr volume`
```bash
boxr volume create <NAME>
boxr volume ls
boxr volume inspect <NAME>
boxr volume rm <NAME>
```

### `boxr network`
```bash
boxr network create [--subnet <CIDR>] [--gateway <IP>] <NAME>
boxr network ls
boxr network inspect <NAME>
boxr network connect <NETWORK> <CONTAINER>
boxr network disconnect <NETWORK> <CONTAINER>
boxr network rm <NAME>
```

---

## 5. Pods & Kubernetes

### `boxr pod`
```bash
# Create a pod sharing IPC and network
boxr pod create --name web-pod -p 8080:80

# List pods
boxr pod ps

# Inspect pod details
boxr pod inspect <POD>

# Stop / Start / Delete
boxr pod stop <POD>
boxr pod start <POD>
boxr pod rm <POD>
```

### `boxr play kube`
Deploy a Kubernetes Pod YAML directly:

```bash
boxr play kube pod.yaml
```

### `boxr generate kube`
Generate Kubernetes Pod YAML for an existing container or pod:

```bash
boxr generate kube my-container
```

### `boxr unshare`
Run any command inside a clean rootless user namespace:

```bash
boxr unshare id
boxr unshare whoami
```

---

## 6. Docker Compose

```bash
# Start multi-container stack
boxr compose [-f <COMPOSE_FILE>] up [-d] [--build]

# Service status
boxr compose ps

# Follow logs
boxr compose logs

# Stop and remove services
boxr compose down [-v]
```

---

## 7. System & Services

### `boxr system df` / `boxr system prune`
Audit and reclaim disk space:

```bash
# Audit disk usage
boxr system df

# Reclaim unused storage
boxr system prune [-a] [--volumes]
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
