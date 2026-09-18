#!/usr/bin/env bash
# Create GitHub issues from audited bug reports in .github/issue-drafts/
# Requires: gh CLI authenticated with issues:write scope
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DRAFTS_DIR="$REPO_ROOT/.github/issue-drafts"

ISSUES=(
  "01-cp-path-traversal.md|[Security] boxr cp allows path traversal outside container rootfs|bug"
  "02-volume-path-traversal.md|[Security] Volume bind mounts allow .. path traversal to host directories|bug"
  "03-tar-slip-load-import.md|[Security] Tar-slip vulnerability in boxr load and import|bug"
  "04-privileged-cap-flags-noop.md|[Security] --privileged, --cap-add, and --cap-drop flags are no-ops|bug"
  "05-exec-fallback-silent-success.md|[Bug] docker exec fallback silently reports success on failure|bug"
  "06-overlay-hardlink-cow-corruption.md|[Bug] Overlay hardlink fallback corrupts shared base image layers|bug"
  "07-registry-pull-ignores-credentials.md|[Bug] boxr pull ignores stored registry credentials|bug"
  "08-restart-missing-port-forwarding.md|[Bug] Restarted containers lose published port forwarding|bug"
  "09-compose-up-swallows-errors.md|[Bug] compose up ignores service start failures|bug"
  "10-cached-blob-skips-digest-verification.md|[Bug] Cached registry blobs skip SHA-256 verification|bug"
  "11-json-store-race-condition.md|[Bug] JSON metadata stores have race conditions on concurrent writes|bug"
  "12-seccomp-runtime-enforcement.md|[Security] Default seccomp filter not enforced at runtime|bug"
  "13-absolute-volume-traversal.md|[Security] Absolute paths with .. bypass volume traversal guard|bug"
)

if ! command -v gh >/dev/null 2>&1; then
  echo "Error: gh CLI is required" >&2
  exit 1
fi

created=0
failed=0

for item in "${ISSUES[@]}"; do
  IFS="|" read -r file title label <<< "$item"
  body_file="$DRAFTS_DIR/$file"
  echo "Creating: $title"
  if [[ "${CREATE_ISSUE_LABELS:-0}" == "1" ]]; then
    if gh issue create -t "$title" -F "$body_file" -l "$label"; then
      created=$((created + 1))
    else
      echo "Failed to create issue for $file" >&2
      failed=$((failed + 1))
    fi
  else
    if gh issue create -t "$title" -F "$body_file"; then
      created=$((created + 1))
    else
      echo "Failed to create issue for $file" >&2
      failed=$((failed + 1))
    fi
  fi
done

echo ""
echo "Done. Created: $created, Failed: $failed"
exit $(( failed > 0 ? 1 : 0 ))
