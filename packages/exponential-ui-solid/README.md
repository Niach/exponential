# `@exponential_app/ui-solid`: the Exponential UI SolidJS renderer

The SolidJS renderer of Exponential UI (FEED-128): an A2UI surface in the
core catalog (`@exponential_app/ui`) painted with the **identical DOM** of the
React renderer (`@exponential_app/ui-react`), with the same compiled stylesheet
and runtime themes. Overlays, menus, selects, tabs, accordions and the form
controls paint Radix's DOM on plain elements, on a headless layer
(`src/behaviors/`) that reproduces Radix's machinery: positioning on
`@floating-ui/dom`, modality on `aria-hidden`. It depends on
`@exponential_app/ui` (the shared web layer `@exponential_app/ui/web`),
`solid-js`, `@floating-ui/dom`, `aria-hidden` and `lucide-solid`. It imports
nothing from the React renderer and nothing from the Exponential app.

## The invariant

For every surface the Solid painter produces exactly what React paints:

- the same elements and attribute set, with the same values: the `class`
  strings in the same order, `data-xui-id`, `data-xui-part`, `data-xui-c`,
  `data-r-*`/`data-m-*`, `data-xs`, aria-*;
- the same `<style data-xui-style="theme">` (`BASE_CSS` + the compiled
  theme) and `<style data-xui-style="nodes">`;
- the same root (`.xui-surface <scope> xui-s-<id>`, `data-xui-mode`, `dir`,
  `data-xq`) and the same overlay, toast and live-region layers.

`src/test/parity.ts` (`expectDomParity`) renders one tree in both renderers
in jsdom and compares the serialised DOM (attributes sorted, generated ids
renamed). Every native port adds its cases to `src/parity.test.ts`. A
browser-level web-parity suite also compares computed styles, frames and
pixels at zero tolerance.

```tsx
import { ExponentialSurface, createSurface } from "@exponential_app/ui-solid"

function Chat(props: { messages: A2uiMessage[] }) {
  const surface = createSurface({ surfaceId: "main" })
  createEffect(() => props.messages.forEach(surface.apply))   // A2UI v0.9 server messages
  return (
    <ExponentialSurface
      surface={surface}
      theme="exponential"                     // or "neutral" | "playful" | "orange" | a theme JSON | a ResolvedTheme
      mode="system"                           // light | dark | system
      locale="de-AT" timeZone="Europe/Vienna"
      host={{ icons, onAction, onInput }}     // the host plugin (lucide-solid icons)
      handleRef={(h) => (handle = h)}         // handle.run({ focus: { id } } | { announce } | { scrollIntoView } | { scrollToIndex })
    />
  )
}
```

The props are the same as React's `ExponentialSurfaceProps`, with these
differences:

- `class`/`style` take Solid's form (kebab-case style keys).
- `handleRef` is a callback that receives the `SurfaceHandle`.
- The React hooks have Solid names: `useSurface` is `createSurface`,
  `useHostOwnedValue` is `createHostOwnedValue`, `useParts` is `parts`,
  `useField` is `createField`, `useMediaSource` is `createMediaSource`,
  `useHostSurface` is `createHostSurface`, and `defineReactExtension` is
  `defineSolidExtension`.

## The QrScanner camera (`qrScanner` host option)

The camera is the host's (VAPP-162, `docs/round-5-contract.md` §7), the same
hook as React's:

```ts
host={{
  qrScanner: {
    // Render the live feed into `feed` (an empty element filling the
    // viewfinder; append a <video>), decode codes of `formats` (default
    // ["qr"]) and call onValue per decoded string. Return the stop.
    start(feed: HTMLElement, formats: readonly string[], onValue: (value: string) => void): (() => void) | false | void
  },
}}
```

`onValue` dispatches `scanned {value}` once per distinct value (a code held
in view reports once; a remount or re-enable reports it again) and announces
it politely. The stop runs on unmount, on `disabled` and when `formats`
change. No `qrScanner`, `start` returning `false` or throwing = the
placeholder (`ui-scan` + `Camera unavailable`), no scan line.

## Layout

