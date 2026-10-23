# Changelog: `exponential-ui` (Rust core)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- The reducer (core and basic catalogs, macros, extensions, validation) and the bind pass (bindings, functions, `$string`).
- The taffy layout engine: batched text measurement, overlay layers, windowed lists, scroll containers, sticky nodes and breakpoints.
- The embedded catalog, macro table and built-in themes. It replays the shared fixtures byte for byte.
- VAPP-106: the embedded catalog is the runtime one (no descriptions or examples); the `descriptions` feature (off) adds `catalog::descriptions()`; the default-on `builtin-themes` feature holds the non-default resolved themes, so `--no-default-features` embeds only `exponential`.
