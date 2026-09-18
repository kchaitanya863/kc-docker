## Summary

`boxr cp` does not validate that container paths stay within the container rootfs. A path containing `..` components can escape the container filesystem boundary and read or write arbitrary host files.

## Severity

**Critical** — host filesystem read/write outside container boundary

## Location

`src/runtime/cp.rs` lines 19–21 and 49–53

## Description

Container paths are sanitized only with `trim_start_matches('/')`. There is no rejection of `..` components and no `canonicalize()` + prefix check to ensure the resolved path remains under `rootfs`.

```rust
let cont_rootfs = PathBuf::from(&cont.bundle_path).join("rootfs");
let clean_cont_path = container_path.trim_start_matches('/');
let source_abs = cont_rootfs.join(clean_cont_path);
```

## Proof of concept

```bash
boxr cp mycontainer:../../../etc/passwd ./stolen-passwd
boxr cp ./malicious.txt mycontainer:../../../tmp/evil.txt
```

## Expected behavior

Docker rejects or confines paths so they cannot escape the container rootfs.

## Suggested fix

Resolve `source_abs` / `dest_abs` with `canonicalize()` (or manual normalization) and reject unless the result is a prefix of `cont_rootfs`.

## Labels

`bug`, `security`, `cp`
