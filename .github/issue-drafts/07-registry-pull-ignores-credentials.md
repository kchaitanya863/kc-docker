## Summary

`boxr pull` does not use stored registry credentials from `~/.boxr/config.json` (or Docker config fallback). Private registry pulls fail even after `boxr login`.

## Severity

**High** — broken authentication for private registries

## Locations

- `src/oci/distribution.rs` — `RegistryClient::authenticate` only handles anonymous bearer token flow
- `src/lib.rs` — `pull_image_with_platform` creates `RegistryClient::new()` without loading credentials
- `src/auth/mod.rs` — `CredentialStore` is only used by the stub `RegistryPusher::push`

## Description

`authenticate()` reacts to `WWW-Authenticate` bearer challenges but never reads `CredentialStore`. `fetch_bearer_token` requests tokens without username/password.

Credentials are loaded in `RegistryPusher::push` but push itself is a stub that only prints messages.

## Impact

Users who run `boxr login` successfully still cannot pull from private registries.

## Suggested fix

Load credentials in `authenticate` / `fetch_bearer_token` and send HTTP Basic auth or registry-specific authenticated token requests.

## Labels

`bug`, `auth`, `registry`, `parity`
