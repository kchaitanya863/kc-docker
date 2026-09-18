## Summary

Volume bind mount validation only rejected relative `..` paths. Absolute paths like `/foo/../../../etc:/data` could bypass the guard and mount `/etc`.

## Status

**Fixed** in PR — any host bind path containing `..` is now rejected unconditionally.

## Labels

`bug`, `security`
