#!/usr/bin/env bash
# Run the full boxr test suite using all CPU cores and parallel VM workers.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

NCPU="$(sysctl -n hw.logicalcpu 2>/dev/null || nproc 2>/dev/null || echo 4)"
MEM_GB="$(sysctl -n hw.memsize 2>/dev/null | awk '{print int($1/1024/1024/1024)}' || echo 16)"
VM_WORKERS=$((MEM_GB / 4))
MAX_VM=$(( (NCPU + 1) / 2 ))
if ((VM_WORKERS > MAX_VM)); then VM_WORKERS=$MAX_VM; fi
if ((VM_WORKERS > 4)); then VM_WORKERS=4; fi
if ((VM_WORKERS < 2)); then VM_WORKERS=2; fi

export CARGO_BUILD_JOBS="$NCPU"
export BOXR_TEST_FAST=1
export BOXR_VM_CPUS="${BOXR_VM_CPUS:-4}"
export BOXR_VM_MEMORY_MB="${BOXR_VM_MEMORY_MB:-4096}"

LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/boxr-test-fast.XXXXXX")"
REAL_BOXR_HOME="${HOME}/.boxr"
FAILED=0

echo "==> boxr fast test runner"
echo "    CPUs: $NCPU  |  VM workers: $VM_WORKERS  |  VM: ${BOXR_VM_CPUS} vCPU, ${BOXR_VM_MEMORY_MB} MiB"
echo "    Logs: $LOG_DIR"

test_binary() {
  local name="$1"
  find target/release/deps -maxdepth 1 -name "${name}-*" ! -name '*.d' -type f -perm -111 | head -1
}

setup_worker_home() {
  local home
  home="$(mktemp -d "${TMPDIR:-/tmp}/boxr-home.XXXXXX")"
  for d in images layers vm; do
    [[ -d "$REAL_BOXR_HOME/$d" ]] && ln -sf "$REAL_BOXR_HOME/$d" "$home/$d"
  done
  if [[ -d "$REAL_BOXR_HOME/bin" ]]; then
    cp -R "$REAL_BOXR_HOME/bin" "$home/bin"
  fi
  [[ -f "$REAL_BOXR_HOME/images.json" ]] && cp "$REAL_BOXR_HOME/images.json" "$home/images.json"
  echo "$home"
}

run_cpu_test() {
  local name="$1" threads="$2"
  local bin log
  bin="$(test_binary "$name")"
  log="$LOG_DIR/${name}.log"
  [[ -n "$bin" ]] || { echo "missing binary for $name" >&2; return 1; }
  "$bin" --test-threads="$threads" >"$log" 2>&1
}

run_vm_test() {
  local name="$1"
  local bin log home
  bin="$(test_binary "$name")"
  log="$LOG_DIR/${name}.log"
  [[ -n "$bin" ]] || { echo "missing binary for $name" >&2; return 1; }
  home="$(setup_worker_home)"
  if ! env BOXR_HOME="$home" "$bin" --test-threads=1 >"$log" 2>&1; then
    rm -rf "$home"
    return 1
  fi
  rm -rf "$home"
}

run_pool() {
  local kind="$1" threads="$2"
  shift 2
  local tests=("$@")
  local pids=() limit
  if [[ "$kind" == vm ]]; then limit=$VM_WORKERS; else limit=$NCPU; fi
  for t in "${tests[@]}"; do
    (
      if [[ "$kind" == vm ]]; then run_vm_test "$t"; else run_cpu_test "$t" "$threads"; fi
      echo "  OK  $t"
    ) || echo "  FAIL $t" &
    pids+=($!)
    while ((${#pids[@]} >= limit)); do
      if ! wait "${pids[0]}"; then FAILED=1; fi
      pids=("${pids[@]:1}")
    done
  done
  for pid in "${pids[@]}"; do
    if ! wait "$pid"; then FAILED=1; fi
  done
}

echo "==> build all tests once (release, -j $NCPU)"
cargo test --tests --release --no-run -j "$NCPU"

echo "==> unit tests (lib, $NCPU threads)"
cargo test --lib --release -j "$NCPU" -- --test-threads="$NCPU"

CPU_TESTS=(
    integration_test issues_47_to_111_test issues_113_to_162_test
    issues_343_to_391_test
    pasta_test podman_parity_test popular_softwares_matrix_test
    qa_dockerfile_and_compose_test qa_e2e_cli_negative_test qa_reference_test
    qa_security_and_system_test qa_volume_and_network_test usernet_test
    vagrant_test_suite
)

VM_TESTS=(
    blackbox_negative_test blackbox_networking_test blackbox_resources_test
    blackbox_runtime_test blackbox_security_test blackbox_stress_test
    blackbox_volumes_test docker_parity_test e2e_test enterprise_scenarios_test
)

echo "==> CPU-only tests (${#CPU_TESTS[@]} files, $NCPU parallel)"
run_pool cpu "$NCPU" "${CPU_TESTS[@]}"

echo "==> serial async issue tests"
if run_cpu_test issues_163_to_342_test 1; then
  echo "  OK  issues_163_to_342_test"
else
  echo "  FAIL issues_163_to_342_test"
  FAILED=1
fi

echo "==> VM tests (${#VM_TESTS[@]} files, $VM_WORKERS parallel)"
run_pool vm 1 "${VM_TESTS[@]}"

if ((FAILED != 0)); then
    echo "==> FAILED — logs in $LOG_DIR"
    for f in "$LOG_DIR"/*.log; do
      if grep -qE 'FAILED|panicked' "$f" 2>/dev/null; then
        echo "--- $(basename "$f" .log) ---"
        tail -8 "$f"
      fi
    done
    exit 1
fi

echo "==> ALL TESTS PASSED"
rm -rf "$LOG_DIR"
