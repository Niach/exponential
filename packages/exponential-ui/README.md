# `@exponential-at/ui` — the Exponential UI catalog

The catalog half of Exponential UI (VAPP-84): the **core catalog** of
components an A2UI surface may use, the **A2UI basic catalog** vendored and
mapped onto it, the **macro table** that turns composite components into
natives, the **model-facing descriptions** and prompt, the **token names**
themes must define, codegen for TypeScript, Swift, Kotlin and Rust, and the
fixtures every renderer suite locks against. No renderer lives here (React:
VAPP-87, the Rust core: VAPP-86, painters: VAPP-88/89/90); themes are VAPP-92.

`"private": true` until VAPP-91 publishes it. Depends on nothing from the
Exponential app.

## Layout

| path | what |
|---|---|
| `catalog/core.catalog.json` | THE source: ids, enums, shared shapes, functions, 61 components with props + descriptions |
| `catalog/macros.json` | the declarative expansion table, one template per macro component |
| `catalog/basic-map.json` | A2UI basic → core: components (+ named transforms), icons, functions |
| `catalog/style.json` | the `Box` style whitelist (VAPP-4), one source for TS / schema / natives |
| `catalog/tokens.json` | the token NAMES (`$color.primary`, `$spacing.md`, …); values come from a theme |
| `catalog/core.schema.json` | generated: the catalog as JSON Schema in A2UI's catalog shape |
| `src/` | the TS reference implementation (below) |
| `generated/*.{swift,kt,rs}` | generated constants for the three native targets |
| `docs/components.generated.json` | generated: one doc page per component as data (ui.exponential.at renders it, VAPP-93) |
| `fixtures/` | the contract (below) |
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

## Fixtures (the contract; the Rust core replays them with these test names)

| file | locks |
|---|---|
| `catalog-components.json` | every component × its example × every enum value × both booleans |
| `catalog-macros.json` | every macro case → the expanded tree, byte for byte |
| `catalog-basic-map.json` | hand-written A2UI basic surfaces → expected trees + issues |
| `catalog-extension.json` | an example extension (native + macro + enum) and its cases |
| `kitchen-sink.json` / `.expanded.json` | every visible component once, the VAPP-4 layout cases kept; view id `exponential-ui-kitchen-sink` |
| `prompt-budget.json` | the prompt's size on record (full / lite / terse) and the budget |

## Commands

```bash
bun run --filter @exponential-at/ui generate   # idempotent; generate.test.ts gates drift
bun run --filter @exponential-at/ui test
bun run --filter @exponential-at/ui typecheck
```

Change a component = edit `core.catalog.json` (and `macros.json` for a macro),
run generate, commit everything it rewrote. The icon vocabulary is read from
`packages/icons/icons.json` at generate time.

## Extensions

The Exponential app's extension (IssueRow, RunRow, IssueChip, …) lives with
the app at `packages/ui/exponential-ui/extension.json` and is validated by
`@exp/ui`'s tests through `defineExtension`; it is the first user of the
mechanism and not part of this package.
