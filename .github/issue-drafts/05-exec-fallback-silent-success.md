## Summary

When `nsenter` is unavailable, `exec_in_bundle` falls back to a partial isolation path that discards all errors and always returns success.

## Severity

**High** — silent command failure; incomplete namespace isolation

## Location

`src/runtime/linux.rs` lines 468–493

## Description

```rust
let _ = unshare(flags);
// ...
let _ = nix::unistd::chroot(&abs_rootfs);
let _ = chdir("/");
let _ = nix::unistd::execvp(&binary_c, &args_c);
Ok(0)
```

All errors from `unshare`, `chroot`, `chdir`, and `execvp` are ignored. The function always returns `Ok(0)`.

The fallback also does not enter the container PID or network namespace — only a mount namespace change via `CLONE_NEWNS` and chroot.

## Impact

`boxr exec` and daemon exec endpoints can report success while the command never ran, or ran with wrong isolation.

## Suggested fix

Propagate errors from each syscall. Return non-zero exit codes on failure. Remove or hard-disable this fallback unless full namespace entry is confirmed.

## Labels

`bug`, `exec`, `runtime`
