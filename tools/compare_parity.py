#!/usr/bin/env python3
"""
compare_parity.py: Compares Boxr CLI options against official Docker CLI options,
computes parity percentages across commands, and displays a formatted audit report.
"""

import sys
import os
import re
import json
import subprocess

# Import our helper modules
sys.path.insert(0, os.path.dirname(__file__))
from extract_boxr_options import find_boxr_binary, extract_all_boxr_options
from query_options import query_docker_command, get_auth_token

CORE_DOCKER_COMMANDS = [
    "run",
    "create",
    "exec",
    "build",
    "ps",
    "images",
    "stop",
    "start",
    "rm",
    "rmi",
    "logs",
    "inspect",
    "volume_create",
    "network_create",
    "context_create",
    "update",
]

# Daily-driver developer flags that 99% of Docker users and CI pipelines rely on
DAILY_DRIVER_FLAGS = {
    "run": [
        "--detach", "-d",
        "--interactive", "-i",
        "--tty", "-t",
        "--rm",
        "--name",
        "--publish", "-p",
        "--volume", "-v",
        "--env", "-e",
        "--env-file",
        "--workdir", "-w",
        "--user", "-u",
        "--entrypoint",
        "--hostname", "-h",
        "--network", "--net",
        "--restart",
        "--memory", "-m",
        "--cpus",
        "--pids-limit",
        "--dns",
        "--add-host",
        "--cidfile",
        "--label", "-l",
        "--init",
        "--tmpfs",
        "--security-opt",
        "--read-only",
        "--privileged",
        "--gpus",
        "--platform",
        "--shm-size",
        "--cap-add",
        "--cap-drop",
    ],
    "create": [
        "--name", "-p", "-v", "-e", "--env-file", "-w", "-u",
        "--network", "-m", "--cpus", "--label", "--cidfile"
    ],
    "exec": [
        "--interactive", "-i",
        "--tty", "-t",
        "--detach", "-d",
        "--workdir", "-w",
        "--user", "-u",
        "--env", "-e"
    ],
    "build": [
        "--tag", "-t",
        "--file", "-f",
        "--no-cache",
        "--build-arg",
        "--target"
    ],
    "ps": [
        "--all", "-a",
        "--quiet", "-q",
        "--no-trunc",
        "--last", "-n",
        "--latest", "-l",
        "--filter", "-f"
    ],
    "images": [
        "--quiet", "-q",
        "--all", "-a",
        "--filter", "-f"
    ],
    "stop": ["--time", "-t"],
    "start": ["--attach", "-a", "--interactive", "-i"],
    "rm": ["--force", "-f", "--volumes", "-v"],
    "rmi": ["--force", "-f"],
    "logs": ["--follow", "-f", "--timestamps", "-t", "--tail", "-n"],
    "inspect": ["--format", "-f", "--size", "-s"],
    "volume_create": ["--driver", "-d", "--opt", "-o", "--label"],
    "network_create": ["--driver", "-d", "--subnet", "--gateway", "--internal", "--attachable", "--label"],
    "context_create": ["--description", "--docker"],
    "update": ["--memory", "-m", "--cpus", "--pids-limit"],
}

def clean_flag_name(name_str):
    """Parse raw flag string into list of clean flags e.g. '-d, --detach' -> ['-d', '--detach']"""
    clean_str = re.sub(r"\[([^\]]+)\]\([^\)]+\)", r"\1", name_str)
    flags = []
    for part in clean_str.split(","):
        p = part.strip()
        if p.startswith("-"):
            flags.append(p.split()[0])
    return flags

def run_parity_comparison():
    boxr_bin = find_boxr_binary()
    if not boxr_bin:
        print("Error: Boxr binary not found. Run 'cargo build --release' first.")
        sys.exit(1)

    print(f"Comparing Boxr ({boxr_bin}) against Official Docker CLI Specification...")
    token = get_auth_token()
    boxr_data = extract_all_boxr_options(boxr_bin)

    report = []
    total_daily_flags = 0
    total_daily_matched = 0

    print("\n" + "=" * 80)
    print(f"{'COMMAND':<16} {'DAILY DRIVER PARITY':<24} {'UPSTREAM TOTAL':<16} {'BOXR COUNT'}")
    print("=" * 80)

    for cmd in CORE_DOCKER_COMMANDS:
        clean_cmd = cmd.replace("_", " ")
        docker_opts = query_docker_command(cmd, token) or []
        
        # Flatten all Docker flags
        docker_flags_set = set()
        for o in docker_opts:
            for f in clean_flag_name(o.get("name", "")):
                docker_flags_set.add(f)

        # Get Boxr flags for this command
        boxr_flags_set = set()
        boxr_opts = {}
        try:
            cmd_parts = clean_cmd.split()
            out = subprocess.check_output([boxr_bin] + cmd_parts + ["--help"], stderr=subprocess.DEVNULL).decode("utf-8")
            from extract_boxr_options import parse_help_options
            boxr_opts = parse_help_options(out)
            for opt_key, opt_data in boxr_opts.items():
                boxr_flags_set.add(opt_key)
                if opt_data.get("short"):
                    boxr_flags_set.add(opt_data["short"])
        except Exception:
            boxr_info = boxr_data.get(clean_cmd.split()[0], {})
            boxr_opts = boxr_info.get("options", {})
            for opt_key, opt_data in boxr_opts.items():
                boxr_flags_set.add(opt_key)
                if opt_data.get("short"):
                    boxr_flags_set.add(opt_data["short"])

        if "--network" in boxr_flags_set:
            boxr_flags_set.add("--net")

        # Check Daily Driver parity
        expected_daily = DAILY_DRIVER_FLAGS.get(cmd, [])
        if expected_daily:
            matched_daily = [f for f in expected_daily if f in boxr_flags_set]
            pct = (len(matched_daily) / len(expected_daily)) * 100.0
            total_daily_flags += len(expected_daily)
            total_daily_matched += len(matched_daily)
            parity_str = f"{len(matched_daily)}/{len(expected_daily)} ({pct:.0f}%)"
            bar_len = int(pct / 10)
            progress_bar = "[" + "#" * bar_len + "-" * (10 - bar_len) + "]"
            parity_display = f"{progress_bar} {parity_str}"
        else:
            parity_display = "N/A"

        print(f"docker {clean_cmd:<9} {parity_display:<24} {len(docker_opts):<16} {len(boxr_opts)}")

        report.append({
            "command": clean_cmd,
            "docker_total_options": len(docker_opts),
            "boxr_options_count": len(boxr_opts),
            "boxr_flags": sorted(list(boxr_flags_set)),
            "daily_driver_parity": parity_display
        })

    print("=" * 80)
    overall_pct = (total_daily_matched / total_daily_flags) * 100.0 if total_daily_flags else 100.0
    print(f"OVERALL DAILY-DRIVER CLI PARITY: {total_daily_matched}/{total_daily_flags} ({overall_pct:.1f}% MATCH)")
    print("=" * 80)

    # Save detailed JSON report
    report_path = "PARITY_REPORT.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"\nDetailed JSON report written to: {report_path}")

if __name__ == "__main__":
    run_parity_comparison()
