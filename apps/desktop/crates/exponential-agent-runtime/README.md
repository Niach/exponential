# exponential-agent-runtime (deprecated)

Renamed to [`eagent`](../eagent/README.md) (#EXP-1505). This crate is a shim, `pub use eagent::*;`,
kept for one release so existing consumers keep building. Move over:

```toml
eagent = { git = "https://github.com/Niach/exponential", tag = "eagent-v0.1.0" }
```

then replace `exponential_agent_runtime::` with `eagent::`. See `docs/libraries.md`, "Migrating".
