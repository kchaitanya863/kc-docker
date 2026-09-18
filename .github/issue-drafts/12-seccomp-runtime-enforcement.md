## Summary

Default seccomp syscall blocking was defined in `SeccompRule::default_filter()` but never applied at runtime.

## Status

**Fixed** in PR — `apply_default_seccomp()` now installs a BPF filter via `seccompiler`, and default seccomp is set on the OCI spec for non-privileged containers.

## Labels

`bug`, `security`
