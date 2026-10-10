# `@exponential-at/ui-react` — the Exponential UI React renderer

The REFERENCE renderer of Exponential UI (VAPP-84): an A2UI surface in the
core catalog (`@exponential-at/ui`) painted with real CSS on shadcn/Radix.
Themes load at runtime, extensions register their own components, overlays
ride Radix portals, inputs are host-owned, and the layout is the one the
Rust core computes (the geometry fixture holds the browser to taffy's
frames within a pixel). Published to npm from `release/` (VAPP-91); the
workspace package.json stays private on the sources.

It depends on `@exponential-at/ui`, React 19 (React 18 would keep
`javascript:` hrefs clickable), `radix-ui` and
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
      mode="system"                           // light | dark | system (default; follows the platform live)
      locale="de-AT"                          // formatting, calendar names, week start, text direction
      timeZone="Europe/Vienna"                // dates and relative times (default: the platform's)
      strings={{ search: "Suchen…" }}         // built-in copy overrides (catalog/strings.json ids)
      density="compact" contrast="system"     // the theme's density multiplier; high-contrast overlays
      host={{ icons, onAction, onInput, onUpload }}     // the host plugin (below)
      handleRef={ref}                         // ref.current.run({ focus: { id } } | { announce } | { scrollIntoView } | { scrollToIndex })
    />
  )
}
```

## Layout

| path | what |
|---|---|
| `src/surface.tsx` | `<ExponentialSurface>`: root (theme scope + variables, `data-xui-mode`, `dir`, `data-xq`), the two `<style>`s, the `xui` container with the tree + the overlay and toast LAYERS, the live regions, the settings (locale, strings, mode/density/contrast) and the host command handle |
| `src/use-surface.ts` | `useSurface`: the TS reducer as React state; A2UI messages in, normalized tree + data model out |
| `src/theme-css.ts` | a resolved theme → one scoped sheet (`--xui-*` variables per mode, recipe classes, the shadcn/glass aliases) |
| `src/box-css.ts` | a surface's node sheet (template subtrees included): the whitelisted `style`s, the `@media` grammar (width → container queries, height/orientation → `data-xq`, hover/reduced-motion → media), the state blocks, dynamic values → `var(--xd-n)` |
| `src/base-css.ts` | layer `xui-base`: the three CSS-equals-taffy rules + each native's structure |
| `src/node-view.tsx` | the BIND pass per node (`visible`, props, dynamic style values, recipe props, `$string`, responsive native props), painter picked, root attributes, the action runner (`set` then the event), keyed templates |
| `src/natives/` | the 46 native painters (Box, Text, Button… Form, NumberField, ChipInput, Date/Range/TimePicker, FileUpload, CodeBlock, Table, Chart, Menu, Segmented, Toast, Resizable, TreeGuides, Unknown); `natives.test.ts` gates them against the catalog |
| `src/form.tsx` | the Form native + the field protocol every named control speaks (`useField`) |
| `src/platform.ts` | live platform preferences (`matchMedia`) and the measured surface box |
| `src/data.ts` | the data model (JSON pointers), `{path}` bindings, the basic + core client functions (the format functions through the surface Formatter), `bindTree` |
| `src/inputs.ts` | `useHostOwnedValue`: local state, 150 ms debounce, revisions, echo rule |
| `src/list.tsx` | `WindowedList`: one axis, item key, estimated extent, gap + dividers, overscan, sticky rows, one ResizeObserver, `scrollToIndex` |
| `src/extensions.ts` | `registerExtension` / `defineReactExtension` |
| `src/primitives/` | the shadcn set (moved from `@exp/ui`, which re-exports it) + `tailwind.css`; `bun run build:css` → `dist/exponential-ui-react.css` |
| `harness/` | the browser harness the Chromium suites and the screenshot script drive (`dev:harness` → :4181; views kitchen-sink, geometry, overlay, catalog, conditions, tree, round2, bench) |
| `browser/` | the headless-Chromium suites: geometry, overlays, typing, round 1 (conditions, overlays × breakpoints, kitchen sink × themes × widths, keyboard) (`test:browser`) |
| `fixtures/kitchen-sink.data.json` | the data model the kitchen sink renders against (harness + the app's view) |
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

The node sheet (`compileNodeSheet`) is per surface: `.xui-s-<id> .xui-n-<nodeId>`
rules from each node's whitelisted style (tokens as variables), then each
`@media` block in SOURCE order (round-1 contract §2; later wins):

| condition | CSS |
|---|---|
| `min-width: N` / `max-width: N` (`N` px or `$breakpoint.*`, resolved from the theme) | `@container xui (width >= N)` / `(width < N)` — strict, so `min md` and `max md` never overlap |
| `min-height` / `max-height` / `orientation` | `.xui-s-<id>:where([data-xq~="<token>"]) …` — the surface lists the matching conditions in `data-xq` (an inline-size container cannot answer height). The height is the VIEWPORT's, like the Rust core's `set_viewport` height: the `viewportHeight` prop, else an explicit inline `style.height` on the surface, else UNKNOWN (landscape, no min/max-height match — what the gpui host passes today); never the content height (no feedback loop) |
| `hover: hover\|none`, `prefers-reduced-motion` | real `@media` |
| `:hover`, `:focus-visible`, `:pressed` | after every media rule, in that order: `:hover`/`[data-xs~=hover]`, `:focus-visible`, `:active`/`[data-xs~=pressed]` |

A value the macro expander left DYNAMIC (Progress `width: percent{…}`) is
emitted as `var(--xd-<n>)` and the node sets the resolved value inline, so the
layer order holds; a value that resolves to nothing is set to `initial`, so the
property falls back instead of inheriting an ancestor's `--xd-<n>`. Template subtrees are part of the sheet and their items
keep the template node's class (ids get the item's suffix), so a templated row
wears its author and macro styles. New style keys map like `styleToCss`:
`transition` → `transition-property: all` + `transition-duration` (the easing
is `transition-timing-function`, `$ease.*` → `var(--xui-ease-*)`), gradients
→ `background-image`, per-side widths default to `solid`, `overflowX/Y:
hidden` → `clip`. Reduced motion (the media query or `data-xui-motion`)
zeroes every duration.

### The three rules that make CSS agree with taffy (VAPP-4)

Every painted element restates `box-sizing: border-box; display: flex;
flex-direction: row; position: relative` and leaves `min-width` AND
`min-height` at `auto` (taffy's automatic minimum size; round 1 dropped the
old `min-height: 0`, the geometry replay stays within 1 px); chrome
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
  openUrl?(url: string): void                       // opens an href the URL policy allowed
  urls?: UrlPolicy                                  // EVERY href (Link, markdown, FileUpload, openUrl); denied = text
  media?: MediaOptions                              // EVERY src without a mediaRequest (schemes, hosts, rules)
  mediaRequest?(src: string): MediaRequest | null   // null = denied; re-checked against media's schemes/hosts
  onPaintError?(e: { surfaceId, componentId, message }): void   // a painter threw (it paints an empty box)
  functions?: Record<string, ClientFunction>        // adds to / overrides the catalog functions
  Markdown?: ComponentType<{ text: string }>       // a richer renderer than the built-in GFM subset
  resolveUrl?(src: string): string                 // rewrites a media src BEFORE the media policy
  onUnknown?(node: UiNode): void
  onUpload?(files: File[], target: { nodeId, name }): void | Promise<void>   // FileUpload bytes (the event carries metadata only)
  optionSource?(source: string, query: string): Option[] | Promise<Option[]> // Select `source` (a host list)
}
```

