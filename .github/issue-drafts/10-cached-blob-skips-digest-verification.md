## Summary

Registry blob downloads skip SHA-256 verification when a file already exists at the destination path.

## Severity

**Medium** — corrupt cached layers trusted indefinitely

## Location

`src/oci/distribution.rs` lines 265–268

## Description

```rust
if dest_path.exists() {
    // Already cached and downloaded
    return Ok(());
}
```

If a partial or corrupt file exists from a crashed download, it is trusted forever without digest validation.

## Impact

- Broken images pulled without re-download
- Hard-to-debug runtime failures from corrupt layers
- Silent data integrity issues

## Suggested fix

Verify SHA-256 digest on cache hit before returning. Use atomic temp-file + rename only after successful verification (the download path already validates digest for new downloads).

## Labels

`bug`, `registry`, `storage`
