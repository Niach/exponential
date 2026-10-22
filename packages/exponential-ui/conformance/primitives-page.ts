// FEED-131: the framework-neutral half of both harnesses' `?view=primitives`
// page (packages/exponential-ui-{react,solid}/harness/primitives/mount.tsx):
//   ?view=primitives&id=<specimen>&theme=<id>&mode=light|dark&width=<px>
// The <head> gets the unscoped shadcn variable block (`globalsCss`, `:root`
// + `.dark`), the package's OWN compiled stylesheet (served by its
// serve.ts at PRIMITIVES_SHEET_URL) and the page rules; `<html>` wears
// `class="dark"` in dark mode; `<body>` wears `data-xui-parity-root` (the
// web-parity probe then compares the WHOLE body) + `data-xui-mode`. The
// caller renders `<div data-xui-specimen="<id>" style="padding:16px">` into
// the returned container. Body inline style stays EMPTY (Radix writes it).

import { builtinTheme } from "../src/themes"
import { globalsCss } from "../src/web/theme-css"

/** Where each harness's serve.ts answers with its package's dist stylesheet. */
export const PRIMITIVES_SHEET_URL = `/dist/primitives.css`

/** The page's own rules (no preflight in the sheet): body chrome only. */
export const PRIMITIVES_PAGE_CSS = `body{margin:0;background:var(--background);color:var(--foreground);font-family:var(--font-sans)}`

export interface PrimitivesPage {
  id: string
  mode: `light` | `dark`
  container: HTMLElement
}

const style = (name: string, css: string) => {
  const s = document.createElement(`style`)
  s.setAttribute(`data-xui-primitives`, name)
  s.textContent = css
  document.head.appendChild(s)
}

/** Head styles + root attributes, then the stylesheet fetched and applied:
 *  resolves once the page is ready to render the specimen. */
export async function preparePrimitivesPage(params = new URLSearchParams(location.search)): Promise<PrimitivesPage> {
  const id = params.get(`id`) ?? ``
  const mode = params.get(`mode`) === `dark` ? `dark` : `light`
  const theme = builtinTheme(params.get(`theme`) ?? `exponential`)
  if (mode === `dark`) document.documentElement.classList.add(`dark`)
  document.body.setAttribute(`data-xui-parity-root`, ``)
  document.body.setAttribute(`data-xui-mode`, mode)
  style(`globals`, globalsCss(theme))
  const r = await fetch(PRIMITIVES_SHEET_URL)
  if (!r.ok) throw new Error(`primitives: ${PRIMITIVES_SHEET_URL} answered ${r.status}`)
  style(`sheet`, await r.text())
  style(`page`, PRIMITIVES_PAGE_CSS)
  return { id, mode, container: document.getElementById(`app`)! }
}
