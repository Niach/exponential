# exponential.at (apps/site)

The public site on Astro + Starlight: landing pages, product pages, docs,
the library docs (the old /ui/ site merged in), blog. The audit and
the URL map: `docs/site/audit.md`.

```bash
bun --filter @exp/site dev        # syncs reference pages, astro dev on :4321
bun --filter @exp/site build      # sync + astro build → dist/ (~2 s)
bun run apps/site/scripts/serve.ts 4391          # dist/ with production headers (CSP) + 301s
bun run apps/site/scripts/paint-check.ts http://localhost:4391 [shotsDir]
```

## Layout

| path | what |
|---|---|
| `src/pages/*.astro` | landing and product pages, each wrapped in `StarlightPage` (one header, search, theme) |
| `src/content/docs/docs/**` | the docs (Starlight), URL = path |
| `src/sidebar/<group>.ts` | the docs sidebar, one file per group |
| `src/styles/{theme,landing,docs,libraries}.css` | tokens (theme.css) + one file per area |
| `src/components/eui/EuiSurface.tsx` | eUI in Astro: no directive = rendered at build time (HTML, no JS); `client:visible` = hydrated island |
| `src/components/PlatformShots.astro` | one view of the repo's `shots/` store per platform |
| `scripts/reference/<area>.ts` | repo markdown → generated reference pages (`*.gen.md`, gitignored, `slug` in frontmatter) |
| `src/redirects.ts` | every 301 (old URL → new), written into `dist/serve.json` |
| `src/integrations/` | shot store copy, shared eUI theme sheet, serve.json (CSP hashes + redirects) |

## Writing

- Product and landing pages: a headline and a visual per section; a
  sentence only where the visual can't carry it. No eyebrow labels, card
  subtitles or descriptions, pills, tag chips or captions. A card = icon +
  short title + visual.
- Reference pages stay complete and precise (agents read them); source them
  from repo markdown through `scripts/reference/` wherever a file exists.
- Never use the contrast construction "X, never Y" / "X, not Y" / "It's
  not X. It's Y." / "not X, but Y" / "No X. Just Y.". State what the
  thing does.
- Pictures come from `shots/` (`/shots/<view>/<platform>.webp`); the build
  copies only the ones pages use.

## CSP

`serve.base.json` is the header template; the build fills `script-src`
with the sha256 of every inline script. It keeps `'wasm-unsafe-eval'`:
without it Motion's WASM engine is refused and every tile stays blank.
