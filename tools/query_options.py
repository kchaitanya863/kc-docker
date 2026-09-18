#!/usr/bin/env python3
"""
query_options.py: Programmatically query Docker and Podman upstream docs
to extract all supported commands, subcommands, and options into structured JSON.
"""

import sys
import os
import json
import urllib.request
import re

DOCKER_DOCS_API = "https://api.github.com/repos/docker/cli/contents/docs/reference/commandline"
DOCKER_RAW_BASE = "https://raw.githubusercontent.com/docker/cli/master/docs/reference/commandline"
PODMAN_OPTIONS_API = "https://api.github.com/repos/podman-container-tools/podman/contents/docs/source/markdown/options"
PODMAN_RAW_BASE = "https://raw.githubusercontent.com/podman-container-tools/podman/main/docs/source/markdown/options"

def get_auth_token():
    # Try GH_TOKEN / GITHUB_TOKEN or via gh auth token
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if not token:
        try:
            import subprocess
            out = subprocess.check_output(["gh", "auth", "token"], stderr=subprocess.DEVNULL)
            token = out.decode("utf-8").strip()
        except Exception:
            pass
    return token

def fetch_json(url, token=None):
    headers = {"User-Agent": "boxr-doc-query"}
    if token:
        headers["Authorization"] = f"token {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as resp:
        return json.loads(resp.read().decode("utf-8"))

def fetch_text(url, token=None):
    headers = {"User-Agent": "boxr-doc-query"}
    if token:
        headers["Authorization"] = f"token {token}"
    req = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(req) as resp:
        return resp.read().decode("utf-8")

def parse_docker_markdown(content):
    """Extract commands, aliases, and options from a docker/cli markdown doc."""
    options = []
    lines = content.splitlines()
    table_started = False

    for line in lines:
        if line.startswith("| Name"):
            table_started = True
            continue
        if table_started:
            if line.startswith("|:"):
                continue
            if not line.startswith("|"):
                break
            parts = [p.strip() for p in line.split("|")[1:-1]]
            if len(parts) >= 4:
                options.append({
                    "name": parts[0].replace("`", ""),
                    "type": parts[1].replace("`", ""),
                    "default": parts[2].replace("`", ""),
                    "description": parts[3].replace("<br>", " ")
                })

    return options

def query_docker_command(cmd_name, token=None):
    """Query options for a specific docker command (e.g. 'run', 'build', 'ps')."""
    # Filename format: e.g. 'run.md', 'compose_up.md'
    filename = f"{cmd_name.replace(' ', '_')}.md"
    url = f"{DOCKER_RAW_BASE}/{filename}"
    try:
        text = fetch_text(url, token)
        return parse_docker_markdown(text)
    except Exception as e:
        return None

def query_all_docker_commands(token=None, limit=None):
    """List and query all docker commands from docker/cli."""
    files = fetch_json(DOCKER_DOCS_API, token)
    result = {}
    md_files = [f for f in files if f.get("name", "").endswith(".md")]
    if limit:
        md_files = md_files[:limit]

    for f in md_files:
        name = f["name"].replace(".md", "").replace("_", " ")
        download_url = f["download_url"]
        try:
            content = fetch_text(download_url, token)
            opts = parse_docker_markdown(content)
            if opts:
                result[name] = opts
        except Exception:
            pass
    return result

def query_podman_options(token=None, limit=20):
    """List podman modular options from podman-container-tools/podman."""
    files = fetch_json(PODMAN_OPTIONS_API, token)
    options = []
    md_files = [f for f in files if f.get("name", "").endswith(".md") and f.get("name") != "README.md"]
    if limit:
        md_files = md_files[:limit]

    for f in md_files:
        name = f["name"].replace(".md", "")
        download_url = f.get("download_url") or f"{PODMAN_RAW_BASE}/{f['name']}"
        try:
            content = fetch_text(download_url, token)
            # Podman option header: e.g. #### **--add-host**=*host:ip*
            first_line = content.splitlines()[0] if content else ""
            options.append({
                "option": name,
                "header": first_line,
                "raw_url": download_url
            })
        except Exception:
            pass
    return options

if __name__ == "__main__":
    token = get_auth_token()
    cmd = sys.argv[1] if len(sys.argv) > 1 else "docker:run"

    if cmd.startswith("docker:"):
        target_cmd = cmd.split(":", 1)[1]
        print(f"Querying Docker CLI options for '{target_cmd}'...")
        opts = query_docker_command(target_cmd, token)
        if opts:
            print(f"Found {len(opts)} options for 'docker {target_cmd}':")
            for o in opts[:10]:
                print(f"  {o['name']:<25} ({o['type']}) : {o['description'][:60]}...")
            if len(opts) > 10:
                print(f"  ... and {len(opts) - 10} more options.")
        else:
            print(f"Command 'docker {target_cmd}' not found.")
    elif cmd == "podman":
        print("Querying Podman upstream modular options...")
        opts = query_podman_options(token, limit=15)
        print(f"Found {len(opts)} sample Podman options:")
        for o in opts:
            print(f"  {o['option']:<25} -> {o['header']}")
    elif cmd == "docker:all":
        print("Querying all Docker CLI commands...")
        all_cmds = query_all_docker_commands(token, limit=10)
        print(f"Parsed {len(all_cmds)} commands:")
        for c, opts in all_cmds.items():
            print(f"  docker {c:<20} : {len(opts)} options")
    else:
        print("Usage: python3 query_options.py [docker:<cmd> | podman | docker:all]")
