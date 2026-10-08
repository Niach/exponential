# `@exponential-at/ui` — the Exponential UI catalog

The data half of Exponential UI (VAPP-84): the **core catalog** of
components an A2UI surface may use, the **A2UI basic catalog** vendored and
mapped onto it, the **macro table** that turns composite components into
natives, the **model-facing descriptions** and prompt, the **token names**
themes must define, the **runtime theme format** with the three built-in
themes and the theme builder (VAPP-92), codegen for TypeScript, Swift, Kotlin
and Rust, and the fixtures every renderer suite locks against. No renderer
lives here (React: VAPP-87, the Rust core: VAPP-86, painters: VAPP-88/89/90).

It also defines the **host API** every renderer shares (VAPP-91: transport,
functions, bindings, negotiation, policy, declarative packages, the
Exponential connector) and packages the **conformance suite**. The workspace
package.json stays `"private": true` with `main` on the sources; the npm
package is staged and published by `release/` on `ui-v*` tags. Depends on
nothing from the Exponential app.

## Layout

| path | what |
|---|---|
| `catalog/core.catalog.json` | THE source: ids, enums, shared shapes, functions, 61 components with props + descriptions |
| `catalog/macros.json` | the declarative expansion table, one template per macro component |
| `catalog/basic-map.json` | A2UI basic → core: components (+ named transforms), icons, functions |
| `catalog/style.json` | the `Box` style whitelist (VAPP-4), one source for TS / schema / natives |
| `catalog/tokens.json` | the token NAMES (`$color.primary`, `$spacing.md`, …); values come from a theme |
| `catalog/recipes.json` | the recipe contract: interaction states, the recipe key whitelist, every native's parts + `when` props (macro parts come from macros.json) |
| `catalog/host.json` | the host API contract (VAPP-91): message kinds, ops, error codes, function decisions, URL schemes, MCP carrier, package format |
| `catalog/core.schema.json` | generated: the catalog as JSON Schema in A2UI's catalog shape |
| `catalog/theme.schema.json` | generated: a theme file as JSON Schema |
| `themes/` | the built-in themes: `neutral` (stock shadcn, the root), `exponential` (GENERATED: design-tokens values + `exponential.recipes.json`), `playful` (the test theme) |
| `builder/` | the theme builder page (`dev:builder` / `build:builder`); ui.exponential.at mounts it (VAPP-93) |
| `src/` | the TS reference implementation (below) |
| `generated/*.{swift,kt,rs}` | generated constants for the three native targets; `*Themes*` = the built-ins RESOLVED, embedded as JSON |
| `docs/components.generated.json` | generated: one doc page per component as data (ui.exponential.at renders it, VAPP-93) |
| `docs/themes.generated.json` | generated: the token vocabulary with every built-in's values, the recipe contract per component |
| `fixtures/` | the contract (below) |
| `src/host/` | the host API reference: router, decoders, policy, sources, packages, the `ExponentialHost` runtime + transports |
| `src/connector/` | the Exponential connector (MCP OAuth + `exp:` sources over MCP) and `createVappHost` |
| `conformance/` | the conformance suite: `manifest.json` (generated), `report.schema.json`, the runner guide |
| `release/` | the npm build + staging scripts and the release runbook (all registries) |
| `vendor/a2ui/` | A2UI v0.9 schemas, byte-pinned (see its README) |
| `vendor/json-render/` | attribution for the borrowed description wording |

## The catalog

Canonical names are shadcn's; the issue's table decides what is **native**
(a renderer paints it) and what is a **macro** (expanded into natives before
painting). Every component has one sentence, every prop one sentence; both
are what the model reads. Components carry `variant` and `size` where
shadcn does (`default|secondary|outline|ghost|destructive|link`,
`sm|default|lg|icon`); what a variant looks like is the theme's recipe, keyed
by the `recipe: {macro, part, props}` every expanded node carries.

Ids: `CORE_CATALOG_ID` = `https://ui.exponential.at/catalogs/core/v1`,
`CORE_LITE_CATALOG_ID` (the subset without overlays, media and Chart),
`A2UI_BASIC_CATALOG_ID`; `SUPPORTED_CATALOG_IDS` is what a client advertises.

