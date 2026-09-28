# VAPP-4 authoring package: decision

**Recommendation: ship our own zero-dependency `vapp-css` (the `create`/`props`
shape of StyleX, `vapp-css.ts` here), not `@stylexjs/stylex` and not
`react-strict-dom`.** Authors keep the StyleX API they know; the host emits
plain JSON with the conditions still nested, and every client resolves them
(D7). Raw evidence: `OUTPUT.md`.

## What each candidate returns on a headless host

| candidate | `create()` | `props()` | grid | conditions |
|---|---|---|---|---|
| `@stylexjs/stylex` 0.19.1, no compiler | **throws** "Styles must be compiled by '@stylexjs/babel-plugin'" (also `defineVars`) | `{}` | n/a | n/a |
| `@stylexjs/dev-runtime` (npm latest = 0.11.1) | `inject()` **throws** on load: imports `@stylexjs/stylex/lib/StyleXSheet`, which 0.19.1 no longer exports. Its inner `create` alone returns atomic CLASS-NAME maps (`display: "xrvj5dj"`) plus CSS text via `insert` | `{ className: "xrvj5dj …" }` | kept, as CSS text | compiled to `:hover` / `@media` CSS rules |
| `react-strict-dom` 0.0.55 native build (with a hand-made `react-native` shim) | plain-ish objects (lengths become `CSSLengthUnitValue` instances until `props`) | **plain resolved RN style objects**, e.g. `{padding: 16, opacity: 1, gap: 4}` | **dropped** with a warning: `gridTemplateAreas`, `gridTemplateColumns` (not in its allowlist); `display: grid` passes through with a "not supported" warning | only StyleX's VALUE-level form (`gap: {default: 4, "@media …": 12}`); our top-level `"@media …": {…}` / `":hover": {…}` blocks pass through UNRESOLVED |
| `vapp-css` (this spike, ~150 lines) | identity + whitelist validation, frozen | ordered merge, conditions merged key by key and kept nested; JSON round-trips byte-identical | kept (strings / row arrays) | kept for the client |

## Why not the two libraries

- **StyleX web** is compile-only. The babel plugin could run on the host, but
  it produces the same thing the dev runtime does: atomic class names plus a
  stylesheet. That is a web artefact. Natives would have to parse CSS back out
  of it, and a class-name map is not a style object.
- **react-strict-dom native** is the closest thing to "plain objects", but it
  resolves in the WRONG PLACE and against the WRONG WIDTH: `props` reads
  `viewportWidth`/`hover`/`fontScale` from `this` (its html components feed it
  from `useWindowDimensions`, `useColorScheme` and contexts, so a real render
  needs hooks; a bare `css.props.call({viewportWidth}, …)` works without
  them). The host would bake one breakpoint into the wire, and the breakpoint
  would be the SCREEN, not the surface the client lays out in. It drops CSS
  Grid, which the kitchen sink uses and taffy supports. `rem` is resolved to
  16 × fontScale on the host. `defineVars` is a process-global registry
  (`var(--card__id__1)`) resolved at `props` time. Loading it at all needs a
  `react-native` stand-in: bun auto-installs the 31 MB `react-native` 0.87.1
  peer, whose Flow source does not parse, so we swap it with a `Bun.plugin`
  `onLoad` hook plus `__DEV__`. Total install: 178 MB, 187 packages.

## Why our own

- The wire contract already IS "whitelisted StyleX-subset object with nested
  conditions" (`vapp_spike::WHITELIST`, `style::resolve`). `vapp-css` is
  exactly that contract as a TypeScript type: unknown keys, bad keywords and
  non-token colours are compile errors (`typecheck-vapp-css.ts`), and runtime
  validation catches `:hover`, `zIndex` and nested conditions.
- It validated all 28 kitchen-sink styles unchanged.
- Zero dependencies, no `react-native` peer, no compiler, runs anywhere a
  vApp host runs (Bun, a worker, the CLI daemon).
- Tokens stay strings (`$palette.card`); each client maps them (web: CSS
  variables, verified in `apps/web/src/components/vapp-spike/style-to-css.ts`).

Costs: we own ~150 lines and the key list must stay in lockstep with the Rust
whitelist (generate both from `domain-contract`, drift-gated like the other
contracts). We lose StyleX's value-level condition form and `defineVars`; add
them later only if authors ask.

## Licences

All candidates are MIT (`@stylexjs/*`, `react-strict-dom`, `react-native`).
`vapp-css` would be ours (Apache-2.0 with the repo).
