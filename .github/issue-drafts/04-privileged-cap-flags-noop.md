## Summary

`--privileged`, `--cap-add`, and `--cap-drop` CLI flags are parsed but never applied at runtime. Capability and seccomp profiles are defined but unused.

## Severity

**High** — security controls advertised but not enforced

## Locations

- `src/cli.rs` — flags defined (`privileged`, `cap_add`, `cap_drop`)
- `src/security/mod.rs` — `CapabilityProfile` and `SeccompRule` defined (only referenced in unit tests)
- `src/runtime/linux.rs` — `run_container_child` never applies capabilities or seccomp

## Description

A grep for `args.privileged`, `args.cap_add`, or `args.cap_drop` in runtime code returns no matches. Users believe they are dropping/adding capabilities or running privileged containers, but behavior is unchanged from default.

## Impact

- `--cap-drop ALL` does not reduce container privileges
- `--privileged` does not grant additional capabilities
- Default seccomp blocking of dangerous syscalls is not enforced

## Suggested fix

Wire CLI flags into OCI spec generation and enforce via libcap/seccomp in `run_container_child`, or explicitly reject unsupported flags with a clear error.

## Labels

`bug`, `security`, `capabilities`, `parity`