Wire form = A2UI's: a flat list of `{id, component, …props, children: [ids] |
{componentId, path}, slots?: {name: id}, on?: {event: Action}, style?}`.
Normalized form (what painters get) = `UiNode`: `{id, component, props,
style?, on?, children: [UiNode], slots?, template?, recipe?}`.

## Themes (VAPP-92)

A theme is **data, never code**: one JSON file (`catalog/theme.schema.json`)
every renderer loads at runtime, safe to fetch from a URL or a vapp package.
Three levels of customisation, cheapest first: **tokens** (colours per mode,
radius, spacing, type, control heights, shadows, border widths, motion),
**recipes** (per component part × `when` selector a small style object), and a
**painter override** through the extension API (VAPP-91) that must keep the
measure contract (`src/geometry.ts`, `fixtures/control-geometry.json`).

```jsonc
{
  "id": "brand", "name": "Brand", "extends": "neutral",      // override only what changes
  "modes": { "light": { "color": { "primary": "#2563eb" }, "shadow": { "sm": [{ "x": 0, "y": 1, "blur": 2, "spread": 0, "color": "#0000000d" }] } }, "dark": { … } },
  "tokens": { "spacing": { "md": 12 }, "radius": { … }, "type": { "size": { … }, "lineHeight": { … }, "weight": { … }, "family": { "sans": "Inter" } }, "control": { … }, "opacity": { … }, "border": { … }, "motion": { … } },
  "fonts": { "Inter": { "fallback": "ui-sans-serif, system-ui", "weights": [400, 500, 600, 700], "source": "host" } },
  "recipes": {
    "Button": { "root": [
      { "style": { "borderRadius": "$radius.full" } },                       // the part's base
      { "when": { "variant": "outline", "state": "hover" }, "style": { "backgroundColor": "$color.accent" } }
    ] }
  }
}
```

- **Colours are `#rrggbb[aa]` only**; the builder's importer converts
  oklch/hsl/rgb (`importShadcnCss`). Shadows live under `modes` (they differ
  in the dark); every other group under `tokens`. Fonts are referenced by
  family; the host registers the files per platform (`fonts` says what to
  fall back to).
- **Recipes**: `recipes.<Component>.<part>` = a list of rules; a rule applies
  when every `when` entry matches (`state` = every listed state is active;
  any other key = the recipe prop equals the value or is in the list). Rules
  merge by SPECIFICITY (one point per `when` condition; ties in source order,
  later wins — VAPP-90, the order the web's CSS gives them), so a child
  theme's appended base rule never shadows its parent's `checked`/`focus`/
  `variant` rules; a child theme's rules come after its parent's.
  The keys a rule may set are `catalog/recipes.json` `keys` (the Box visual
  subset + padding/gap/size + `native`); values may be token references.
  Parts and `when` props per component: `recipeParts()` (natives from
  `recipes.json`, macros from their templates: every `part`, the macro's
  `recipeProps` + `$recipe` keys). States: hover, pressed, focus, disabled,
  checked, open, selected.
- **Precedence a painter applies** (`resolveNodeStyle`): the native's own
  recipe (Text/root for its variant) < the node's style (the macro
  template's structure + the author's style) < the macro part's recipe
  (Badge/label). Macro templates state only structure, spacing, radius and
  control heights; every colour and border is a recipe, so a theme restyles
  what a template drew.
- **Loading**: `validateTheme(json)` → issues with a path and a readable
  message (unknown token, key outside the whitelist, unknown part…);
  `loadTheme(json, {themes})` flattens the `extends` chain into a
  `ResolvedTheme` (every token present, recipes merged) or throws ONE
  `ThemeError` listing every issue; `tryLoadTheme` never throws. A root theme
  must give every token name a value. Built-ins: `builtinTheme(id)`,
  `BUILTIN_THEMES`, `DEFAULT_THEME_ID` (`exponential`).
- **Resolving**: `resolveRecipe(theme, {component, part, props, states}, mode)`
  → concrete values (hex, px, shadow layers, family); `resolveStyleValues`
  for a Box style; `styleToCss` for web painters.
