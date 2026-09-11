#!/usr/bin/env python3
"""
boxr Feature & Command Parity Checker
Compares boxr against standard Docker CLI and Podman CLI suites.
"""

import os
import sys
import shutil
import subprocess
import argparse
from typing import Dict, Set, List, Tuple

# Canonical Docker commands grouped by functional category
DOCKER_CORE_COMMANDS: Dict[str, Dict[str, str]] = {
    "Container Lifecycle": {
        "run": "Run a command in a new container",
        "ps": "List containers",
        "start": "Start one or more stopped containers",
        "stop": "Stop one or more running containers",
        "restart": "Restart one or more containers",
        "kill": "Kill one or more running containers",
        "rm": "Remove one or more containers",
        "pause": "Pause all processes within one or more containers",
        "unpause": "Unpause all processes within one or more containers",
        "wait": "Block until container stops, print exit code",
        "rename": "Rename a container",
        "attach": "Attach local standard input/output/error streams to container",
        "exec": "Run a command in a running container",
        "logs": "Fetch the logs of a container",
        "top": "Display the running processes of a container",
        "diff": "Inspect changes to files on container filesystem",
        "cp": "Copy files/folders between container and local filesystem",
        "update": "Update configuration of one or more containers",
        "inspect": "Return low-level information on Boxr/Docker objects",
        "port": "List port mappings or a specific mapping for the container",
        "create": "Create a new container without starting it",
    },
    "Image Operations": {
        "build": "Build an image from a Dockerfile",
        "images": "List images",
        "pull": "Download an image from a registry",
        "push": "Upload an image to a registry",
        "rmi": "Remove one or more images",
        "tag": "Create a tag that refers to source_image",
        "commit": "Create a new image from a container's changes",
        "save": "Save one or more images to a tar archive",
        "load": "Load an image from a tar archive",
        "import": "Import the contents from a tarball to create an image",
        "export": "Export a container's filesystem as a tar archive",
        "history": "Show the history of an image",
        "search": "Search Docker Hub for images",
    },
    "Orchestration & Resources": {
        "compose": "Multi-container application orchestration",
        "volume": "Manage volumes",
        "network": "Manage networks",
        "builder": "Manage builds and build cache",
        "system": "Manage Docker / Boxr system (df, prune)",
        "daemon": "Run background engine API daemon",
    },
    "Authentication & Info": {
        "login": "Log in to an OCI registry",
        "logout": "Log out from an OCI registry",
        "version": "Show engine version information",
        "info": "Display system-wide information",
        "events": "Get real time events from the server",
        "stats": "Display a live stream of container resource usage statistics",
    },
    "Docker Enterprise / Swarm Extensions": {
        "context": "Manage contexts",
        "manifest": "Manage Docker image manifests and manifest lists",
        "plugin": "Manage plugins",
        "swarm": "Manage Swarm cluster",
        "node": "Manage Swarm nodes",
        "service": "Manage Swarm services",
        "secret": "Manage sensitive data",
        "config": "Manage configurations",
    }
}

PODMAN_SPECIALIZED_COMMANDS: Dict[str, str] = {
    "pod": "Manage pods (create, rm, start, stop, pause, ps, inspect)",
    "generate": "Generate structured artifacts (generate kube, generate systemd)",
    "play": "Play Kubernetes YAML pods and deployments (play kube)",
    "unshare": "Run a command inside a user namespace",
    "auto-update": "Auto-update containers according to registry labels",
    "healthcheck": "Manage container health checks via dedicated CLI",
    "machine": "Manage Podman virtual machines",
    "kube": "Deploy, inspect, or manage Kubernetes workloads",
    "farm": "Build multi-architecture images across farm nodes",
}

def find_boxr_binary() -> str:
    candidates = [
        os.path.join(os.path.dirname(os.path.dirname(__file__)), "target", "release", "boxr"),
        os.path.join(os.path.dirname(os.path.dirname(__file__)), "target", "debug", "boxr"),
        os.path.expanduser("~/.boxr/bin/boxr"),
        shutil.which("boxr"),
    ]
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    return ""

def extract_boxr_commands(binary_path: str) -> Set[str]:
    if not binary_path:
        return set()
    try:
        proc = subprocess.run([binary_path, "--help"], capture_output=True, text=True, check=True)
        lines = proc.stdout.splitlines()
        commands = set()
        in_commands_section = False
        for line in lines:
            line_str = line.strip()
            if line_str == "Commands:":
                in_commands_section = True
                continue
            if in_commands_section:
                if line_str.startswith("Options:") or not line_str:
                    if line_str.startswith("Options:"):
                        break
                    continue
                parts = line_str.split(None, 1)
                if parts:
                    cmd_name = parts[0]
                    if not cmd_name.startswith("-") and cmd_name != "help":
                        commands.add(cmd_name)
        # Also version/info are supported as flags/endpoints
        commands.add("version")
        return commands
    except Exception as e:
        print(f"Error querying boxr binary: {e}", file=sys.stderr)
        return set()

