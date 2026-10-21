# `@exponential-at/ui-solid`: the Exponential UI SolidJS renderer

The SolidJS renderer of Exponential UI (FEED-128): an A2UI surface in the
core catalog (`@exponential-at/ui`) painted with the **identical DOM** of the
React renderer (`@exponential-at/ui-react`), with the same compiled stylesheet
and runtime themes. Overlays, menus, selects, tabs, accordions and the form
controls are painted on Kobalte. It depends on `@exponential-at/ui` (the shared
web layer `@exponential-at/ui/web`), `solid-js`, `@kobalte/core` and
`lucide-solid`. It imports nothing from the React renderer and nothing from the
Exponential app.

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
import { ExponentialSurface, createSurface } from "@exponential-at/ui-solid"

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

## Layout

| path | what |
|---|---|
| `src/surface.tsx` | `<ExponentialSurface>`: theme resolution, density/contrast, the measured box + breakpoints, the two `<style>`s, the container, layers, live regions, the handle |
| `src/create-surface.ts` | `createSurface`: the reducer as a Solid store (tree and data in two signals) |
| `src/node-view.tsx` | the bind pass as per-node memos, painter lookup through `<Dynamic>`, `NodeList` (children keyed by id) and `TemplateItems` (keyed by `template.key`), `runNodeAction`, the paint boundary |
| `src/context.ts` | the surface context (an object of getters; `deriveContext` re-points fields), scope + instance accessors |
| `src/natives/` | `shared.tsx` (`parts()`, the porting rules), `bound.ts`, `text.tsx`, `layout.tsx`, `pending.tsx` (temporary placeholders), `index.ts` (the 48 keys) |
| `src/style.ts` | `css()`: React's inline-style semantics (camelCase, px for numbers, unitless list) |
| `src/icons.tsx` | the fallback glyphs on lucide-solid, painted with lucide-react's exact `<svg>` attributes |
| `src/form.tsx`, `list.tsx`, `markdown.tsx`, `media.ts`, `inputs.ts`, `platform.ts`, `extensions.ts`, `host*.ts*` | the React files' Solid twins |
| `harness/` | the same page protocol as React's harness (`dev:harness` serves on :4182, built with Vite + `vite-plugin-solid`) |

## Kobalte vs Radix

The theme CSS (`STATE_SELECTORS`) targets Radix's attribute names. The
Solid painters therefore set the Radix-named attributes on the parts
themselves, and keep a Kobalte attribute only where React's Radix DOM has the
same one. The full rules are in `src/natives/shared.tsx`.

| state | Radix (React, the theme CSS) | Kobalte | the Solid painter writes |
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
bun run --filter @exponential-at/ui-solid typecheck
bun run --filter @exponential-at/ui-solid test          # jsdom: surface, natives × catalog, DOM parity with React
bun run --filter @exponential-at/ui-solid dev:harness   # http://localhost:4182/?view=kitchen-sink&theme=orange
```