- **Built-ins**: `neutral` = stock shadcn light + dark and the full recipe
  set (the root every other theme extends); `exponential` = the zinc glass
  look, GENERATED from `packages/design-tokens/tokens.json` + the app's
  `styles.css` palettes + `themes/exponential.recipes.json` so the app and
  the SDK cannot drift; `playful` = the deliberately different test theme
  (pill buttons, no card borders, underlined tabs, Nunito) the conformance
  suite renders on every painter. The natives get all three RESOLVED in
  `generated/ExponentialUIThemes.generated.*` (parse once at startup).
- **Builder** (`builder/`, `bun run --filter @exponential-at/ui dev:builder`
  → :4180): pick a base, edit tokens and recipes with the recipe sheet and
  the kitchen sink previewed live, import a shadcn `globals.css` / tweakcn
  export or a theme JSON, export the smallest `extends` theme
  (`diffTheme`). The preview is the real React renderer
  (`builder/preview.tsx` on `@exponential-at/ui-react`, VAPP-87): what the
  builder shows is what a host gets.

## The reference implementation (`src/`)

- `reduceSurface(components, {catalogId, extensions?})` — flat list → one
  normalized tree, basic components mapped, macros expanded, every unknown
  component the `Unknown` placeholder (never an error), prop issues listed.
  `reduceNested` does the same for the nested authoring form the fixtures use.
- `expandMacros(tree)` — the macro table applied until every node is native.
  The template language is documented in `macros.json`'s `$comment` and
  implemented in `expr.ts` (pure, line-by-line mirrorable in Rust).
- `validateProps(def, props)` — the mini schema (`types.ts` `PropSchema`).
- `validateStyle` / `create` / `props` — the VAPP-4 `vapp-css` over the whitelist.
- `defineExtension(def)` — an extension catalog: own id, `extends` the core,
  components (+ macro templates), no core name shadowed.
- `catalogPrompt({extensions?, lite?, terse?})` — the compact system prompt.
- `coreSchema(iconNames)` / `extensionSchema(ext, iconNames)` — JSON Schema.

## The host API (VAPP-91)

A surface needs exactly this from its host, the same shape on all four
platforms (TS here, the Rust core's `host` module, the Swift and Kotlin
painters through the facade); `catalog/host.json` is the contract and
`fixtures/host-{transport,policy,router}.json` lock it everywhere.

- **Transport**: messages in = the four A2UI v0.9 messages plus two
  extensions, `applyTemplate {surfaceId, templateId, packageId?, data?}` and
  `bindDataModel {surfaceId, path, source}`; client messages out = A2UI's
  `{version, action: {name, surfaceId, sourceComponentId, timestamp,
  context, payload?}}` or `{version, error: {code, surfaceId, message,
  path?}}`. Adapters: `MemoryTransport`, `JsonlStreamTransport`,
  `SseTransport`, `WebSocketTransport`, `McpTransport` (A2UI resources with
  `application/json+a2ui` in a tool result; actions back as tools/call
  `a2ui_event`), all on the shared decoders (`JsonlDecoder`, `SseDecoder`,
  `messagesFromMcpResult`).
- **Router**: `HostRouter.route(message)` → ops `create | components | data |
  bind | delete | send` (pure; errors `UNSUPPORTED_CATALOG`,
  `SURFACE_NOT_FOUND`, `INVALID_MESSAGE`, `TEMPLATE_NOT_FOUND` go back as
  `send`).
- **Functions**: an `on.<event>` `{functionCall: {call, args}}` (A2UI's key;
  `function` = the legacy alias) to a non-built-in name runs the host's
  registered function after the gate: `decideFunction(policy, name,
  registered)` → `allow | ask | deny | not_found` (deny wins, then allow,
  then ask, then `default`; `harness.*` prefixes), `ask` = the host's
  `onFunctionCall` consent hook; a package surface's `functions` list
  narrows it (`combineDecisions`). The 14 catalog functions are built in.
- **Bindings**: `source` URIs `<scheme>:<name>?k=v` (`parseSource`); a host
  registers a resolver per scheme (`subscribe(source, emit) → cancel`), each
  emit lands at the bound path.
- **Negotiation**: `supportedCatalogIds(extensionIds)` = core, core lite,
  A2UI basic, then the registered extensions; `clientCapabilities`.
- **Policy**: `decideUrl` (schemes `https http mailto tel`, optional host
  allowlist, relative urls against `baseUrl`) for `openUrl`/`Link`;
  `mediaRequest(url, {baseUrl, rules})` = the image loader's url + headers
  (auth for `/api/attachments`).
