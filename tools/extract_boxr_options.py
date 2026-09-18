#!/usr/bin/env python3
"""
extract_boxr_options.py: Programmatically extracts all commands, subcommands,
and options supported by the boxr binary using Clap's introspective --help.
"""

import sys
import os
import re
import json
import subprocess

def find_boxr_binary():
    candidates = [
        "target/release/boxr",
        "target/debug/boxr",
        os.path.expanduser("~/.boxr/bin/boxr"),
        "/usr/local/bin/boxr"
    ]
    for c in candidates:
        if os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    try:
        out = subprocess.check_output(["which", "boxr"], stderr=subprocess.DEVNULL)
        return out.decode().strip()
    except Exception:
        pass
    return None

def parse_help_options(help_text):
    """Parse option flags and descriptions from Clap help output."""
    options = {}
    lines = help_text.splitlines()
    in_options = False

    current_flag = None
    current_desc = []

    for line in lines:
        if line.strip() in ("Options:", "Arguments:"):
            in_options = True
            continue
        if in_options:
            if line.strip().startswith("Commands:"):
                break
            
            # Match flag pattern: e.g. "  -d, --detach             Run in background"
            # or "      --env-file <ENV_FILE>  Read in a file..."
            match = re.match(r"^\s+(?:(-[a-zA-Z0-9]),\s+)?(--[a-zA-Z0-9\-]+)(?:\s+<[^>]+>)?(?:\s+(.*))?$", line)
            if match:
                short, long_name, desc = match.groups()
                current_flag = long_name
                desc_text = desc.strip() if desc else ""
                options[long_name] = {
                    "long": long_name,
                    "short": short,
                    "description": desc_text
                }
            elif current_flag and line.startswith("          "):
                # Continuation line
                options[current_flag]["description"] += " " + line.strip()

    return options

def get_top_level_commands(boxr_bin):
    """Extract list of subcommands from boxr --help."""
    out = subprocess.check_output([boxr_bin, "--help"]).decode("utf-8")
    commands = []
    in_commands = False
    for line in out.splitlines():
        if line.strip() == "Commands:":
            in_commands = True
            continue
        if in_commands:
            if not line.strip() or line.strip() in ("Options:", "Arguments:"):
                break
            match = re.match(r"^\s+([a-zA-Z0-9\-]+)\s+(.*)$", line)
            if match:
                cmd_name, desc = match.groups()
                if cmd_name not in ("help",):
                    commands.append(cmd_name)
    return commands

def extract_all_boxr_options(boxr_bin):
    """Extract options for every command in Boxr."""
    commands = get_top_level_commands(boxr_bin)
    results = {}

    for cmd in commands:
        try:
            out = subprocess.check_output([boxr_bin, cmd, "--help"], stderr=subprocess.DEVNULL).decode("utf-8")
            opts = parse_help_options(out)
            results[cmd] = {
                "command": cmd,
                "options": opts,
                "count": len(opts)
            }
        except Exception:
            pass

    return results

if __name__ == "__main__":
    boxr_bin = sys.argv[1] if len(sys.argv) > 1 else find_boxr_binary()
    if not boxr_bin:
        print("Error: Could not find boxr binary. Specify path: python3 extract_boxr_options.py <path>")
        sys.exit(1)

    print(f"Extracting Boxr options from: {boxr_bin}")
    data = extract_all_boxr_options(boxr_bin)
    
    total_options = sum(d["count"] for d in data.values())
    print(f"Extracted {len(data)} commands with a total of {total_options} options.")

    if len(sys.argv) > 2 and sys.argv[2] == "--json":
        print(json.dumps(data, indent=2))
    else:
        for cmd, info in sorted(data.items()):
            opts_sample = ", ".join(list(info["options"].keys())[:6])
            print(f"  boxr {cmd:<14} ({info['count']:>2} opts): {opts_sample}...")