- **Actions** are optimistic: a Button is disabled (`aria-busy`,
  `data-xs~="pending"`) from the press until `onAction`'s promise settles.
  An action resolves its function args AND event context against the data
  as it is, then runs the function (`set` writes the data model, two-way
  binding for macros; `openUrl` → `host.openUrl`), then sends the event with
  the component's payload merged OVER the author's context (the payload wins
  a clashing key, like the Rust core's `fire`).
- **Re-renders**: the surface keeps its measured box in a ref and re-renders
  only when the active breakpoint or `data-xq` changes; nodes are memoized,
  so a parent re-render (or a resize inside a breakpoint) repaints nothing.
- **Template instance ids** accumulate: an item's nodes are
  `<id><outer suffixes>.<key>` (`cell.a.0`), the key = the `template.key`
  value, the index without a key, `#<index>` when the key is missing, null,
  empty or a duplicate. A Table slot cell appends the row key the same way.
- **Host-owned inputs** (`Input`, `Textarea`, `Composer`): the value lives in
  local state; every edit bumps a revision and, 150 ms later, sends ONE
  `change` with the latest value; `commit` goes out on blur/Enter. A value
  the host pushes (the data model, a new prop) is adopted only when the
  field is not focused and every sent revision is acknowledged (the promise
  settled). A bound `value` (`{path}`) is also written through to the
  surface's data model. `browser/typing.test.ts`: 40 keys at 150 ms RTT,
  none dropped, three runs.
- **Bindings**: `{path}` is a JSON pointer into `surface.data` (relative
  inside a template item); `{call, args}` runs a client function (the bind
  table `bindFunctionNames`; truthiness = the catalog's, `0` is true; the
  format functions run through the surface Formatter);
  `$string.<id>` resolves through the surface's string table. A falsy
  `visible` drops the node. `bindTree` is the whole-tree form, replayed
  against `fixtures/bind-time.json`.
- **Bindable state**: a native whose state prop is bound (Dialog `open`,
  Tabs `value`, Checkbox `checked`, Table `sort`/`selected`, ChipInput
  `values`, …) writes the user's change to the path itself AND fires
  `change`.
- **Forms**: every named control registers with the nearest `Form`; a
  submit (a `submit` Button, Enter in a single-line field) runs every
  field's checks; a failure fires `invalid {errors}`, shows ALL failing
  messages under each field (`<id>.error`, `role=alert`, linked by
  `aria-describedby`, the `invalid` recipe state), the `summary`, moves
  focus to the first invalid field and announces `invalidFields`.
- **Templates**: `children: {componentId, path, key?}` renders the component
  once per item at `path`, keyed by `key` (reordering keeps the item's
  state). The component comes from the reducer's LIFTED `templates`
  (`useSurface` keeps them; a fixture passes `templates` beside `root`); it
  never renders in place.

### The host API (VAPP-91)

An `ExponentialHost` from `@exponential-at/ui` (transport, router, sources,
functions, policy; see that package's README) feeds `<HostSurface host
surfaceId plugin? …ExponentialSurface props>`; `hostPlugin(host, base)`
routes actions to A2UI client messages, `functionCall`s to
`host.callFunction` (the gate + consent), every href to the URL policy,
every src to `host.mediaRequest` (fetched under the contract's
`media.limits`: bytes, timeout, header pixels) and `onPaintError` to
`host.paintError` (an A2UI `RENDER_FAILED` error). `useHostSurfaceIds(host)` and
`useHostStatus(host)` (`status`, `unsupportedCatalog`) drive a host's own
chrome (`host_offline`, the catalog-update banner). Two plugin members
joined: `onFunctionCall(call)` and `mediaRequest(src)` (a request with
headers is fetched once and shown as a blob url). Template items (a List's
`{componentId, path}` rows) wear their component's node styles.

`browser/suites.test.ts` is the renderer's conformance runner (every
suite of the manifest; `test:conformance`).

## Overlays

Dialog, Drawer, Popover, Tooltip, Menu, Select and the
pickers portal into the surface's OVERLAY LAYER inside the `xui` container
(not `document.body`): the theme's variables and scope reach them, container
breakpoints still match inside a Dialog (round 1), and an inline-size
container does not trap `position: fixed`. Toasts portal into the TOAST layer
above it (bottom-centre below `md`, bottom-end from `md`, newest at the edge,
three visible; the duration pauses on hover/focus). Enter/exit animate on the
theme's motion and easing tokens. Placement follows the core's
rule (`placeOverlay` in `@exponential-at/ui`: `OVERLAY_OFFSET` 4 px, centred,
flip when the preferred side lacks room, shift `OVERLAY_PADDING` 8 px inside
the viewport); `browser/overlay.test.ts` holds Radix's popper to
`fixtures/overlay-geometry.json` within 2 px, same side and flip. A trigger
slot is wrapped in an inline-flex span the popper measures. `open` is a
controlled prop with a local mirror (`change` reports the user's toggles).

## Round 1 natives (contract §3)

| native | behaviour |
|---|---|
| Form | field collection, submit gating, every failing message, summary, focus + announce, `busy`, `disabled` (a disabled fieldset) |
| NumberField | `role=spinbutton`, steppers, Arrow ± step (Shift ×10), PageUp/Down, Home/End, clamp on commit, locale formatting/parsing |
| ChipInput | Enter / comma add, Backspace focuses then removes the last chip, arrows between chips, suggestions listbox |
| DatePicker, DateRangePicker | one keyboard grid (arrows, PageUp/Down ± month, Shift ± year, Home/End by the locale week start), Intl month/weekday names, two-step range |
| TimePicker | a listbox of `step`-minute times in the locale's clock, type-ahead |
| Select | Radix when plain; searchable/multiple = the combobox (aria-activedescendant, `search {query}` debounced, bound options = async source, `host.optionSource`) |
| FileUpload | drop zone button, `dragover` state, accept/maxSize refusals, bytes → `host.onUpload`, removable files |
| CodeBlock | the catalog tokenizer (`CodeBlock/token` per kind), gutter, highlighted lines, wrap, `maxLines`, copy + `copied` announced |
| Table | typed cells (locale number/date, tick, badge, slot with the ROW as scope), sort (local or bound), single/multiple selection with select-all, striped odd rows, sticky header, windowing past 50; ONE roving row tab stop (focused, else first selected, else first), ArrowUp/Down + Home/End by index (through a windowed body), Enter presses, Space selects; slot edits over LITERAL rows stay table-local (never the data model, never a host path) |
| Chart | measured width, `niceTicks` axes + grid, bar/stackedBar/line/area/pie/donut (a ring per series)/sparkline, values, legend (2+), tooltip on hover and ArrowLeft/Right, an sr-only data table; cartesian kinds MIRROR in RTL (first category on the right) |
| Menu | item / checkbox (bound `checked` written) / separator / label / submenu (bound `items` render like literal ones), shortcuts; `openOn: press` = its child (or a default outline button) is the trigger, `contextmenu` = right-click / long-press / Shift+F10 / Menu key over the child |
| Toast | the toast layer, duration paused on hover/focus, `open` written false, `dismiss` + `change`, error = assertive |
| Image / Textarea / Drawer / Popover / Text / Markdown | fallback glyph + focal point + `loading`; autosize; `dismissible` + drag-to-dismiss + responsive `side`; `openOn: hover` (+ keyboard focus); `live`; `lines` |

Owned parts draw the catalog's `builtinIcons` glyph (host registry first,
then the same Lucide glyphs icons.json maps), and every piece of built-in
copy comes from the string table.

## Extensions

```ts
import { registerExtension } from "@exponential-at/ui-react"
registerExtension({ catalog: myExtensionJson, components: { TrendLine } })   // page-wide
<ExponentialSurface extensions={[defineReactExtension({ catalog, components })]} />  // per surface
```

A component receives `{ node, props (resolved), theme, mode, tokens, children,
slots, emit, rootProps }` and spreads `rootProps` on its root element (the node
class, the theme scope, the part classes, the recipe `data-*`). Macros of the
extension expand in the reducer and need no component; their parts go in the
`xui-part` layer. An entry for a CORE native (`{ Switch: MySwitch }`) is the
VAPP-92 painter override.

## Lists

`List` renders its children, then the template's items. Past the catalog's
`WINDOW_THRESHOLD` (50), or with `stickyHeaders`, it windows
(`WindowedList`): one axis (`direction` vertical or horizontal), rows end to
end with the gap (+ the hairline when `divided`, the divider centred in it),
unmeasured rows at `$control.row`, ONE `ResizeObserver`. The window follows
the nearest ancestor that actually scrolls on that axis, else the viewport.
The numbers are the core's (`virtualWindow`, `scrollOffsetForIndex`,
`stickyHeader`). Fewer items render flat, so fixtures snapshot every row.

| feature | web |
|---|---|
| `sectionBy` | consecutive items share a header: the `section` slot bound to `{value, count, index}`, else the value; `role=heading` level 3, part `<id>.section.<i>` |
| `stickyHeaders` | the current header pins at the top, pushed back by the next one; drawn even outside the window |
| a11y | every item `role=listitem` with `aria-setsize` / `aria-posinset` of the WHOLE list |
| `scrollToIndex` | host command `{scrollToIndex: {id, index, align}}` on List and Table (data index); re-aimed once the revealed rows are measured |

Bench (`fixtures/bench-list.json`: 100,000 `Row`s, 390 × 800, neutral;
i9-13900K, Linux, Playwright 1.59 headless Chromium / jsdom 27; median of 3):

| renderer | firstPaintMs | scrollStepMs | scrollToIndexMs | renderedItems |
|---|---|---|---|---|
| Chromium (`bun run bench:list`) | 147 | 4.9 | 38 | 30 |
| jsdom (`XUI_BENCH=1 vitest run src/bench.test.tsx`) | 549 | 14.9 | 70 | 30 |

A step = the scroll, the window's render flushed and the layout read back
(main-thread work, no frame wait); the jump includes the first re-aim.

## Round 2 (contract `docs/round-2-contract.md`)

| item | web |
|---|---|
| Resizable | flex panels (sizes = grow factors over a 0 basis = `panelExtents`), hairline handles with an 8 px hit area, pointer drag from the START sizes, `keyboardResize`, bound `sizes` + `change {sizes}` at drag end / key, `separator` a11y named `$string.resize` |
| templates (breaking) | `<ExponentialSurface>` takes `templates` beside `root` (the reducer's lifted ones; `useSurface` and `HostSurface` pass them). The in-tree fallback and the `templateNodeFrom` export are gone: a `root`-only caller renders no template items, and dev builds warn once per missing template |
| `position: sticky` | CSS sticky |
| `backdropBlur` | `backdrop-filter: blur(var(--xui-blur-*))` (+ `-webkit-`) |
| `animation` | `@keyframes xui-<name>` in the base sheet, duration `calc($motion × factor)`, `animationDuration` keeps the factor, shimmer = an `::after` band on `--xui-band`; reduced motion = 0 ms (the rest frame) |
| `direction` | on any node: its `dir`, and its subtree's natives (Radix `dir`, arrows, chart mirror) read it; `rtlMirroredIcons` flip by `:dir(rtl)` |
| Formatter | `intlFormatter(locale, timeZone)` per surface (`timeZone` prop, default the platform's): the six format functions, Table `number`/`currency`/`percent`/`date`/`relativeTime` cells, NumberField, Slider value, chart ticks and summary, picker triggers, calendar names; a `formatRelativeTime` without `now` or a Table `relativeTime` column re-binds every minute |
| text props | a bound number shows `412`, a boolean `true`, an object nothing (`displayString`) |
| strings | `invalidValue`, `message`, `codeBlock`, `dialog`, `table`, `carousel`, `slide`, `resize` |
| parts | every laid-out part carries `data-xui-id="<id>.<part>[.<i or row key>]"` + `data-xui-part`; inactive carousel pages are `inert` + `data-xui-inactive` |
| §7 sizes | overlays = their trigger's box (`data-xui-overlay-root`), menu trigger without chevron, Chart `height` = the whole box, AudioPlayer track + controls row, Video / Image 16:9, Table and CodeBlock from recipes only (per-side border widths no longer pick up the UA's 3 px), Browse in the drop zone, TreeGuides columns (round 3: 14 px), Switch label first, Checkbox gap, Radio items at the root gap, Accordion count part |

## Round 3 (VAPP-102: Row, Section, Chip, Segmented, Menu)

| item | web |
|---|---|
| macros | `Row`, `Section`, `Chip` need no painter: they expand to Box/Text/Icon/List/TreeGuides/Badge/Button/Image and the two new natives. A `Section` body is a divided `List`; with `tree` it is `role=tree` of `treeitem`s |
| Segmented | ONE row of segments (Radix ToggleGroup: radiogroup single / toolbar multiple, roving arrows), variants `segmented` (default) / `toggles` / `outline`; `bar` = a full-width `<nav>` of column buttons (icon above a caption label, `$control.tabBar` tall), `aria-current="page"`, the same roving keys; parts root, item, icon, label |
| Menu | ONE painter for press and context menus (`openOn`); builtin glyphs `Menu.check`, `Menu.submenuIndicator` |
| TreeGuides | a Row's `guides` part the CORE fills (`elbowAt`, `tee`, `passThrough`); `TREE_GUIDE_COLUMN` 14 px columns, the line at i·14 + 7, the elbow = one element (left + bottom borders) to the column's right edge with a `TREE_GUIDE_RADIUS` 3 px corner, every vertical starting `TREE_GUIDE_BRIDGE` 1 px above the part's top (overflow visible) |

## The primitive set

`src/primitives/` holds the generic shadcn/Radix components the app and
extension authors share (button, input, select, dialog, sheet, tabs, the glass
rows, pill, meter, composer, calendar, … plus round 1's alert-dialog,
breadcrumb, kbd, scroll-area), themed only through the surface's variables;
only what the app imports lives here (the catalog's natives own Radix
directly). `@exp/ui` re-exports every one
(its old paths are one-line shims), and its `styles.css` scans this directory
so the app's Tailwind build still emits their classes. An embedder without
Tailwind takes the compiled `dist/exponential-ui-react.css`
(`bun run --filter @exponential-at/ui-react build:css`).

## Commands

```bash
bun run --filter @exponential-at/ui-react test            # jsdom: fixtures → DOM, bind-time, round 1, inputs, data, theme css, primitives (LANG=en_US.UTF-8)
bun run --filter @exponential-at/ui-react test:browser    # headless Chromium: geometry, overlays, typing, rounds 1–2, conformance
bun run --filter @exponential-at/ui-react bench:list      # the 100,000-row list bench in Chromium
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
