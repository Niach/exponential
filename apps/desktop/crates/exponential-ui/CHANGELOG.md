# Changelog: `exponential-ui` (Rust core)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.1 (2026-10-23)

- Version-only release: the SDK's npm packages moved from the `@exponential-at` scope to `@exponential_app` and every package shares one version. The crate is unchanged from 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-146: a Select trigger shows the option holding `""` (an unseeded value reads as `""`), and pressing the option that holds the current value only closes the list; `tests/interactions_fixture.rs` replays `select`.
- The reducer (core and basic catalogs, macros, extensions, validation) and the bind pass (bindings, functions, `$string`).
- The taffy layout engine: batched text measurement, overlay layers, windowed lists, scroll containers, sticky nodes and breakpoints.
- The embedded catalog, macro table and built-in themes. It replays the shared fixtures byte for byte.
- VAPP-106: the embedded catalog is the runtime one (no descriptions or examples); the `descriptions` feature (off) adds `catalog::descriptions()`; the default-on `builtin-themes` feature holds the non-default resolved themes, so `--no-default-features` embeds only `exponential`.
- VAPP-124: on Android and iOS the stack-growth check reads the thread's stack limit from pthread (`stack.rs`), so passes run on the host's own stack and only switch to a heap segment when it runs low (ART refused JNA upcalls from the segment with StackOverflowError).
- VAPP-132: a restyle pass memoises the node-independent style work (template items share it), `get_pointer` reads without allocating, template keys skip the duplicate walk when they are distinct, list heights hash with FxHash and the measure memo is keyed per slot (forgetting a leaf no longer scans every leaf).
