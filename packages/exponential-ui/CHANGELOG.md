# Changelog: `@exponential-at/ui`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- The core catalog (components with props, events and model-facing descriptions), the macro table, the A2UI basic catalog mapped onto it, the style whitelist and the token names.
- The runtime theme format, three built-in themes and the theme builder.
- Codegen of the catalog constants for TypeScript, Swift, Kotlin and Rust.
- The host API shared by every renderer: transports, functions, bindings, negotiation, URL policy and declarative packages.
- The agent prompt and the validator.
- The fixtures every renderer replays, plus the conformance suite and `conformance:check`.
- VAPP-106: a size budget. The root entry ships the runtime catalog without its descriptions and examples (`generated/core.runtime.json`; `describeCatalog()` attaches them), and the prompt, the schema emitters, the conformance suite and the host API are entries of their own (`@exponential-at/ui/prompt`, `./schema`, `./conformance`, `./host`): the root bundle went from 1.74 MB / 230 KB gzipped to a fraction of it (`release/sizes.json`); `release/size.ts` measures every npm entry, the xcframework, the Android libraries and the crate, and CI fails a change over `release/size-budget.json`.
- `Trace` + `TraceItem` (shadcn's Trace: icon, title, muted detail, a collapsible content rail; `status` running|error|pending, `size` default|sm), the Text `shimmer` (`textShimmer`, `fixtures/text-shimmer.json`), CodeBlock `variant` (destructive) + `size` (sm) in shadcn's code-block look, the `ansi` language and the `success`/`warning`/`error` token kinds (`codeText`, `ansiRuns`).
- The real-font geometry matrix: the kitchen sink, the responsive section and a project dashboard, in every built-in theme, both modes, four widths and both directions.
