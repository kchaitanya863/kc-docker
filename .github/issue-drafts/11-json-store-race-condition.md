## Summary

JSON metadata stores (containers, images, volumes, networks, pods) use non-atomic read-modify-write without file locking, causing data loss under concurrent access.

## Severity

**Medium** — data corruption / lost updates

## Locations

- `src/storage/container_store.rs` lines 86–97
- `src/storage/image_store.rs` — same pattern
- `src/volume/mod.rs` — same pattern
- `src/network/mod.rs` — same pattern
- `src/pod/mod.rs` — same pattern

## Description

```rust
fn load(&self) -> ContainerStoreData {
    if let Ok(content) = fs::read_to_string(&self.index_file) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        ContainerStoreData::default()
    }
}

fn save(&self, data: &ContainerStoreData) -> Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    fs::write(&self.index_file, content)?;
    Ok(())
}
```

No `flock`, no write-to-temp + `rename`, no retry on conflict.

## Impact

Concurrent daemon API requests, CLI commands, and the process reaper can lose updates or produce truncated/corrupt JSON index files.

## Suggested fix

Use `flock` around load/save, or write to a temp file and atomically `rename`. Consider retry on conflict for concurrent writers.

## Labels

`bug`, `concurrency`, `storage`
