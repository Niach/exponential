# `@exponential-at/ui-react` — the Exponential UI React renderer

The REFERENCE renderer of Exponential UI (VAPP-84): an A2UI surface in the
core catalog (`@exponential-at/ui`) painted with real CSS on shadcn/Radix.
Themes load at runtime, extensions register their own components, overlays
ride Radix portals, inputs are host-owned, and the layout is the one the
Rust core computes (the geometry fixture holds the browser to taffy's
frames within a pixel). Published to npm from `release/` (VAPP-91); the
workspace package.json stays private on the sources.

It depends on `@exponential-at/ui`, React 18 or 19, `radix-ui` and
`lucide-react` (its own chrome glyphs) — nothing from the Exponential app, no
TanStack, no Electric, no tRPC. Tailwind is an internal build detail of the
primitive set (below); the renderer itself ships no stylesheet an embedder
has to wire up.

```tsx
import { ExponentialSurface, useSurface } from "@exponential-at/ui-react"

function Chat({ messages }) {
  const surface = useSurface({ surfaceId: "main" })
  useEffect(() => messages.forEach(surface.apply), [messages])   // A2UI v0.9 server messages
  return (
    <ExponentialSurface
      surface={surface}
      theme="exponential"                     // or "neutral" | "playful" | a theme JSON | a ResolvedTheme
      mode="dark"
      host={{ icons, onAction, onInput }}     // the host plugin (below)
    />
  )
}
```

## Layout

| path | what |
|---|---|
| `src/surface.tsx` | `<ExponentialSurface>`: root (theme scope + variables, `data-xui-mode`, `dir`), the two `<style>`s, the `xui` container, the tree |
| `src/use-surface.ts` | `useSurface`: the TS reducer as React state; A2UI messages in, normalized tree + data model out |
| `src/theme-css.ts` | a resolved theme → one scoped sheet (`--xui-*` variables per mode, recipe classes, the shadcn/glass aliases) |
| `src/box-css.ts` | a surface's node sheet: the whitelisted `style`s, `@container xui (min-width)`, `:pressed` → `:active` |
| `src/base-css.ts` | layer `xui-base`: the three CSS-equals-taffy rules + each native's structure |
| `src/node-view.tsx` | one node → one element: bindings resolved, painter picked, root attributes, the `emit` for `on` |
| `src/natives/` | the ~36 native painters (Box, Text, Button… Dialog, Select, Composer, Unknown) |
| `src/data.ts` | the data model (JSON pointers), `{path}` bindings, the 14 client functions |
| `src/inputs.ts` | `useHostOwnedValue`: local state, 150 ms debounce, revisions, echo rule |
| `src/list.tsx` | `WindowedList`: item key, estimated height, overscan |
| `src/extensions.ts` | `registerExtension` / `defineReactExtension` |
| `src/primitives/` | the shadcn set (moved from `@exp/ui`, which re-exports it) + `tailwind.css`; `bun run build:css` → `dist/exponential-ui-react.css` |
| `harness/` | the browser harness the Chromium suites and the screenshot script drive (`dev:harness` → :4181) |
| `browser/` | the headless-Chromium suites: geometry, overlays, typing (`test:browser`) |
| `scripts/` | `build-css.ts`, `shots.ts` (the stored kitchen-sink shots), `pack-smoke.ts` (a plain Vite app on the packed tarballs) |

## How a theme becomes CSS

`compileTheme(theme)` emits, scoped by a class derived from the theme's
content (`xui-t-<id>-<hash>`, on the root AND on every painted element, so two
surfaces on one page wear two themes and a host page's own variables are
never touched):

1. **Variables** on `.xui-surface.<scope>`: every token as `--xui-<group>-<name>`
   (`--xui-spacing-md: 12px`, `--xui-type-lineHeight-sm`, `--xui-font-sans: "Inter", …`),
   the colours and shadows per `[data-xui-mode="light|dark"]`, plus the shadcn
   names (`--primary`, `--radius`, `--font-sans`…) and the app's `--glass-*`
   set, so a Tailwind-built primitive inside a surface wears the theme too.
