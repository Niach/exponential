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
- `Trace` + `TraceItem` (shadcn's Trace: icon, title, muted detail, a collapsible content rail; `status` running|error|pending, `size` default|sm), the Text `shimmer` (`textShimmer`, `fixtures/text-shimmer.json`), CodeBlock `variant` (destructive) + `size` (sm) in shadcn's code-block look, the `ansi` language and the `success`/`warning`/`error` token kinds (`codeText`, `ansiRuns`).
- `@exponential-at/ui/web`: the framework-free web renderer layer (theme and base CSS, node sheets, the bind pass, URL/media policy, host plugin types, the natives' pure helpers) and `globalsCss`, a theme as plain shadcn globals.
- `orange`, the brand-coloured reference theme: the exponential look with an orange primary.
- The real-font geometry matrix: the kitchen sink, the responsive section and a project dashboard, in every built-in theme, both modes, four widths and both directions.
