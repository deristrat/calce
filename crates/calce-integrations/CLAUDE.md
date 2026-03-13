# calce-integrations

External data source integrations, organized by provider. Each provider is
feature-gated so its dependencies are only pulled in when needed.

## Structure

```
src/
├── lib.rs
└── njorda/           — Njorda legacy market data system (feature: "njorda")
    ├── mod.rs        — NjordaLoader, build_service(), NjordaError, print_summary()
    ├── backend.rs    — NjordaBackend: DataBackend impl
    ├── repo.rs       — NjordaRepo: legacy DB queries
    ├── types.rs      — CachedMarketData, CachedPrice, CachedFxRate, etc.
    ├── cache.rs      — rkyv/lz4 serialization
    └── bin.rs        — njorda-fetch CLI
```

## Adding a new provider

1. Create `src/<provider>/` with a feature gate in `Cargo.toml`
2. Add `#[cfg(feature = "<provider>")] pub mod <provider>;` to `lib.rs`
3. Implement `DataBackend` if the provider supplies portfolio data