2. **Recipe classes**: `recipes.<Component>.<part>` → `.xui-<Component>-<part>`.
   A rule's `when` on recipe props becomes `[data-r-<prop>="…"]` (natives) or
   `[data-m-<prop>="…"]` (macro parts: a Badge's label is a Text, both key on
   `variant`); `state` becomes the pseudo-class or Radix attribute AND the
   forced marker `[data-xs~="<state>"]` the builder's recipe sheet sets.
   Values stay `var(--xui-…)`, so a mode switch is one attribute flip.
3. **Layers**: `@layer xui-base, xui-recipe, xui-node, xui-part` IS the painter
   precedence of `resolveNodeStyle`: a native's own recipe < the node's `style`
   < the macro part's recipe.

The node sheet (`nodeSheet`) is per surface: `.xui-s-<id> .xui-n-<nodeId>`
rules from each node's whitelisted style (tokens as variables), every
`@media (min-width: Npx)` as `@container xui (min-width: Npx)` against the
surface's own container, `:pressed` as `:active`.

### The three rules that make CSS agree with taffy (VAPP-4)

Every painted element restates `box-sizing: border-box; display: flex;
flex-direction: row; position: relative` and leaves `min-width: auto`; chrome
is paint or inside the measured size (borders start at 0, a recipe's
`borderWidth` is layout in both engines); `overflow: hidden` is emitted as
`clip`. `fixtures/layout-geometry.json` (the spike's kitchen sink, taffy's
frames at 900/390 px × LTR/RTL, a fixed fake measure) is replayed in headless
Chromium by `browser/geometry.test.ts`: every node within 1 px. In geometry
mode (`measure` prop) a leaf renders as a fixed box with no recipe on it.

## The host plugin

```ts
interface HostPlugin {
  icons?: IconMap                                   // registry name → component (the app passes its Lucide map)
  onAction?(e: SurfaceActionEvent): void | Promise<void>   // on.<event> = {event: {name, context}}; a promise keeps the Button pending
  onInput?(e: SurfaceInputEvent): void | Promise<void>     // host-owned edits: {name, path?, value, revision, kind: change|commit}
  openUrl?(url: string): void
  functions?: Record<string, ClientFunction>        // adds to / overrides the 14 catalog functions
  Markdown?: ComponentType<{ text: string }>       // a richer renderer than the built-in GFM subset
  resolveUrl?(src: string): string
  onUnknown?(node: UiNode): void
}
```

- **Actions** are optimistic: a Button is disabled (`aria-busy`,
  `data-xs~="pending"`) from the press until `onAction`'s promise settles.
  A `functionCall` action runs client-side (`openUrl` → `host.openUrl`).
- **Host-owned inputs** (`Input`, `Textarea`, `Composer`): the value lives in
  local state; every edit bumps a revision and, 150 ms later, sends ONE
  `change` with the latest value; `commit` goes out on blur/Enter. A value
  the host pushes (the data model, a new prop) is adopted only when the
  field is not focused and every sent revision is acknowledged (the promise
  settled). A bound `value` (`{path}`) is also written through to the
  surface's data model. `browser/typing.test.ts`: 40 keys at 150 ms RTT,
  none dropped, three runs.
- **Bindings**: `{path}` is a JSON pointer into `surface.data` (relative
  inside a template item); `{call, args}` runs a client function. The
  catalog's `checks` show the first failing message under the field per
  `validateOn`.
- **Templates**: `children: {componentId, path}` renders the component once
  per item at `path`; `useSurface` keeps the flat list so the template node
  resolves (`templateNode`).

### The host API (VAPP-91)

