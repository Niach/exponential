# ui.exponential.at

The public site of **Exponential UI**, the open (Apache-2.0) generative-UI
SDK (VAPP-84): an agent sends an A2UI surface, native renderers paint it in
React, SwiftUI, Jetpack Compose and gpui. VAPP-93. Same stack and look as
exponential.at (`apps/marketing`): Vite + React, prerendered to static HTML,
indexed, the shared chrome and styles of `@exp/site-shell`. Wherever it shows
components it renders them with the SDK itself (`@exponential-at/ui-react`).

## Commands

```bash
bun run dev:ui-site        # vite dev server (regenerates the guide sources first)
bun run build:ui-site      # guide sources → tsc → vite build → prerender of every route
bun --filter @exp/ui-site typecheck
bun --filter @exp/ui-site test
bun --filter @exp/ui-site check:guides        # every guide example from a fresh project
cd apps/ui-site && bun run scripts/generate-og.tsx   # rewrites public/og/og-ui.png (by hand)
```

## Pages

`src/lib/routes.ts` is THE page manifest: every path, its page module and its
SEO (title, description, the OG image `public/og/og-ui.png`, JSON-LD). The
prerender (`scripts/prerender.tsx`, on `@exp/site-shell/prerender`) writes one
`dist/<path>/index.html` per entry plus `sitemap.xml` and `llms.txt`; the
client entry loads each page type as its own chunk through `src/lib/pages.ts`
(`routes.test.ts` keeps the two in lockstep).

| path | page | what |
|---|---|---|
| `/` | `Home.tsx` | generative UI in one screen: the live demo (`src/sdk/HomeDemo.tsx`), the four renderers, the pitch, entry points, the packages |
| `/concepts/` | `Concepts.tsx` | A2UI in short, catalog, A2UI basic mapped, themes, extensions, host plugins, vapps, how renderers agree |
| `/components/`, `/components/<slug>/` | `ComponentsIndex.tsx`, `Component.tsx` | one page per core component: live render, A2UI JSON, props, four native shots |
| `/themes/`, `/themes/builder/` | `Themes.tsx`, `ThemeBuilder.tsx` | the built-in themes; the theme builder |
| `/guides/`, `/guides/<slug>/` | `Guides.tsx`, `Guide.tsx` | the guides hub; every guide by slug (`src/lib/guides.ts`) |
| `/playground/` | `Playground.tsx` | paste A2UI, render it in any theme, read the catalog prompt |
| `/conformance/` | `Conformance.tsx` | the suites (from the manifest), running and checking a report, the real-font harness |

Pages must prerender under Bun: no `window`/`document` at render time, no
`?raw` imports, no CSS imports in pages. Styles: `src/styles.css` (the shared
site-shell sheets), `src/styles/content.css` (home, concepts, guides,
conformance), `src/styles/sdk.css` (the SDK-rendering widgets).

## Data

Nothing is copied into this app; it reads the SDK's generated files:

- `@exponential-at/ui/docs/components.generated.json` and
  `docs/themes.generated.json`: the component and theme docs (`src/lib/catalog.ts`);
- `@exponential-at/ui/fixtures/specimens.json`: one surface per component
  plus the home demo;
- `@exponential-at/ui/conformance/manifest.json`: the conformance suites;
- the screenshot store `shots/exponential-ui-*` at the repo root (four
  platforms per specimen), served in dev and copied into `dist/shots/` at
  build time by `vite.config.ts`, along with exponential.at's fonts and brand
  assets from `apps/marketing/public`.

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
