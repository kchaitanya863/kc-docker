## Summary

Volume bind mounts accept relative paths with `..` traversal, allowing containers to mount sensitive host directories such as `/etc`.

## Severity

**Critical** — host path exposure via volume mounts

## Location

`src/volume/mod.rs` lines 184–201

## Description

Host bind paths are canonicalized but never validated as safe. A spec like `../../../etc:/data` resolves to `/etc` and is accepted.

```rust
let canonical_source = if host_path.exists() {
    host_path.canonicalize()?
} else {
    fs::create_dir_all(&host_path)?;
    host_path.canonicalize()?
};
```

## Test expectation (currently failing)

`tests/e2e_test.rs` (`test_e2e_defensive_security_and_kill`) expects this to fail:

```rust
// Path traversal in volume mount is rejected
boxr run --rm -v ../../../etc:/data alpine /bin/echo test
// assert !output.status.success()
```

## Suggested fix

Reject volume specs containing `..`, or require the canonical source to remain under an allowed root (cwd, explicit allowlist, or user home).

## Labels

`bug`, `security`, `volumes`