def main():
    parser = argparse.ArgumentParser(description="Analyze command parity: boxr vs Docker and Podman")
    parser.add_argument("--markdown", action="store_true", help="Output summary in Markdown format")
    parser.add_argument("--json", action="store_true", help="Output summary as JSON")
    args = parser.parse_args()

    binary_path = find_boxr_binary()
    boxr_cmds = extract_boxr_commands(binary_path)

    # Flatten Docker commands by core vs enterprise
    core_docker_total = {}
    enterprise_docker_total = {}
    for category, cmds in DOCKER_CORE_COMMANDS.items():
        if "Enterprise" in category:
            enterprise_docker_total.update(cmds)
        else:
            core_docker_total.update(cmds)

    core_implemented = {c for c in core_docker_total if c in boxr_cmds}
    core_missing = {c for c in core_docker_total if c not in boxr_cmds}

    enterprise_implemented = {c for c in enterprise_docker_total if c in boxr_cmds}
    enterprise_missing = {c for c in enterprise_docker_total if c not in boxr_cmds}

    podman_implemented = {c for c in PODMAN_SPECIALIZED_COMMANDS if c in boxr_cmds}
    podman_missing = {c for c in PODMAN_SPECIALIZED_COMMANDS if c not in boxr_cmds}

    core_parity_pct = (len(core_implemented) / len(core_docker_total)) * 100

    if args.json:
        import json
        payload = {
            "binary_path": binary_path,
            "boxr_commands_count": len(boxr_cmds),
            "docker_core_parity_percentage": round(core_parity_pct, 1),
            "docker_core_implemented": sorted(list(core_implemented)),
            "docker_core_missing": sorted(list(core_missing)),
            "docker_enterprise_missing": sorted(list(enterprise_missing)),
            "podman_specialized_missing": sorted(list(podman_missing)),
        }
        print(json.dumps(payload, indent=2))
        return

    print("=" * 72)
    print("           BOXR vs DOCKER & PODMAN COMMAND PARITY REPORT         ")
    print("=" * 72)
    print(f"Target Binary: {binary_path or 'Not built yet (using static inspection)'}")
    print(f"Total Boxr Implemented Commands: {len(boxr_cmds)}")
    print(f"Docker Core CLI Parity:          {core_parity_pct:.1f}% ({len(core_implemented)}/{len(core_docker_total)} commands)")
    print("-" * 72)

    # Category breakdown
    for category, cmds in DOCKER_CORE_COMMANDS.items():
        cat_implemented = [c for c in cmds if c in boxr_cmds]
        print(f"\n📂 {category.upper()} ({len(cat_implemented)}/{len(cmds)})")
        for cmd, desc in sorted(cmds.items()):
            status = "✅ [IMPLEMENTED]" if cmd in boxr_cmds else "❌ [MISSING]    "
            print(f"  {status} {cmd:<14} - {desc}")

    print("\n" + "=" * 72)
    print("🦭 PODMAN SPECIALIZED FEATURES (Kubernetes & Pod Extensions)")
    print("=" * 72)
    for cmd, desc in sorted(PODMAN_SPECIALIZED_COMMANDS.items()):
        status = "✅ [IMPLEMENTED]" if cmd in boxr_cmds else "💡 [PODMAN EXT] "
        print(f"  {status} {cmd:<14} - {desc}")

    print("\n" + "=" * 72)
    print("🎯 ACTIONABLE ROADMAP - NEXT COMMANDS TO BUILD FOR 100% PARITY")
    print("=" * 72)
    priority_order = [
        ("tag", "Image", "Aliasing local images (boxr tag <src> <target>)"),
        ("port", "Container", "Print container port forwardings (boxr port <container>)"),
        ("restart", "Container", "Restart a running or stopped container (boxr restart <container>)"),
        ("create", "Container", "Create container without immediately starting (boxr create <image>)"),
        ("export", "Container", "Export raw container rootfs to tarball (boxr export <container>)"),
        ("import", "Image", "Import raw rootfs tarball as new image (boxr import <tar>)"),
        ("history", "Image", "Show layer history of an image (boxr history <image>)"),
        ("search", "Registry", "Search Docker Hub for images from CLI (boxr search <term>)"),
        ("pod", "Podman", "Multi-container Pod abstraction sharing namespaces (boxr pod)"),
        ("play", "Podman", "Run Kubernetes pod YAML specifications (boxr play kube)"),
    ]

    for cmd, kind, detail in priority_order:
        if cmd in core_missing or cmd in podman_missing:
            print(f"  👉 {cmd:<12} [{kind:<9}] : {detail}")
    print("=" * 72)

if __name__ == "__main__":
    main()
