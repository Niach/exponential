import { readFileSync } from "node:fs"
import type { ReactElement } from "react"

// The site's own parts (exponential.at, apps/site) on this page: the SAME
// stylesheet the site loads (apps/site/src/styles/parts.css), scoped under
// `.sg-site`, over the site's dark zinc ladder (theme.css's first `:root`
// block, scoped the same way). The entries site-mega-menu and site-cta-band
// draw the site's markup with it as islands (a shadow root keeps the site's
// classes and styles off this page), so the two can never drift apart.

const SITE_STYLES = new URL(`../../../site/src/styles/`, import.meta.url)
const read = (file: string) => readFileSync(new URL(file, SITE_STYLES), `utf8`)

/** theme.css's `:root { … }` (the site's tokens) as `.sg-site { … }`. */
const tokens = /:root\s*\{[^}]*\}/.exec(read(`theme.css`))![0].replace(`:root`, `.sg-site`)
const parts = read(`parts.css`).replace(/:root(?![\w-])/g, `.sg-site`)

/** The scope's frame on this page: the site's ground, a Starlight-ish button. */
const SCOPE = `
.sg-site { position: relative; overflow: hidden; padding: 16px; border-radius: 12px; background: var(--sl-color-black); color: var(--sl-color-white); font-family: var(--sl-font); --sl-text-sm: 0.875rem; --sl-text-xs: 0.8125rem; }
.sg-site .sg-site-button { display: inline-flex; align-items: center; gap: 0.5em; padding: 0.6rem 1.25rem; border: 1px solid var(--sl-color-white); border-radius: 999px; color: var(--sl-color-white); font-size: 0.95rem; text-decoration: none; }
.sg-site .sg-site-button.primary { background: var(--sl-color-white); color: var(--sl-color-black); }
`

export function siteScope(body: ReactElement, extraCss = ``): ReactElement {
  return (
    <>
      <style dangerouslySetInnerHTML={{ __html: `${tokens}${parts}${SCOPE}${extraCss}` }} />
      {body}
    </>
  )
}
