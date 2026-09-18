# Boxr Production Parity Roadmap & Todo List

Tracked tasks to reach 100% production readiness and drop-in parity with Docker / Podman:

- [x] **1. CLI Exec Flags (`cli-exec-flags`)**
  - Added `-t / --tty` (terminal allocation) to `docker exec`.
  - Added `-w / --workdir` (working directory inside container) to `docker exec`.
  - Added `-u / --user` (username/UID) to `docker exec`.
  - Added `-d / --detach` (background execution) to `docker exec`.
  - Verified with integration test `test_docker_parity_exec_flags` and `run_parity_tests.sh`.

- [x] **2. CLI Run & Create Flags (`cli-run-flags`)**
  - Added `-m` shorthand for `--memory`.
  - Added `-l / --label <key=val>` (container metadata labels).
  - Added `--dns <ip>` (custom nameservers in container `/etc/resolv.conf`).
  - Added `--cidfile <path>` (write container ID to file).
  - Verified with integration test `test_docker_parity_run_flags` and `run_parity_tests.sh`.

- [x] **3. CLI Ps Flags (`cli-ps-flags`)**
  - Added `-n / --last <n>` (show n last created containers).
  - Added `-l / --latest` (show latest created container).
  - Added `-f / --filter <filter>` (filter by status, name, ancestor).
  - Verified with integration test `test_docker_parity_ps_flags` and `run_parity_tests.sh`.

- [x] **4. CLI Images Flags (`cli-images-flags`)**
  - Added `-q / --quiet` (only display numeric image IDs).
  - Added `-a / --all` (show all images).
  - Added `-f / --filter <filter>` (filter images by reference, label, etc.).
  - Verified with integration test `test_docker_parity_images_flags` and `run_parity_tests.sh`.

- [x] **5. Docker Management Command Groups (`mgmt-subcommands`)**
  - Implemented `docker container <subcommand>` (`ls`, `run`, `start`, `stop`, `rm`, `inspect`, `logs`, `exec`, `prune`, `kill`, `pause`, `unpause`, `top`, `port`, `cp`, `diff`, `wait`).
  - Implemented `docker image <subcommand>` (`ls`, `build`, `pull`, `push`, `tag`, `rm`, `inspect`, `history`, `save`, `load`, `prune`).
  - Verified with integration test `test_docker_parity_management_subcommands` and `run_parity_tests.sh`.

- [x] **6. Prune Commands (`prune-commands`)**
  - Implemented `docker container prune [-f/--force]`.
  - Implemented `docker image prune [-a/--all] [-f/--force]`.
  - Implemented `docker network prune [-f/--force]`.
  - Implemented `docker volume prune [-f/--force]`.
  - Verified with section 12 of `run_parity_tests.sh`.

- [x] **7. Dockerfile Directives & Build Flags (`builder-directives`)**
  - Supported `ARG <name>[=<default>]` in Dockerfile parser & executor.
  - Supported `USER <uid|name>` in Dockerfile and image config.
  - Supported `VOLUME ["/path"]` in Dockerfile and image config.
  - Added `--build-arg <KEY=VAL>` to `docker build`.
  - Added `--target <stage>` multi-stage targeting to `docker build`.
  - Verified with integration test `test_docker_parity_builder_directives`.

- [x] **8. Docker Engine REST API Expansion (`daemon-rest-api`)**
  - `POST /containers/prune`, `POST /images/prune`, `POST /volumes/prune`, `POST /networks/prune`.
  - `GET /networks/{id}`, `DELETE /networks/{id}`.
  - `GET /volumes/{name}`, `DELETE /volumes/{name}`.
  - Verified with unit test `test_daemon_prune_and_crud_endpoints` and section 10 of `run_parity_tests.sh`.

- [x] **9. Interactive PTY & Terminal Signal Handling (`interactive-pty`)**
  - Full raw-mode PTY passthrough forwarding terminal size / `SIGWINCH` resize events.
  - Signal forwarding (`SIGINT`, `SIGTERM`, `SIGWINCH`) for interactive container sessions.
  - Verified with `TerminalGuard` lifecycle in `test_docker_parity_exec_flags`.

- [x] **10. Real Container Execution & Exec in REST API (`daemon-real-exec`)**
  - Wired `POST /v1.45/containers/create` to full bundle CoW creation.
  - Wired `POST /v1.45/containers/{id}/start` and `POST /v1.45/containers/{id}/stop` to actual hypervisor/cgroup lifecycle.
  - Implemented `POST /v1.45/containers/{id}/exec`, `POST /v1.45/exec/{id}/start`, and `GET /v1.45/exec/{id}/json`.
  - Verified with `test_daemon_prune_and_crud_endpoints` and section 10 of `run_parity_tests.sh`.

- [x] **11. Docker Context Management (`cli-context-cmds`)**
  - Implemented `docker context ls`, `show`, `use`, `inspect`, `create`, `rm`.
  - Verified with integration test `test_docker_parity_context_commands` and section 15 of `run_parity_tests.sh`.

