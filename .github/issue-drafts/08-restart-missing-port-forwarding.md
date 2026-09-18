## Summary

Published ports are forwarded on initial detached `run`, but not restored when a container is restarted via `boxr start` / `boxr restart`.

## Severity

**Medium** — Docker parity / networking regression after restart

## Locations

- `src/lib.rs` lines 996–1001 — port forwarding started on detach run
- `src/lib.rs` lines 1146–1160 — `start_container` does not call `PortForwardManager`
- `src/network/rootless.rs` — `start_forwarding` only referenced from initial run path

## Description

On initial detached run:

```rust
if !parsed_ports.is_empty() {
    let _ = network::rootless::PortForwardManager::start_forwarding(&parsed_ports).await;
}
```

On restart, `rec.ports` is ignored and `PortForwardManager` is never called.

## Impact

After `boxr restart`, published ports stop working until the container is recreated.

## Suggested fix

In `start_container`, read `rec.ports` (or `ports.json` in the bundle) and call `PortForwardManager::start_forwarding` when appropriate.

## Labels

`bug`, `networking`, `restart`, `parity`
