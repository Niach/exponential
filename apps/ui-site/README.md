# ui.exponential.at

The public site of **Exponential UI**, the open (Apache-2.0) generative-UI
SDK (VAPP-84): an agent sends an A2UI surface, native renderers paint it in
React, SwiftUI, Jetpack Compose and gpui. VAPP-93. Same stack and look as
exponential.at (`apps/marketing`): Vite + React, prerendered to static HTML,
indexed, the shared chrome and styles of `@exp/site-shell`. Wherever it shows
components it renders them with the SDK itself (`@exponential-at/ui-react`).

## Commands

```bash
bun run dev:ui-site        # vite dev server (runs `gen` first)
bun run build:ui-site      # gen → tsc → vite build → prerender of every route
bun --filter @exp/ui-site typecheck
bun --filter @exp/ui-site test
bun --filter @exp/ui-site screenshots         # Playwright screenshots of dist/ → /tmp/ui-site-shots (build first)
bun --filter @exp/ui-site check:guides        # every guide example from a fresh project
cd apps/ui-site && bun run scripts/generate-og.tsx   # rewrites public/og/og-ui.png (by hand)
```

`gen` writes two gitignored files under `src/generated/`: the guide sources
(`scripts/guide-sources.ts`) and the shot index (`scripts/shot-index.ts`:
which `shots/exponential-ui-*/<platform>.webp` exist, so a missing shot
prerenders as "Shot pending", never a broken image).

## Pages

`src/lib/routes.ts` is THE page manifest: every path, its page module and its
SEO (title, description, the OG image `public/og/og-ui.png`, JSON-LD). The
prerender (`scripts/prerender.tsx`, on `@exp/site-shell/prerender`) writes one
`dist/<path>/index.html` per entry plus `sitemap.xml` and `llms.txt`; the
client entry loads each page type as its own chunk through `src/lib/pages.ts`
(`routes.test.ts` keeps the two in lockstep).

| path | page | what |
|---|---|---|
| `/` | `Home.tsx` | the live demo (`src/sdk/HomeDemo.tsx`), the four renderers, entry points, packages |
| `/concepts/` | `Concepts.tsx` | A2UI in short, catalog, A2UI basic mapped, themes, extensions, host plugins, vapps, how renderers agree |
| `/components/` | `ComponentsIndex.tsx` | every component as a live mini render, grouped; search (word starts, `/` focuses), group filter (`?q=`, `?group=`), shot dots |
| `/components/<slug>/` | `Component.tsx` | studio (theme, mode, LTR/RTL, width 390/768/1280; tabs Preview, A2UI JSON, React, SwiftUI, Compose, gpui), four shots at equal height, variants gallery, props, events + slots, keyboard (`a11y.json`), recipe parts, app parity, related |
| `/themes/`, `/themes/builder/` | `Themes.tsx`, `ThemeBuilder.tsx` | the built-in themes; the theme builder |
| `/guides/`, `/guides/<slug>/` | `Guides.tsx`, `Guide.tsx` | the guides hub; every guide by slug (`src/lib/guides.ts`) |
| `/playground/` | `Playground.tsx` | paste A2UI, render it in any theme, read the catalog prompt (prerendered) |
| `/conformance/` | `Conformance.tsx` | the suites (from the manifest), reports, the real-font harness |

The component pages are generated: a catalog component appears with its page,
specimen, variants and embed code without a change here.
`src/lib/docs-coverage.test.ts` fails when a catalog component lacks a page or
a specimen, when its one-line summary passes 160 chars (`componentSummary`
cuts long catalog sentences at a clause) or when lorem ipsum shows up.

Pages must prerender under Bun: no `window`/`document` at render time, no
`?raw` imports, no CSS imports in pages, locale-free number formatting
(`toLocaleString("en-US")`). Live renders mount after hydration
(`src/sdk/Island.tsx`); the small ones (index cards, variants) mount near the
viewport, one per idle slice (`useNearViewport`, `MiniSurface.tsx`).

Styles: `src/styles.css` (the shared site-shell sheets), `content.css` (home,
concepts, guides, conformance), `sdk.css` (the SDK widgets), `scheme.css`
(the light scheme: token overrides under `html[data-scheme=light]`). The
scheme is set before the first paint by the inline script in `index.html`
(stored choice, else the OS); `public/serve.json`'s CSP allows that exact
script by hash (`scheme.test.ts`), so editing it means updating the hash.

## Guides

`guides/` holds the guides' example projects (React, the agent server, a
theme, Swift, Compose, gpui). The guide pages show those files verbatim:
`scripts/guide-sources.ts` bundles them into `src/generated/guide-sources.ts`
(gitignored; runs before dev, build and typecheck) and pages read them with
`guideSource("<path under guides/>")`. `guides/check.ts` builds each one from a
fresh temp project the way a reader would, in CI:

```bash
bun apps/ui-site/guides/check.ts [react|agent|theme|swift|compose|gpui|all] [--keep]
```

## Deploy

Coolify app `exponential-ui-site` (home-LAN only, like every deploy): it
builds `cd apps/ui-site && bun run build` and serves `apps/ui-site/dist` with
`npx -y serve` on port 80. `public/serve.json` sets the headers, including the
CSP: an origin the browser has to fetch must be added there in the same change
(`src/lib/site.ts` lists every origin the site links).
