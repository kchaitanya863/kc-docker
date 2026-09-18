## Summary

`boxr compose up` ignores failures when starting individual services and always reports success.

## Severity

**Medium** — orchestration silently fails

## Location

`src/compose/mod.rs` line 395

## Description

```rust
let _ = crate::run_container(run_args).await;
// ...
println!("Project '{}' started successfully.", self.name);
Ok(())
```

Service start errors are discarded. The project is reported as started successfully even when one or more services failed to start.

## Impact

Users believe all compose services are running when some may have failed. Downstream health checks and dependency assumptions break.

## Suggested fix

Propagate `run_container` errors. Collect and report per-service failures. Return non-zero exit code if any service fails to start.

## Labels

`bug`, `compose`, `parity`
