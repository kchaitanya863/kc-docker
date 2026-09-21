# Contributing to `boxr` 🤝

Thank you for your interest in contributing to `boxr`! We welcome contributions from everyone — bug fixes, documentation improvements, performance optimizations, and new feature additions.

`boxr` is currently in **Beta**. We prioritize stability, rootless security, spec conformance (OCI), and fast execution.

---

## Code of Conduct

We are committed to providing a welcoming, inclusive, and harassment-free environment for all contributors. Please be respectful and constructive in all discussions, pull requests, and issues.

---

## Architectural Invariants to Keep in Mind

Before writing code, please review [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/ROOTLESS.md](docs/ROOTLESS.md). Keep these core invariants in mind:

1. **Pre-Tokio Single-Threaded Trampoline**:
   - The Linux kernel rejects `unshare(CLONE_NEWUSER)` with `EINVAL` if called from a multi-threaded process.
   - Any unshare operations (`boxr unshare`, `__internal-trampoline`, `__internal-trampoline-exec`) MUST be intercepted in `src/main.rs` before building or entering the multi-threaded Tokio runtime.
2. **Rootless by Default**:
   - Never introduce code requiring `sudo`, setuid binaries, or host daemon permissions.
   - All network and filesystem operations must work within unprivileged user namespaces and rootless TAP virtualization (`usernet` or `pasta`).
3. **SOLID Subsystem Organization**:
   - Avoid monolithic files exceeding 700 lines. Place new domain models in submodules (`src/cli/`, `src/daemon/`, `src/builder/`, `src/network/usernet/`, etc.).
4. **Interface Segregation in Storage**:
   - Read-only operations (`ps`, `inspect`, `df`, `stats`) must accept `&impl ContainerReader` or `&impl ImageReader` rather than mutating handles.
5. **Regression Test Contracts**:
   - Key traits and public module interfaces are covered by automated tests. Always run the test suite before submitting PRs.

---

## Development Setup

### Prerequisites

- **Rust toolchain** (stable 1.80+ or latest stable edition 2024):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup component add clippy rustfmt
  ```

### Building the Project

```bash
# Debug build
cargo build

# Optimized release build
cargo build --release
```

The resulting binary will be located at `target/release/boxr`.

---

## Verification & Testing

Before submitting a pull request, run all verification steps:

```bash
# 1. Check syntax and compilation across all targets
cargo check --all-targets

# 2. Check code formatting
cargo fmt --all -- --check

# 3. Run Clippy linter
cargo clippy --all-targets

# 4. Run unit and integration tests
cargo test --all -- --test-threads=1
```

For platform-specific testing or full end-to-end suites:
- See [docs/BLACKBOX_QA.md](docs/BLACKBOX_QA.md) for black-box QA and smoke test instructions.

---

## Submitting Pull Requests

1. **Fork & Branch**: Create a feature branch off `main` with a descriptive name (e.g., `fix/overlay-symlink` or `feat/compose-healthcheck`).
2. **Commit Hygiene**:
   - Write clear, concise commit messages following standard conventions (e.g., `feat: ...`, `fix: ...`, `docs: ...`).
   - Keep commits focused on a single change.
3. **Tests & Docs**:
   - Add unit/integration tests covering new features or bug fixes.
   - Update relevant documentation in `docs/` and `README.md` if CLI flags or runtime behaviors change.
4. **PR Description**:
   - Clearly explain the problem being solved, the approach taken, and how the changes were verified.

---

## License

By contributing to `boxr`, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
