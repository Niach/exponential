# `@exponential-at/ui-element`: Exponential UI as a custom element

`<exponential-surface>` paints an A2UI surface in the core catalog
(`@exponential-at/ui`) in any page, with no framework (FEED-130). It does not
have a painter of its own. It mounts the Solid renderer
(`@exponential-at/ui-solid`) inside itself, so you get one painter with two
entry points. The element uses light DOM and is `display: contents` through
one shared zero-specificity rule `define()` installs (`:where(exponential-surface){display:contents}`,
so any rule of yours wins), so the surface's `.xui-surface` root is THE box,
in the same place React's div would be. Elements attached in one task mount
together: the first at once, the rest in a microtask inside one batch.

## The invariant

For every surface, the element paints exactly what the React renderer
paints: the same elements, attributes, `<style data-xui-style>` sheets,
layers and runtime themes. `src/parity.test.ts` compares the element's
`.xui-surface` subtree against `@exponential-at/ui-react` in jsdom, using
the Solid package's `serializeDom` and `renderReact`.

```html
<script type="module">
  import "@exponential-at/ui-element/define"   // or the self-contained ./standalone bundle
</script>

<exponential-surface theme="orange" mode="system" src="/ui/stream" post="/ui/events"></exponential-surface>

<script type="module">
  const el = document.querySelector("exponential-surface")
  el.addEventListener("xui-action", (e) => e.detail.waitUntil(fetch("/save", { method: "POST", body: JSON.stringify(e.detail.context) })))
</script>
```

Without a server, call `apply()` to feed it A2UI v0.9 messages:

```js
const el = document.createElement("exponential-surface")
document.body.append(el)
el.apply({ createSurface: { surfaceId: "main", catalogId } })
el.apply({ updateComponents: { surfaceId: "main", components } })
```

## API