| path | what |
|---|---|
| `src/surface.tsx` | `<ExponentialSurface>`: theme resolution, density/contrast, the measured box + breakpoints, the two `<style>`s, the container, layers, live regions, the handle |
| `src/create-surface.ts` | `createSurface`: the reducer as a Solid store (tree and data in two signals) |
| `src/node-view.tsx` | the bind pass as per-node memos, painter lookup through `<Dynamic>`, `NodeList` (children keyed by id) and `TemplateItems` (keyed by `template.key`), `runNodeAction`, the paint boundary |
| `src/context.ts` | the surface context (an object of getters; `deriveContext` re-points fields), scope + instance accessors |
| `src/natives/` | `shared.tsx` (`parts()`, the porting rules), `bound.ts`, the painters (`overlays.tsx`, `select.tsx`, `dates.tsx`, `nav.tsx`, `inputs.tsx`, `qr.tsx`…), `index.ts` (the 51 keys) |
| `src/behaviors/` | the headless Radix machinery the natives and `src/primitives/` share, one concern per module: `portal`, `presence`, `dismissable-layer`, `focus-scope`, `focus-guards`, `scroll-lock`, `modal`, `popper`, `menu`, `roving-focus`, `listbox`, `typeahead`, `collapsible`, `picker-popover`, `bubble-input`, `slider`, `grace-area`, `visually-hidden`, `drag-to-dismiss`, `style-writer`, `element-size`, `image-loading-status`, `search-match` (`index.ts` names the Radix primitive each reproduces). Nothing in it reads the surface context; package-internal |
| `src/primitives/` | the React primitive set on Solid (FEED-131), one module per React module but the six app-specific ones, on the shared recipes of `@exponential_app/ui/web/primitives` and on `src/behaviors/`; `index.ts` = `./primitives`; `bun run build:css` → `dist/exponential-ui-solid.css` |
| `src/style.ts` | `css()`: React's inline-style semantics (camelCase, px for numbers, unitless list) |
| `src/icons.tsx` | the fallback glyphs on lucide-solid, painted with lucide-react's exact `<svg>` attributes |
| `src/form.tsx`, `list.tsx`, `markdown.tsx`, `media.ts`, `inputs.ts`, `platform.ts`, `extensions.ts`, `host*.ts*` | the React files' Solid twins |
| `harness/` | the same page protocol as React's harness (`dev:harness` serves on :4182, built with Vite + `vite-plugin-solid`) |

## Primitives

`@exponential_app/ui-solid/primitives` is the Solid twin of
`@exponential_app/ui-react/primitives` (FEED-131): button, dialog, select,
tabs, table, command, toast, calendar and the rest, with the same DOM and
classes. The class strings come from ONE source, the recipes of
`@exponential_app/ui/web/primitives`; neither set holds a class literal.

```tsx
import "@exponential_app/ui-solid/styles.css"          // the compiled primitives sheet
import { globalsCss } from "@exponential_app/ui/web"
import { Button, Dialog, DialogContent, DialogTitle, DialogTrigger } from "@exponential_app/ui-solid/primitives"
```

- `styles.css` is `dist/exponential-ui-solid.css`, compiled by
  `bun run build:css` from the React package's `tailwind.css`: byte-identical
  to `@exponential_app/ui-react/styles.css` (`src/primitives/styles.test.ts`).
- Inside a surface the primitives wear its theme. Outside one,
  `globalsCss(theme)` writes the theme as shadcn variables (`:root` + `.dark`).
- The names follow Solid: `class` for `className`, `as` for `asChild` (the
  element or component the part paints as), `create*` for the `use*` hooks.
- React's `SegmentedControl` has no `embedded` arm either: the glass-group row
  is the Exponential app's (`@exp/ui`), like its other own components.
- The web-parity gate compares every specimen of
  `@exponential_app/ui/fixtures/primitive-specimens.json` with React's over the
  whole page body: `bun run --filter @exponential_app/ui-solid test:parity:primitives`.

## State attributes: the porting rule

The theme CSS (`STATE_SELECTORS`) targets Radix's attribute names, so the
Solid painters write the Radix-named attributes on the parts themselves. The
package was first painted on Kobalte, which names the same states
differently; the dependency is gone, and the Kobalte column stays as the map
for porting code written against Kobalte (or any non-Radix library). The full
rules are in `src/natives/shared.tsx`.

| state | Radix (React, the theme CSS) | Kobalte (history) | the Solid painter writes |
|---|---|---|---|
| checked / unchecked | `data-state="checked\|unchecked"` | `data-checked` | `data-state` |
| open / closed | `data-state="open\|closed"` | `data-expanded`, `data-closed` | `data-state` |
| selected tab | `data-state="active\|inactive"` | `data-selected` | `data-state` |
| pressed toggle | `data-state="on\|off"` | `data-pressed` | `data-state` |
| placement | `data-side`, `data-align` | (none, style only) | `data-side`, `data-align` |
| highlighted item | `data-highlighted=""` | `data-highlighted` | `data-highlighted=""` |
| disabled | `data-disabled=""` | `data-disabled` | `data-disabled=""` |

## Commands

```bash
bun run --filter @exponential_app/ui-solid typecheck
bun run --filter @exponential_app/ui-solid test          # jsdom: surface, natives × catalog, DOM parity with React
bun run --filter @exponential_app/ui-solid dev:harness   # http://localhost:4182/?view=kitchen-sink&theme=orange
bun run --filter @exponential_app/ui-solid test:conformance  # the shared web runner → .conformance/exponential-ui-solid.json
bun run --filter @exponential_app/ui-solid test:parity       # web parity vs React → .conformance/exponential-ui-parity-react-solid.json
bun run --filter @exponential_app/ui-solid test:parity:primitives  # the primitive specimens only (seconds)
bun run --filter @exponential_app/ui-solid test:browser      # every headless-Chromium suite (overlays, inputs, tables…)
bun run --filter @exponential_app/ui-solid build:css         # dist/exponential-ui-solid.css (after ui-react's build:css)
bun run --filter @exponential_app/ui-solid shots             # the kitchen sink → /tmp/feed128/solid-shots/ (--theme, --mode, --out)
```
