## Summary

When OverlayFS mount fails, the CoW fallback creates hardlinks to base image files. Container writes modify the same inode as the shared base image layer.

## Severity

**High** — data integrity; image layer corruption

## Location

`src/storage/overlay.rs` lines 57–58, 123–157

## Description

```rust
// Fallback: fast CoW hardlink tree
Self::create_hardlink_tree(base_rootfs, &merged_dir)?;
```

```rust
// Attempt hardlink; if cross-device or permission fails, fallback to copy
if fs::hard_link(&from, &to).is_err() {
    let _ = fs::copy(&from, &to);
}
```

When hardlink succeeds, writes in the container modify the base image in place. Concurrent containers sharing the same base layer can corrupt each other's filesystem state.

## Suggested fix

Never hardlink for the CoW fallback — always copy files. Alternatively, refuse to run without overlay/clonefile support.

## Labels

`bug`, `storage`, `overlay`
