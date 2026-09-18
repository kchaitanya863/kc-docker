## Summary

`boxr load`, `boxr import`, and layer unpacking in `auth/mod.rs` use `archive.unpack()` without path validation, enabling classic tar-slip attacks.

## Severity

**Critical** — arbitrary file write outside intended directory

## Locations

- `src/auth/mod.rs` lines 254–255 (`load` outer archive)
- `src/auth/mod.rs` lines 295–296 (`load` layer archives)
- `src/lib.rs` line 2748 (`import` command)

## Description

Archives are unpacked directly:

```rust
archive.unpack(temp_dir.path())?;
// ...
layer_archive.unpack(&dest_rootfs)?;
```

By contrast, `src/oci/image.rs` uses the safer `entry.unpack_in(target_dir)` when pulling from registries.

## Impact

A malicious or compromised image tar can write files anywhere on the host filesystem accessible to the user running `boxr load` or `boxr import`.

## Suggested fix

Use `entry.unpack_in()` with validation that rejects absolute paths and `..` traversal, matching the approach in `unpack_layer`.

## Labels

`bug`, `security`, `load`, `import`