- **Runtime**: `new ExponentialHost({transport, functions, sources,
  extensions, packages, policy})`: `connect()`, `receive(message)`,
  `surface(id)` (a `SurfaceStore`), `action(...)`, `callFunction(...)`,
  `openUrl(url)`, `mediaRequest(src)`, `status` / `hasTransport` /
  `unsupportedCatalog` (the `host_offline` state and the catalog-update
  banner). React paints it with `<HostSurface host surfaceId>`.
- **Declarative vapps** (VAPP-82): a package `{id, name, version, catalogId,
  templates: {<id>: {components, data?, bindings?}}, functions?, theme?,
  icon?}` (`validatePackage`); `createVappHost({package, …})` runs one in any
  host. Against an Exponential instance, `ExponentialConnector` does the MCP
  OAuth grant (discovery, dynamic registration, PKCE S256, consent) and
  serves `exp:issues|boards|teams|members` over the instance's MCP tools
  (`sources()`, polled) plus `exponential.mcp` (`functions()`). Hosted vapps
  (the peer-link transport, VAPP-10) embed through the same `Transport`
  interface.

The Exponential app is a host like any other: web `apps/web/src/lib/exponential-ui-host.tsx`
(`harness.*` functions, `exp:` over the Electric collections, the consent
card, the app extension's painters from `@exp/ui`, the Devices package
`packages/ui/exponential-ui/templates/devices.json` on `/exponential-ui-devices`).
Samples outside the workspace: `samples/exponential-ui/` (a local A2UI
server, a third-party theme, one extension component, four hosts).

## Conformance

`conformance/README.md`: 15 suites, 1008 cases; a renderer is conformant
when `bun run --filter @exponential-at/ui conformance:check <report>` says
so. All four renderers and the core run it in CI (`exponential-ui.yml`).

## Fixtures (the contract; the Rust core replays them with these test names)

| file | locks |
|---|---|
| `catalog-components.json` | every component × its example × every enum value × both booleans |
| `catalog-macros.json` | every macro case → the expanded tree, byte for byte |
| `catalog-basic-map.json` | hand-written A2UI basic surfaces → expected trees + issues |
| `catalog-extension.json` | an example extension (native + macro + enum) and its cases |
| `kitchen-sink.json` / `.expanded.json` | every visible component once, the VAPP-4 layout cases kept; view id `exponential-ui-kitchen-sink` |
| `prompt-budget.json` | the prompt's size on record (full / lite / terse) and the budget |
| `theme-resolved.json` | every built-in theme resolved: what a native loader must produce from the same files |
| `theme-recipes.json` | theme × component part × recipe props → visuals per mode and state (one case per distinct look) |
| `theme-extends.json` | `extends` cases (the acceptance case: neutral + primary + button radius) → chain + probe styles |
| `theme-invalid.json` | bad themes → the issues they must raise (paths ×4, never a crash) |
| `control-geometry.json` | theme × control × props → the box a painter or override must measure to |
| `host-transport.json` / `host-policy.json` / `host-router.json` | the host API (VAPP-91): decoders, the gate / urls / media / sources / negotiation, router flows → ops |

## Commands

```bash
bun run --filter @exponential-at/ui generate   # idempotent; generate.test.ts gates drift
bun run --filter @exponential-at/ui test
bun run --filter @exponential-at/ui typecheck
```

Change a component = edit `core.catalog.json` (and `macros.json` for a macro),
run generate, commit everything it rewrote. The icon vocabulary is read from
`packages/icons/icons.json` at generate time; the exponential theme reads
`packages/design-tokens/tokens.json` and `packages/ui/src/styles.css` at
generate time too (never at runtime).

```bash
bun run --filter @exponential-at/ui dev:builder     # the theme builder on :4180
bun run --filter @exponential-at/ui build:builder   # bundles builder/dist (ignored)
```

## Extensions

The Exponential app's extension (IssueRow, RunRow, IssueChip, …) lives with
the app at `packages/ui/exponential-ui/extension.json` and is validated by
`@exp/ui`'s tests through `defineExtension`; it is the first user of the
mechanism and not part of this package.
