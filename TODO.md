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
