# Changelog: `@exponential-at/ui`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- `@exponential-at/ui/web/primitives` (FEED-131): the primitives' class recipes, framework-free and shared by the React and Solid primitive sets (cva recipes under their shadcn names, plain strings as `<MODULE>_CLASSES`, `cn`). New runtime dependencies: `class-variance-authority`, `clsx`, `tailwind-merge`.
- The web-parity check covers the primitive specimens (`fixtures/primitive-specimens.json`, group `primitive`), compared over the whole page body.
- `fixtures/toast-stack.json`: the toast stack contract both toast primitives follow.

- The core catalog (components with props, events and model-facing descriptions), the macro table, the A2UI basic catalog mapped onto it, the style whitelist and the token names.
- The runtime theme format, three built-in themes and the theme builder.
- Codegen of the catalog constants for TypeScript, Swift, Kotlin and Rust.
- The host API shared by every renderer: transports, functions, bindings, negotiation, URL policy and declarative packages.
- The agent prompt and the validator. VAPP-108: the prompt is sectioned (`sections`, `only`, `componentPrompt(name)`, `catalogIndex()`: rules + a one-line index first, a component's block on demand), the typed authoring DSL (`defineSurface`, `ui.<Component>()`, `bind` / `call` / `event` / `run`, `ComponentProps` generated from the catalog) and the ergonomics eval under `eval/` (the corpus, the cached replies, the scorer, the web render, the Devices parity on web and desktop, the rewrite gap list).
- The fixtures every renderer replays, plus the conformance suite and `conformance:check`.
- `Trace` + `TraceItem` (shadcn's Trace: icon, title, muted detail, a collapsible content rail; `status` running|error|pending, `size` default|sm), the Text `shimmer` (`textShimmer`, `fixtures/text-shimmer.json`), CodeBlock `variant` (destructive) + `size` (sm) in shadcn's code-block look, the `ansi` language and the `success`/`warning`/`error` token kinds (`codeText`, `ansiRuns`).
- `@exponential-at/ui/web`: the framework-free web renderer layer (theme and base CSS, node sheets, the bind pass, URL/media policy, host plugin types, the natives' pure helpers) and `globalsCss`, a theme as plain shadcn globals.
- `orange`, the brand-coloured reference theme: the exponential look with an orange primary.
- The real-font geometry matrix: the kitchen sink, the responsive section and a project dashboard, in every built-in theme, both modes, four widths and both directions.
