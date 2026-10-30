# exponential.at/ui

The public site of **Exponential UI**, the open (Apache-2.0) generative-UI
SDK (VAPP-84): an agent sends an A2UI surface, native renderers paint it in
React, Solid, web components, SwiftUI, Jetpack Compose and gpui. VAPP-93. A
section of exponential.at (`apps/marketing`, EXP-1392; `ui.exponential.at`
answers 301 to the same path here): Vite + React, prerendered to static HTML,
indexed, ONE header with the rest of the site (the main menu's UI SDK link
steps into this section's level, the back button steps out). Wherever it shows
components it renders them with the SDK itself (`@exponential_app/eui-react`).

## Commands

```bash
bun run dev:marketing                          # vite dev server; /ui/ routes to ui/index.html
bun run build:marketing                        # gen:ui → tsc → vite build → prerender of every route
bun --filter @exp/marketing typecheck
bun --filter @exp/marketing test               # this section's tests (bun test ./ui/src)
bun --filter @exp/marketing screenshots:ui     # Playwright screenshots of dist/ → /tmp/ui-shots (build first)
bun --filter @exp/marketing check:guides       # every guide example from a fresh project
cd apps/marketing && bun run ui/scripts/generate-og.tsx   # rewrites public/og/og-ui.png (by hand)
```

`gen:ui` writes two gitignored files under `ui/src/generated/`: the guide sources
(`scripts/guide-sources.ts`) and the shot index (`scripts/shot-index.ts`:
which `shots/exponential-ui-*/<platform>.webp` exist, so a missing shot
prerenders as "Shot pending", never a broken image).

## Pages

`src/lib/routes.ts` is THE page manifest: every path, its page module and its
SEO (title, description, the OG image `public/og/og-ui.png`, JSON-LD). The
site's prerender (`apps/marketing/scripts/prerender.tsx`) copies the built
`dist/ui/index.html` to one `dist/ui/<path>/index.html` per entry (with the
route's `<title>`) and renders them with the rest of the site into ONE
`sitemap.xml` and `llms.txt`; the
client entry loads each page type as its own chunk through `src/lib/pages.ts`
(`routes.test.ts` keeps the two in lockstep).

| path | page | what |
|---|---|---|
| `/ui/` | `Home.tsx` | the live demo (`src/sdk/HomeDemo.tsx`), the six renderers, entry points, packages |
| `/ui/concepts/` | `Concepts.tsx` | A2UI in short, catalog, A2UI basic mapped, themes, extensions, host plugins, apps, how renderers agree |
| `/ui/components/` | `ComponentsIndex.tsx` | every component as a live mini render, grouped; search (word starts, `/` focuses), group filter (`?q=`, `?group=`), shot dots |
| `/ui/components/<slug>/` | `Component.tsx` | studio (theme, mode, LTR/RTL, width 390/768/1280; tabs Preview, A2UI JSON, React, Solid, Web component, SwiftUI, Compose, gpui), four shots at equal height, variants gallery, props, events + slots, keyboard (`a11y.json`), recipe parts, app parity, related |
| `/ui/themes/`, `/ui/themes/builder/` | `Themes.tsx`, `ThemeBuilder.tsx` | the built-in themes; the theme builder |
| `/ui/guides/`, `/ui/guides/<slug>/` | `Guides.tsx`, `Guide.tsx` | the guides hub; every guide by slug (`src/lib/guides.ts`) |
| `/ui/playground/` | `Playground.tsx` | paste A2UI, render it in any theme, read the catalog prompt (prerendered) |
| `/ui/conformance/` | `Conformance.tsx` | the suites (from the manifest), reports, the real-font harness |
| `/ui/numbers/` | `Numbers.tsx` | the benchmark suite (VAPP-105): what a pass is per renderer, the scenarios (from `@exponential_app/eui-bench`'s manifest), the numbers per machine, weak points, the ratchet |

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
scheme is set before the first paint by the inline script in `ui/index.html`
(stored choice, else the OS); `apps/marketing/public/serve.json`'s CSP allows
that exact script by hash (`scheme.test.ts`), so editing it means updating
the hash. The light/dark switch sits in the header on these pages only.

## Guides

`guides/` holds the guides' example projects (React, Solid, the web
component, the agent server, a theme, Swift, Compose, gpui). The guide pages show those files verbatim:
`scripts/guide-sources.ts` bundles them into `src/generated/guide-sources.ts`
(gitignored; runs before dev, build and typecheck) and pages read them with
`guideSource("<path under guides/>")`. `guides/check.ts` builds each one from a
fresh temp project the way a reader would, in CI:

```bash
bun apps/marketing/ui/guides/check.ts [react|solid|element|agent|theme|swift|compose|gpui|all] [--keep]
```

## Deploy

Ships with exponential.at (`apps/marketing`, its Coolify app). The Coolify app
`exponential-ui-site` keeps the old host alive as a redirect only: it runs
`bun apps/marketing/ui/scripts/redirect.ts` (301 to the same path and query
under `https://exponential.at/ui/`). Catalog and schema ids
(`https://ui.exponential.at/catalogs/...`) are identifiers and stay as they are.