An `ExponentialHost` from `@exponential-at/ui` (transport, router, sources,
functions, policy; see that package's README) feeds `<HostSurface host
surfaceId plugin? …ExponentialSurface props>`; `hostPlugin(host, base)`
routes actions to A2UI client messages, `functionCall`s to
`host.callFunction` (the gate + consent), `openUrl` to the URL policy and
media to `host.mediaRequest`. `useHostSurfaceIds(host)` and
`useHostStatus(host)` (`status`, `unsupportedCatalog`) drive a host's own
chrome (`host_offline`, the catalog-update banner). Two plugin members
joined: `onFunctionCall(call)` and `mediaRequest(src)` (a request with
headers is fetched once and shown as a blob url). Template items (a List's
`{componentId, path}` rows) wear their component's node styles.

`browser/suites.test.ts` is the renderer's conformance runner (every
suite of the manifest; `test:conformance`).

## Overlays

Dialog, Drawer, Popover, Tooltip, DropdownMenu, Select and DatePicker portal
INTO the surface root (not `document.body`), outside the `xui` container's
containment, so the theme's variables and scope reach them and
`position: fixed` still means the viewport. Placement follows the core's
rule (`placeOverlay` in `@exponential-at/ui`: `OVERLAY_OFFSET` 4 px, centred,
flip when the preferred side lacks room, shift `OVERLAY_PADDING` 8 px inside
the viewport); `browser/overlay.test.ts` holds Radix's popper to
`fixtures/overlay-geometry.json` within 2 px, same side and flip. A trigger
slot is wrapped in an inline-flex span the popper measures. `open` is a
controlled prop with a local mirror (`change` reports the user's toggles).

## Extensions

```ts
import { registerExtension } from "@exponential-at/ui-react"
registerExtension({ catalog: myExtensionJson, components: { Sparkline } })   // page-wide
<ExponentialSurface extensions={[defineReactExtension({ catalog, components })]} />  // per surface
```

A component receives `{ node, props (resolved), theme, mode, tokens, children,
slots, emit, rootProps }` and spreads `rootProps` on its root element (the node
class, the theme scope, the part classes, the recipe `data-*`). Macros of the
extension expand in the reducer and need no component; their parts go in the
`xui-part` layer. An entry for a CORE native (`{ Switch: MySwitch }`) is the
VAPP-92 painter override.

## Lists

`WindowedList({ count, itemKey, estimatedItemHeight, overscan, renderItem })`
positions the visible window absolutely inside a spacer, corrects estimates
with a `ResizeObserver` and follows the nearest scrolling ancestor. The `List`
native uses it past `WINDOW_THRESHOLD` (24) items; fewer render flat so the
fixtures snapshot every row.

## The primitive set

`src/primitives/` holds the generic shadcn/Radix components the app and
extension authors share (button, input, select, dialog, sheet, tabs, the glass
rows, pill, meter, composer, calendar, … plus radio-group, slider, toggle,
toggle-group, carousel, accordion, pagination, button-group, spinner, table),
themed only through the surface's variables. `@exp/ui` re-exports every one
(its old paths are one-line shims), and its `styles.css` scans this directory
so the app's Tailwind build still emits their classes. An embedder without
Tailwind takes the compiled `dist/exponential-ui-react.css`
(`bun run --filter @exponential-at/ui-react build:css`).

## Commands

```bash
bun run --filter @exponential-at/ui-react test            # jsdom: fixtures → DOM, inputs, data, theme css, primitives
bun run --filter @exponential-at/ui-react test:browser    # headless Chromium: geometry, overlays, typing
bun run --filter @exponential-at/ui-react typecheck
bun run --filter @exponential-at/ui-react dev:harness     # http://localhost:4181/?view=kitchen-sink&theme=playful
bun run --filter @exponential-at/ui-react shots           # shots/exponential-ui-kitchen-sink/{web,web-mobile}.webp
bun run --filter @exponential-at/ui-react build:css
bun run --filter @exponential-at/ui-react smoke:pack      # packs both packages, builds a plain Vite app on them, renders the sink
```

The app's `/exponential-ui-kitchen-sink` route (view `exponential-ui-kitchen-sink`)
renders the same fixture with the app's icon registry; the theme builder
(`packages/exponential-ui/builder`) previews its recipe sheet and the sink
through this renderer.
