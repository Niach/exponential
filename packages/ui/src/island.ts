// @exp/ui/island — the SERVER-ONLY render path that puts a REAL component
// into a page that does not run the web app: the styleguide's Components
// group and the marketing docs (EXP-887). It pulls `react-dom/server` and the
// Tailwind compiler, which is why it is a separate export and never part of
// the `@exp/ui` barrel.
//
// An island is the component's RESTING state rendered to static markup inside
// a declarative shadow root, styled by the package stylesheet compiled ONCE
// per host page. The shadow tree is what keeps the web theme (`--border`,
// `--accent`, preflight …) from colliding with the host's own CSS — marketing
// has hand-written tokens of the same names. Tailwind v4 emits `@theme` vars
// to `:root, :host` and preflight to `html, :host`, so both reach the tree,
// and the package `:root` token block is written `:root, :host` for the same
// reason.
//
// Limits (also in README.md): `renderToStaticMarkup` = resting state, so Radix
// portals (open menus, dialogs, popovers, sheets) render nothing;
// `AvatarImage` never renders (fixtures use initials); a `Select` with a value
// shows an empty trigger; islands never carry their own ground — the HOST
// page paints it (`bg-app-gradient` is the body's fixed `::before` pair, and a
// fixed layer inside a shadow tree is wrong).

import { compile, optimize } from "@tailwindcss/node"
import { Scanner } from "@tailwindcss/oxide"
import type { ReactElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

/** The wrapper inside every island's shadow root. `dark` is what makes the
 * `.dark` token block and the `dark:` variant apply; the rest is the web
 * `body`'s own face. No background: the host paints the ground. */
export const ISLAND_ROOT_CLASS = `dark font-sans text-foreground antialiased`

/** The id of the inert `<template>` carrying the compiled CSS. */
export const ISLAND_CSS_TEMPLATE_ID = `ui-css`

/**
 * Compile the package stylesheet for a host page. `base` is the CALLER's
 * directory, so its own island wrappers/fixtures are scanned too; `sources`
 * adds explicit `@source` globs (relative to `base`).
 *
 * This mirrors the Vite plugin's scanner wiring: `compiler.root` is the
 * automatic-detection arm (`"none"` = off, `null` = the base dir, else an
 * explicit `@source` root), `compiler.sources` the explicit `@source` entries
 * — including the package's own `@source "./**\/*.{ts,tsx}"`, resolved to
 * its REAL path (not node_modules), so the components are scanned.
 */
export async function compileUiCss({
  base,
  sources = [],
}: {
  base: string
  sources?: string[]
}): Promise<string> {
  const input = [
    `@import "@exp/ui/styles.css";`,
    ...sources.map((source) => `@source "${source}";`),
  ].join(`\n`)
  const compiler = await compile(input, {
    base,
    shouldRewriteUrls: true,
    onDependency() {},
  })
  const root =
    compiler.root === `none`
      ? []
      : compiler.root === null
        ? [{ base, pattern: `**/*`, negated: false }]
        : [{ ...compiler.root, negated: false }]
  const scanner = new Scanner({ sources: [...root, ...compiler.sources] })
  const css = compiler.build(scanner.scan())
  const { code } = optimize(css, { minify: true })
  return assertStyleSafe(code)
}

/** The CSS rides inside a `<style>` in a `<template>`; a closing tag in it
 *  (any case — RAWTEXT end tags match case-insensitively) would end the
 *  carrier early. */
function assertStyleSafe(css: string): string {
  if (/<\/style/i.test(css)) {
    throw new Error(`ui css contains "</style"`)
  }
  return css
}

/**
 * One island: the element's static markup inside a declarative shadow root.
 * Plain string concatenation because `shadowrootmode` is not in
 * `@types/react`. The host page must carry {@link renderIslandCssTemplate}
 * once and {@link ISLAND_CLIENT_SCRIPT} once.
 */
export function renderIsland(element: ReactElement): string {
  return (
    `<div data-ui-island>` +
    `<template shadowrootmode="open">` +
    `<div class="${ISLAND_ROOT_CLASS}">${renderToStaticMarkup(element)}</div>` +
    `</template>` +
    `</div>`
  )
}

/** The inert carrier for the compiled CSS — goes in `<head>` ONCE. */
export function renderIslandCssTemplate(css: string): string {
  return `<template id="${ISLAND_CSS_TEMPLATE_ID}"><style>${assertStyleSafe(css)}</style></template>`
}

/**
 * Builds ONE `CSSStyleSheet` from the carrier and adopts it into every
 * island's shadow root; polyfills `attachShadow` from the `<template>` for
 * browsers without declarative shadow DOM (guarded by `if (host.shadowRoot)`,
 * so it is a no-op wherever DSD already ran). Where constructible stylesheets
 * are missing too (WebKit < 16.4) each root gets its own `<style>` clone
 * instead, so the structural polyfill never dies with the styling one.
 * Inline it in a `<script>` at the end of `<body>`.
 */
export const ISLAND_CLIENT_SCRIPT = `(() => {
  const carrier = document.getElementById(${JSON.stringify(ISLAND_CSS_TEMPLATE_ID)});
  const style = carrier && carrier.content.querySelector("style");
  const css = style ? style.textContent : "";
  let sheet = null;
  try {
    sheet = new CSSStyleSheet();
    sheet.replaceSync(css);
  } catch {
    sheet = null;
  }
  for (const host of document.querySelectorAll("[data-ui-island]")) {
    if (!host.shadowRoot) {
      const template = host.querySelector(":scope > template");
      if (!template) continue;
      host.attachShadow({ mode: "open" }).appendChild(template.content.cloneNode(true));
      template.remove();
    }
    const root = host.shadowRoot;
    if (sheet && "adoptedStyleSheets" in root) {
      root.adoptedStyleSheets = [sheet];
    } else if (!root.querySelector("style[data-ui-css]")) {
      const clone = document.createElement("style");
      clone.setAttribute("data-ui-css", "");
      clone.textContent = css;
      root.prepend(clone);
    }
  }
})();`