- [x] **12. Container Init Subreaper Process (`cli-init-flag`)**
  - Added `--init` flag to `docker run` and `docker create`.
  - Enabled Linux `PR_SET_CHILD_SUBREAPER` and guest init supervision.
  - Verified with integration test `test_docker_parity_run_init` and section 17 of `run_parity_tests.sh`.

- [x] **13. Docker Hub & Registry Credential Fallback (`auth-docker-fallback`)**
  - Seamlessly fall back to `~/.docker/config.json` when `~/.boxr/config.json` has no credentials for a registry.

- [x] **14. Native Process Resource Metrics in `docker stats` (`stats-real-metrics`)**
  - Added real process CPU% and RSS memory sampling via PID on Unix/macOS when cgroup v2 controllers are absent.

- [x] **15. Multi-Arch Manifest Subcommands (`cli-manifest-cmds`)**
  - Implemented `docker manifest inspect`, `create`, and `push`.
  - Verified with integration test `test_docker_parity_manifest_commands` and section 16 of `run_parity_tests.sh`.

- [x] **16. Advanced Compose v2 Directives (`compose-advanced-directives`)**
  - Supported `container_name`, single and multi-file `env_file`, and `restart` policies in `docker-compose.yml`.
  - Verified with integration test `test_docker_parity_compose_advanced`.

- [x] **17. Multi-Tag Image Builds (`builder-multi-tags`)**
  - Supported multiple `-t / --tag` flags in `docker build` to tag images with multiple references in one build.
  - Verified with integration test `test_docker_parity_builder_multi_tags` and section 18 of `run_parity_tests.sh`.

- [x] **18. Advanced Runtime Isolation Flags (`cli-runtime-flags`)**
  - Added `--tmpfs <path[:options]>`, `--device`, and `--security-opt` (`seccomp=unconfined`, `no-new-privileges:true`) to `docker run/create`.
  - Verified with integration test `test_docker_parity_runtime_flags` and section 19 of `run_parity_tests.sh`.

- [x] **19. CPU & Memory Resource Restrictions & Dynamic Updates (`cgroups-restrictions`)**
  - Bound child container processes to cgroups v2/v1 hierarchy (`add_process` to `cgroup.procs` and `tasks`).
  - Supported memory limits (`-m / --memory`), CPU quotas (`--cpus`), and PID limits (`--pids-limit`).
  - Enabled dynamic updates via `docker update --memory ... --cpus ... <container>`.
  - Passed `--memory` and `--cpus` to Apple `Virtualization.framework` microVMs on macOS.
  - Verified with integration test `test_docker_parity_resource_limits` and section 20 of `run_parity_tests.sh`.

- [x] **20. Docker Exec Advanced Flags (`exec-env-file-privileged`)**
  - Added `--env-file` to `docker exec`.
  - Added `--privileged` to `docker exec`.
  - Verified with integration test `test_docker_parity_exec_env_file` and section 21 of `run_parity_tests.sh`.

- [x] **21. Docker Logs Time Filtering (`logs-since-until`)**
  - Added `--since <timestamp>`, `--until <timestamp>`, and `--details` to `docker logs`.
  - Verified with integration test and `run_parity_tests.sh`.

- [x] **22. Docker Ps Formatting & Sizing (`ps-format-size`)**
  - Added `--format <format>` (json, table, Go template) to `docker ps`.
  - Added `-s / --size` to display container disk sizes.
  - Verified with integration test `test_docker_parity_ps_format_and_size` and section 21 of `run_parity_tests.sh`.

- [x] **23. Advanced CPU & Memory Resource Constraints (`run-advanced-resource-flags`)**
  - Added `-c / --cpu-shares <shares>` (relative CPU weight).
  - Added `--cpuset-cpus <cpus>` (pin execution to specific CPU cores).
  - Added `--memory-swap <swap>` (total memory + swap limit).
  - Added `--memory-reservation <reservation>` (soft memory limit).
  - Verified with integration test `test_docker_parity_advanced_run_options` and section 21 of `run_parity_tests.sh`.

- [x] **24. Advanced Network & Kernel Namespace Options (`run-dns-network-opts`)**
  - Added `--dns-search <domain>` and `--dns-option <opt>`.
  - Added `--expose <port>` (expose port without host publishing).
  - Added `--sysctl <key=val>` (configure namespaced kernel parameters).
  - Verified with integration test and `run_parity_tests.sh`.

- [x] **25. Lifecycle Signals, Annotations & Limits (`run-lifecycle-signals`)**
  - Added `--stop-timeout <seconds>` and `--stop-signal <sig>`.
  - Added `--annotation <key=val>` (pass OCI runtime annotations).
  - Added `--ulimit <type=soft:hard>`.
  - Verified with integration test `test_docker_parity_advanced_run_options` and section 21 of `run_parity_tests.sh`.

- [x] **26. Advanced Builder Resource Flags (`build-resource-flags`)**
  - Added `--add-host <host:ip>`, `--memory <bytes>`, `--shm-size <size>`, `--rm` to `docker build`.
  - Verified with integration test `test_docker_parity_builder_directives` and section 18 of `run_parity_tests.sh`.