| attribute / property | what |
|---|---|
| `theme` | A built-in id (attribute). The property also takes a theme JSON or a `ResolvedTheme`; an object is not reflected. An unusable theme never throws: the default theme paints and `xui-theme-issues` fires. |
| `mode` | `light`, `dark` or `system` (the default, followed live) |
| `density`, `contrast` | `compact`, `default` or `comfortable`; `normal`, `high` or `system` |
| `locale`, `time-zone` (`timeZone`) | BCP 47 locale (default `en-US`) and IANA time zone (default the platform's) |
| `direction` | `ltr` or `rtl` (default: the locale's direction) |
| `width`, `height` | A numeric string means px; any other string is a CSS length. An explicit `height` is the viewport for height conditions. |
| `surface-id` (`surfaceId`) | The surface id (default `main`): the `xui-s-<id>` class, the store `apply()` makes, and the host surface it paints |
| `src`, `post`, `transport` | The host API (see below) |
| `surface` | A `createSurface` store (it takes priority over the other sources) |
| `root`, `templates`, `data` | Fixture mode: a normalized tree, its lifted templates, and a data model |
| `host` | An `ExponentialHost` to use instead of `src` |
| `plugin` | A `HostPlugin`: icons, `Markdown`, URL and media policies, `optionSource`, `mediaRequest`, `mediaStreamUrl`, `resolveUrl`, `functions`, and the callbacks |
| `extensions`, `strings`, `states`, `measure`, `viewportHeight` | The same as the Solid props |

String properties reflect to their attributes. Setting a property to
`null` removes its attribute. Properties set before the element is attached
(or before it is upgraded) still apply.

Read-only properties:

- `handle`: the `SurfaceHandle`, or `null` until the first paint.
- `issues`: the active store's reduce issues.
- `status`: the transport status; `statusDetail`: its detail (an error, or why the URL policy refused `src`/`post`).
- `surfaceState`: the `SurfaceState` being painted.

Methods:

- `apply(message)` sends the message to the `surface` property, else to the host (`receive`), else to a store created on first use.
- `setData(pointer, value)` writes the active store. In fixture mode it sets a new `data`.
- `reset()` resets the active store.
- `run(command)`, `focus(id)`, `announce(text, live?)`, `scrollIntoView(id)` and `scrollToIndex(id, index, align?)` go through the handle. Before the first paint they return `false` or do nothing. Called without a string id, `focus()` and `scrollIntoView()` keep their usual `HTMLElement` meaning.

Detaching the element disposes the painted DOM. Re-attaching it paints
again, and every property survives. Attaching also clears any children the
author put inside the element.

## Events

Every event is a `CustomEvent` that bubbles and is composed. When there is
a matching `plugin` callback, both the callback and the event fire. Where a
promise is involved, the results are joined: the control stays pending until
all of them settle.

| event | detail |
|---|---|
| `xui-action` | `SurfaceActionEvent` + `waitUntil(promise)`. A Button stays pending (`aria-busy="true"` and `disabled` at once; `data-pending="true"` after `PENDING_DELAY_MS`) until the promise settles. |
| `xui-input` | `SurfaceInputEvent` + `waitUntil`. The revision is acknowledged when the promise settles. |
| `xui-function-call` | `{ call, respond(value) }`. The first `respond` wins. With no response, the plugin's `onFunctionCall` result is used. |
| `xui-open-url` | `{ url }`, after the URL policy passed it. Cancelable: if not cancelled, it goes to `plugin.openUrl`, else `window.open(url, "_blank", "noopener,noreferrer")`. |
| `xui-upload` | `{ files, target: { nodeId, name }, waitUntil }` |
| `xui-paint-error` | `{ error }` |
| `xui-theme-issues` | `{ issues }` |
| `xui-unknown` | `{ node }` |
| `xui-status` | `{ status, detail? }` |
| `xui-ready` | `{ handle }`. Fires once per mount, in a microtask after the handle arrives, so listeners added right after `appendChild` still get it. |

`HTMLElementEventMap` and `HTMLElementTagNameMap` are augmented, so
`addEventListener("xui-action", …)` and `document.createElement("exponential-surface")`
are typed.

## The host API

When `src` is set and no `host` property is given, the element builds
`new ExponentialHost({ transport })` with one of these transports:

- `WebSocketTransport` when `src` is a `ws:`/`wss:` URL or `transport="ws"`.
- `SseTransport` when `transport="sse"`.
- `JsonlStreamTransport` otherwise.

Client messages are POSTed to `post` (default `src`). The element connects
the host it built when attached, closes it when detached, and rebuilds it
when `src`, `post` or `transport` change. A host surface paints nothing
until the host has a surface with `surface-id`. Actions, functions, URLs and
media route through the host (`hostPlugin`). `xui-open-url` still fires
before the host opens a URL.

A `host` property belongs to the page. The element connects it only if it
has a transport and has never connected (`status === "closed"`), and it
never closes it.

`src` and `post` pass the page's URL policy (`plugin.urls`, the one every
href passes) BEFORE a transport is built (`decideTransport`): a private,
loopback or link-local host (`10.0.0.5`, `localhost`, `*.local`,
`169.254.169.254`, `::1`, decimal and IPv4-mapped literals) is denied unless
`plugin.urls.allowPrivate` opts it in (the page's own private origin is, so
a dev server on localhost streams from itself); a `hosts` allowlist holds;
`schemes: ["https"]` means https + wss only; `javascript:`, `file:`, `data:`
never connect. A `ws:`/`wss:` url is decided as its http(s) twin. A denied
`src` or `post` builds no host and fetches nothing: `status` is `closed`,
`statusDetail` says why (`src denied by the URL policy (private)`) and
`xui-status` fires once. The standalone bundle carries the same filter.

## Extensions

```js
import { defineElementExtension } from "@exponential-at/ui-element"

el.extensions = [defineElementExtension({
  catalog,                                    // defineExtension(...) or its JSON
  components: {
    Gauge: (ctx) => {
      const div = document.createElement("div")
      ctx.applyRootProps(div)                   // class, data-xui-id, data-xui-c, data-r-*, aria, dir, style
      div.textContent = `${ctx.props.value}`
      div.append(...ctx.children())             // ctx.slot("header") for a slot
      div.onclick = () => ctx.emit("press")
      return div
    },
  },
})]
```

A painter is a plain function: `ElementPaintContext` goes in, and one DOM
element comes out. It runs again when the node, props, theme, mode or root
attributes change, and the new element replaces the old one. The painted
children and slots are the same nodes on every re-paint. `registerExtension(ext)`
registers one for every surface on the page. A `components` key that names a
core native overrides its painter. A host built from `src` learns the
catalogs of `el.extensions` when it is built: set the extensions before the
element attaches (or before `src`).

## Icons for framework-less hosts

```js
el.plugin = { icons: iconsFromSvg((name) => sprite[name]) }   // SVG markup strings
```

Each markup string becomes an icon component, created once per distinct
string. A name with no markup falls back to the renderer's own Lucide
glyphs. The wrapper sets on the `<svg>` root what lucide-react's root
carries for the props it receives:

- `width`/`height` = `width` ?? `size` ?? 24.
- `class` = the markup's own classes + the painter's.
- `aria-hidden="true"`, unless an aria-*/role/title prop is passed.
- `stroke` = `color`, `stroke-width` = `strokeWidth`.
- Every other string or number attribute as given (`data-icon`…).

The markup is trusted, but `<script>`, `foreignObject`, `on*` attributes
and `javascript:` links are stripped anyway.

## Entries

| entry | what |
|---|---|
| `@exponential-at/ui-element` | No side effects: the class, `TAG`, `define(tag?)`, the helpers, and re-exports of `createSurface`, `registerExtension` and `defineSolidExtension` |
| `@exponential-at/ui-element/define` | Calls `define()` on import (idempotent; a tag taken by another class throws; another tag gets its own subclass) |
| `@exponential-at/ui-element/standalone` | The same, bundled self-contained (Solid and the renderer inlined) into one ES module |

## Layout

| path | what |
|---|---|
| `src/element.ts` | `ExponentialSurfaceElement`: the signals, attributes and properties, the source order, the merged plugin, mounting |
| `src/events.ts` | Event names, detail types, the global augmentations, the `waitUntil` join |
| `src/transport.ts` | `src`/`post`/`transport` → an `ExponentialHost` |
| `src/extensions.ts` | `defineElementExtension`, `registerExtension`, `applyRootProps` |
| `src/icons.ts` | `iconsFromSvg`, `svgIcon` |
| `src/index.ts`, `define.ts`, `standalone.ts` | The entries |
| `src/*.test.ts` | jsdom suites: element, events, transport, extensions, DOM parity with React |

## Commands

```bash
bun run --filter @exponential-at/ui-element typecheck
bun run --filter @exponential-at/ui-element test
```
