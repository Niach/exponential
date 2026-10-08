# `@exponential-at/ui` — the Exponential UI catalog

The data half of Exponential UI (VAPP-84): the **core catalog** of
components an A2UI surface may use, the **A2UI basic catalog** vendored and
mapped onto it, the **macro table** that turns composite components into
natives, the **model-facing descriptions** and prompt, the **token names**
themes must define, the **runtime theme format** with the three built-in
themes and the theme builder (VAPP-92), codegen for TypeScript, Swift, Kotlin
and Rust, and the fixtures every renderer suite locks against. No renderer
lives here (React: VAPP-87, the Rust core: VAPP-86, painters: VAPP-88/89/90).

`"private": true` until VAPP-91 publishes it. Depends on nothing from the
Exponential app.

## Layout

| path | what |
|---|---|
| `catalog/core.catalog.json` | THE source: ids, enums, shared shapes, functions (the basic 14 + the 15 core ones, `functions.core`), the built-in glyphs (`builtinIcons`), 82 components with props + descriptions |
| `catalog/macros.json` | the declarative expansion table, one template per macro component |
| `catalog/basic-map.json` | A2UI basic → core: components (+ named transforms), icons, functions |
| `catalog/style.json` | the `Box` style whitelist (VAPP-4), one source for TS / schema / natives |
| `catalog/tokens.json` | the token NAMES (`$color.primary`, `$spacing.md`, `$breakpoint.md`, `$ease.standard`, …); values come from a theme |
| `catalog/strings.json` | round 1: the renderers' built-in UI copy (ids + English defaults), overridable per surface, referenced as `$string.<id>` |
| `catalog/locale.json` | round 1: week start by region, likely regions (CLDR), deprecated language aliases, the RTL languages, the glyphs mirrored under RTL |
| `catalog/code.json` | round 1: CodeBlock's built-in tokenizer (rules + per-language specs) |
| `catalog/a11y.json` | round 1: the machine-readable role vocabulary; per component its role, notes and keyboard expectations; the rules; the host commands (focus, announce, scrollIntoView). Macro parts carry role/states/name via `$a11y` (macros.json) |
| `catalog/recipes.json` | the recipe contract: interaction states (+ `invalid`, `dragover`), the recipe key whitelist (+ motion, transform, per-side borders), every native's parts + `when` props (macro parts come from macros.json) |
| `catalog/core.schema.json` | generated: the catalog as JSON Schema in A2UI's catalog shape |
| `catalog/theme.schema.json` | generated: a theme file as JSON Schema |
| `themes/` | the built-in themes: `neutral` (stock shadcn, the root), `exponential` (GENERATED: design-tokens values + `exponential.recipes.json`), `playful` (the test theme) |
| `builder/` | the theme builder page (`dev:builder` / `build:builder`); ui.exponential.at mounts it (VAPP-93) |
| `src/` | the TS reference implementation (below) |
| `generated/*.{swift,kt,rs}` | generated constants for the three native targets; `*Themes*` = the built-ins RESOLVED, embedded as JSON |
| `docs/components.generated.json` | generated: one doc page per component as data (ui.exponential.at renders it, VAPP-93) |
| `docs/themes.generated.json` | generated: the token vocabulary with every built-in's values, the recipe contract per component |
| `fixtures/` | the contract (below) |
| `docs/round-1-contract.md` | the renderer-hardening round: every contract change with notes per renderer |
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
  checked, open, selected, invalid, dragover.
- **Precedence a painter applies** (`resolveNodeStyle`): the native's own
  recipe (Text/root for its variant) < the node's style (the macro
  template's structure + the author's style) < the macro part's recipe
  (Badge/label). Macro templates state only structure, spacing, radius and
  control heights; every colour and border is a recipe, so a theme restyles
  what a template drew.
- **Surface settings** (round 1): `mode` light | dark | system
  (`resolveMode`), `density` compact | default | comfortable
  (`applyDensity` scales `$control.*` and `$spacing.*` by the theme's
  `$density.*`), high contrast (`applyContrast` merges the theme's
  `contrast` overlays), `$breakpoint.*` for style conditions and responsive
  props, `$ease.*` for transitions; `shadow` none → xl is the elevation scale.
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

- Round 1 (`docs/round-1-contract.md`): `dynamic.ts` (the BIND pass: `resolveDynamic`, `bindTree` with props along the schema and DATA props verbatim, per-row Table slot cells via `bindRowSlot`, `runAction` with the `set` write-back), `strings.ts`, `locale.ts`, `code.ts` (CodeBlock tokenizer), `chart.ts` (extents, nice ticks), `a11y.ts`, and in `style.ts` the condition resolver (`resolveConditions`, `activeBreakpoint`); `theme.ts` gains `resolveMode`, `applyDensity`, `applyContrast`.
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
| `catalog-macros.json` | every macro case → the expanded tree, byte for byte; round 1 adds a `bound:<prop>` case per bindable macro prop and a `responsive:<prop>` case per responsive one |
| `bind-time.json` | round 1: every bound macro case × three data models (sample, falsy twin, path missing) → the tree after the bind pass + every `set` press; `extra` = Table slot cells bound per row (`rowSlots`), literal data rows, a bound accessible name, the `$set` collision issue, string page data |
| `style-conditions.json` | round 1: style × surface/state context → the flattened style (media in source order, then states) |
| `code-tokens.json` | round 1: source × language → CodeBlock tokens per line |
| `catalog-basic-map.json` | hand-written A2UI basic surfaces → expected trees + issues |
| `catalog-extension.json` | an example extension (native + macro + enum) and its cases |
| `kitchen-sink.json` / `.expanded.json` | every visible component once, the VAPP-4 layout cases kept, round 1's responsive section (1→2→3 column grid, collapsing sidebar, hidden-on-narrow nodes, state blocks) and bound macros; view id `exponential-ui-kitchen-sink` |
| `prompt-budget.json` | the prompt's size on record (full / lite / terse) and the budget |
| `theme-resolved.json` | every built-in theme resolved: what a native loader must produce from the same files |
| `theme-recipes.json` | theme × component part × recipe props → visuals per mode and state (one case per distinct look) |
| `theme-extends.json` | `extends` cases (the acceptance case: neutral + primary + button radius) → chain + probe styles |
| `theme-invalid.json` | bad themes → the issues they must raise (paths ×4, never a crash) |
| `control-geometry.json` | theme × control × props → the box a painter or override must measure to |

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
