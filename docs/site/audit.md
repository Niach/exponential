# exponential.at audit (phase 1 of the Astro + Starlight rework)

Snapshot: the prerendered build of `apps/marketing` at `/tmp/site-spike-old-dist/` (master `b44d61d8ef`, 2026-11-03): 152 `index.html` pages, the same 152 URLs as `sitemap.xml`.

## Method

`/tmp/site-audit/count.ts` (bun, no dependencies), reproducible with

```bash
bun /tmp/site-audit/count.ts /tmp/site-spike-old-dist > /tmp/site-audit/pages.json
bun /tmp/site-audit/classify.ts          # totals and medians per register
```

Per page: only the text inside `<main>` counts; `<script>`, `<style>`, `<svg>`, `<noscript>`, `<template>` and every `<nav>`, `<aside>`, `<footer>` inside `<main>` (docs sidebars, TOCs, pagers) are dropped; the site header and footer sit outside `<main>`. A word is a whitespace-separated token holding a letter or digit. Text inside `<pre>`/`<code>` counts separately as **code words**. **Pics** = `<img>`/`<video>`/`<picture>`/`<canvas>`/`<iframe>` inside `<main>` (live React islands that prerender to markup count as text, so a live demo with no static shot reads as 0 pics). **Contrast** = regex hits for the banned pattern on prose (`, never|not …`, `It's not … It's`, `not … but`, `— not|never`, `no … . Just`); a heuristic: ~5 of 89 hits are false positives (table cells such as `/talk/api/`'s "not used yet"). German = lines with two or more German function words or umlauts, checked by hand. Stale vocabulary = `workspace|project|action|thread` hits, each read in context.

## 1. Summary

| | pages | prose words | median | max |
|---|---:|---:|---:|---:|
| landing (/, pricing, download, contact) | 4 | 889 | 335 | 435 |
| product (/ui/, /talk/, /apps/, /net/, /pilot/, /mcp/, /agent/, /core/) | 8 | 2,888 | 452 | 563 |
| guide (docs how-tos, eUI + eTalk guides) | 22 | 15,855 | 375 | 4,295 |
| docs index pages (/docs/, /ui/guides/, /ui/components/, /talk/guides/) | 4 | 2,007 | 519 | 1,151 |
| reference (concepts, numbers, eval, conformance, API, CLI, MCP, self-host, widget, themes, eTalk reference) | 14 | 21,946 | 1,606 | 3,844 |
| eUI component reference | 91 | 18,924 | 136 | 1,233 |
| tools (playground, theme builder) | 2 | 103 | 62 | 62 |
| blog | 4 | 2,836 | 828 | 1,291 |
| legal | 3 | 2,051 | 424 | 1,572 |
| **all** | **152** | **67,499** | | |

Plus 16,631 code words. User-facing copy to halve (landing + product + guides + indexes) = 21,639 words → target ≈ 10,800. 89 contrast-pattern hits on 46 pages. 48 pages carry no picture, among them 6 of 8 product pages.

Top problems:

1. **eApps and Exponential's own apps share one page.** `/docs/apps/` (3,244 words, `src/AppsDocsPage.tsx`) is titled "Apps", its hero reads "The apps your team builds inside Exponential … and Exponential's own apps for desktop, iOS and Android"; §01–09 are eApps, §10–13 are the desktop app, the phones, push and steering. The home page's "Apps, built by asking" and `/apps/` link here for both meanings.
2. **eApps is told five times.** `/apps/`, `/docs/apps/` §01–09, `/ui/guides/apps/`, the Apps section of `/ui/concepts/`, and `docs/spec/apps.md` §0 each explain "one package, three things run it, data never code" (the phrase appears on `/`, `/apps/`, `/docs/apps/` ×2, `/ui/guides/apps/`, `/ui/concepts/` ×2).
3. **Two pages called MCP mean two different things.** `/mcp/` = eMCP, the device library; `/docs/mcp/` = connecting a client to Exponential's own MCP server. Both describe the app tool broker (`/api/mcp/apps`), and so does `/docs/apps/` "Exposes and upstreams".
4. **Guides and references far over any budget, hand-written copies of repo sources.** `/docs/coding/` 4,295, `/ui/eval/` 3,844, `/docs/self-host/` 3,352 (parallel to `INSTALL.md`), `/docs/apps/` 3,244, `/docs/issues/` 2,118, `/ui/numbers/` 1,983, `/docs/mcp/` 1,984 (its tool reference restates `apps/web/src/lib/emcp` tool descriptions by hand), `/docs/cli/` 1,838 (misses `exponential motion`, which `crates/cli/src/main.rs` dispatches).
5. **Product pages carry reference and no picture.** `/net/`, `/pilot/`, `/mcp/`, `/agent/`, `/core/` have 0 pics and spend half their words on Install / Packages / Status tables; `/agent/`, `/mcp/`, `/pilot/` repeat the same git-tag install boilerplate from `docs/libraries.md`. `/talk/` (110 words) has no static picture. Motion has no product page and is missing from the home page's library list.
6. Smaller: the URL `/docs/actions/` (+ `ActionsDocsPage.tsx`, `entry-docs-actions.tsx`) for a page titled Prompts, half of it about agents and triggers; 14 eUI guides end in the same "Compiled in CI" section; `/docs/` Quickstart repeats `/docs/getting-started/`; two components prerender "Shot pending" (`choice-tiles`, `search-list`); the coding docs' live mock shows the name "Danny Strähhuber" while the imprint names Dennis Strähhuber.

Vocabulary: no stale product vocabulary in copy. Every `workspace` hit is a compositor workspace (blog), every `project` a Firebase/tsconfig/Vite project, every `action` an A2UI or eApps package action (the spec's own term), every `thread` the top session (current MCP wording) or a CPU thread. The one stale item is the `/docs/actions/` path and its file names. Prompts are current (`settings-prompts` shots, MCP `promptId`, `exponential_prompts`), confirming CLAUDE.md.

German: none stray. `/imprint/` carries the legally required German headings ("Angaben gemäß § 5 DDG", "Verantwortlich für den Inhalt gemäß § 18 Abs. 2 MStV"; keep). `/talk/install/` and `/talk/guides/your-voice/` show the UI label "Deutsch" in a language picker (keep).

## 2. Inventory

Columns: **W** prose words · **C** code words · **P** pics · **×** contrast hits. Audience: B buyer, U user, D developer, A agent. Register: L landing, P product, G guide, R reference, I index, T tool, Lg legal, Bl blog.

### Landing, legal, blog

| URL | W | C | P | × | aud. | reg. | duplicates | problems |
|---|---:|---:|---:|---:|---|---|---|---|
| `/` | 435 | 9 | 10 | 3 | B | L | each section restates a page: Talk to it = `/talk/`; Agents work your boards = `/docs/coding/`; Sealed = `/net/` + `/docs/coding/`; Generative UI = `/ui/`; Apps = `/apps/`; Free for three = `/pricing/` | Open libraries lists 8 libs, no Motion; "Open libraries" section 168 words (over 150); 3 contrast |
| `/pricing/` | 335 | 0 | 0 | 1 | B | L | Firebase push note = `/docs/self-host/` Push; "never a requirement" = `/docs/self-host/` Licensing | no picture |
| `/download/` | 47 | 0 | 0 | 0 | U | L | `/docs/apps/` §10 Desktop, `/docs/cli/` Install | no picture; platform detail lives on `/docs/apps/` |
| `/contact/` | 72 | 0 | 0 | 0 | B | L | – | – |
| `/privacy/` | 1572 | 1 | 0 | 1 | B/U | Lg | – | 1 contrast |
| `/terms/` | 424 | 0 | 0 | 0 | B/U | Lg | – | – |
| `/imprint/` | 55 | 0 | 0 | 0 | B | Lg | – | German headings (required, keep) |
| `/blog/` | 39 | 0 | 3 | 0 | B/D | Bl | – | – |
| `/blog/exponentialos/` | 678 | 32 | 2 | 0 | B/D | Bl | `/blog/ui-as-data-down-to-the-compositor/` (eDE, same `os-stack`/`watch-view` pictures); library list = `/` Open libraries | – |
| `/blog/session-as-interface/` | 828 | 1 | 11 | 2 | B/D | Bl | `/docs/coding/` (stories, asks, plans) | 2 contrast |
| `/blog/ui-as-data-down-to-the-compositor/` | 1291 | 0 | 5 | 0 | D | Bl | `/blog/exponentialos/`, `/ui/` | – |

### Product pages

| URL | W | C | P | × | aud. | reg. | duplicates | problems |
|---|---:|---:|---:|---:|---|---|---|---|
| `/ui/` | 472 | 20 | 3 | 0 | B/D | P | Start here = `/ui/guides/`; intro = `/ui/concepts/` §01 | "Why eUI" 163 words (over 150); Packages table is reference |
| `/talk/` | 110 | 0 | 0 | 0 | B/U | P | "What works today" = `/talk/how-it-works/`; `/` Talk to it | live island only, no static picture; thin |
| `/apps/` | 388 | 52 | 5 | 2 | B/D | P | `/docs/apps/` §01–09, `/ui/guides/apps/`, `/ui/concepts/` Apps, `docs/spec/apps.md` §0 | "data, never code" ×2 (headline); "The format, open" is reference code |
| `/net/` | 452 | 3 | 0 | 1 | D | P | `/docs/coding/` sealed section (same sentence); `crates/enet-core/README.md` | no picture; "In Exponential" 170 words; Packages = reference |
| `/pilot/` | 563 | 11 | 0 | 1 | D | P | install boilerplate = `/agent/`, `/mcp/`; `crates/epilot/README.md` | over 500; no picture; 2 sections > 120 words |
| `/mcp/` | 472 | 142 | 0 | 4 | D | P | broker = `/docs/mcp/` App tools + `/docs/apps/` Exposes; install boilerplate | name clash with `/docs/mcp/`; no picture; code-heavy for a product page; 4 contrast |
| `/agent/` | 272 | 6 | 0 | 2 | D | P | install boilerplate; `crates/eagent/README.md` | no picture; 2 contrast |
| `/core/` | 159 | 1 | 0 | 1 | D | P | `crates/ecore/README.md` | no picture; unreleased, thin |

### Docs (`src/*DocsPage.tsx`)

| URL | W | C | P | × | aud. | reg. | duplicates | problems |
|---|---:|---:|---:|---:|---|---|---|---|
| `/docs/` | 519 | 1 | 1 | 0 | U | I | Quickstart = `/docs/getting-started/`; "Browse the docs" cards = the sidebar | links "Prompts" to `/docs/actions/` |
| `/docs/getting-started/` | 1128 | 1 | 2 | 1 | U | G | Plans = `/pricing/`; Connect GitHub = `/docs/coding/` Setup | over guide budget |
| `/docs/self-host/` | 3352 | 204 | 0 | 1 | D/A | R | `INSTALL.md` (2,064 words, agent-shaped, the image install), `selfhost/.env.example`, `/pricing/` | env-var table hand-copied from `.env.example`; no picture |
| `/docs/issues/` | 2118 | 45 | 7 | 4 | U | G | Branches & PRs = `/docs/coding/` Review & merge | over budget; 4 contrast |
| `/docs/coding/` | 4295 | 1 | 11 | 7 | U | G | Setup = `/docs/cli/` + `/download/`; agents = `/docs/actions/` §04; sealed = `/net/`; git IDE = desktop app | 4× guide budget; 7 contrast; mock shows "Danny Strähhuber" |
| `/docs/feedback/` | 588 | 3 | 0 | 1 | U | G | `/docs/widget/` "What lands in Exponential" | no picture (shots `support-reporter`, `settings-widget` exist) |
| `/docs/widget/` | 1159 | 473 | 0 | 0 | D | R | `/docs/feedback/` | no picture; JS API reference mixed with install how-to |
| `/docs/actions/` | 520 | 1 | 5 | 0 | U | G | §04 Schedules and events (agents + triggers) = `/docs/coding/` agents | stale path "actions" for Prompts; half the page is agents |
| `/docs/cli/` | 1838 | 189 | 0 | 2 | D/A | R | Install = `/download/`; daemon = `/docs/self-host/` | misses `exponential motion` (14 commands in `main.rs`); no picture |
| `/docs/mcp/` | 1984 | 87 | 0 | 1 | D/A | R | App tools = `/mcp/` + `/docs/apps/`; tool list = `lib/emcp` descriptions | hand copy of tool reference (drift); name clash with `/mcp/` |
| `/docs/apps/` | 3244 | 301 | 5 | 7 | U/D | G+R | §01–09 = `/apps/` + `/ui/guides/apps/` + `docs/spec/apps.md`; Voice = `/talk/install/`; Push = `/docs/self-host/`; Desktop = `/download/` | **eApps mixed with desktop/iOS/Android apps**; 7 contrast; 4× budget |

### eUI section (`ui/src/pages/*`)

| URL | W | C | P | × | aud. | reg. | duplicates | problems |
|---|---:|---:|---:|---:|---|---|---|---|
| `/ui/concepts/` | 1606 | 196 | 0 | 5 | D/A | R | Apps = `/apps/`; Motion = `/ui/guides/motion/`; Themes = `/ui/themes/`; `packages/exponential-ui/README.md` | 5 contrast; no picture |
| `/ui/guides/` | 313 | 13 | 0 | 0 | D | I | `/ui/` Start here | – |
| `/ui/guides/agents/` | 375 | 288 | 0 | 0 | D/A | G | `packages/exponential-ui/README.md` | "Compiled in CI" boilerplate |
| `/ui/guides/apps/` | 510 | 997 | 0 | 1 | D | G | `/docs/apps/` package + records, `/apps/` format, `packages/exponential-ui/docs/package-format.md` | eApps content filed under eUI |
| `/ui/guides/compose/` | 197 | 173 | 0 | 0 | D | G | `packages/exponential-ui-compose/README.md` | boilerplate |
| `/ui/guides/define/` | 286 | 226 | 0 | 0 | D | G | element/solid "A surface in TypeScript" | boilerplate |
| `/ui/guides/element/` | 358 | 672 | 0 | 0 | D | G | `packages/exponential-ui-element/README.md`, `/ui/guides/define/` | boilerplate |
| `/ui/guides/extensions/` | 296 | 375 | 0 | 0 | D | G | `/ui/concepts/` Extensions | boilerplate |
| `/ui/guides/gpui/` | 250 | 293 | 0 | 0 | D | G | `crates/eui-gpui/README.md` | boilerplate |
| `/ui/guides/host-plugins/` | 394 | 211 | 0 | 0 | D | G | `/ui/concepts/` Host plugins | boilerplate |
| `/ui/guides/motion/` | 1124 | 457 | 0 | 2 | D/A | G+R | `/ui/components/motion/`, `/ui/concepts/` Motion, `docs/spec/motion.md`, `crates/eui-motion/README.md` | over budget; dialect reference inside a guide; Motion gets no section of its own |
| `/ui/guides/react/` | 305 | 192 | 0 | 0 | D | G | `packages/exponential-ui-react/README.md` | boilerplate |
| `/ui/guides/security/` | 970 | 3 | 0 | 2 | D | R | – | reference filed as a guide |
| `/ui/guides/solid/` | 343 | 712 | 0 | 0 | D | G | `packages/exponential-ui-solid/README.md`, `/ui/guides/define/` | boilerplate |
| `/ui/guides/swiftui/` | 242 | 152 | 0 | 0 | D | G | `packages/exponential-ui-swift/README.md` | boilerplate |
| `/ui/guides/themes/` | 322 | 87 | 0 | 0 | D | G | `/ui/themes/`, `/ui/themes/builder/` | – |
| `/ui/themes/` | 153 | 35 | 0 | 0 | D | R | `/ui/guides/themes/` | – |
| `/ui/themes/builder/` | 41 | 1 | 0 | 0 | D | T | – | – |
| `/ui/playground/` | 62 | 4398 | 0 | 0 | D/A | T | the catalog prompt = `/ui/guides/agents/` | 4,398-word system prompt inlined in the HTML |
| `/ui/conformance/` | 1159 | 76 | 0 | 2 | D | R | `packages/exponential-ui/conformance/README.md` | – |
| `/ui/numbers/` | 1983 | 50 | 0 | 1 | D | R | `packages/exponential-ui-bench/README.md` | – |
| `/ui/eval/` | 3844 | 120 | 0 | 1 | D/A | R | `packages/exponential-ui/eval/README.md` | the site's largest page; internal eval report |
| `/ui/components/` | 1151 | 4 | 0 | 1 | D | I | the 91 component pages' summaries | long for an index |

### eUI components (`/ui/components/<slug>/`, `ui/src/pages/Component.tsx` over `packages/exponential-ui/catalog`)

Audience D/A, register R, each page restates its own catalog entry; P = 4 means one shot per platform (web, iOS, Android, desktop). "over budget" = above 250 prose words.

| slug | W | C | P | problems |
|---|---:|---:|---:|---|
| accordion | 110 | 58 | 4 | – |
| alert-dialog | 131 | 53 | 4 | – |
| alert | 228 | 49 | 4 | 1× contrast |
| app-bar | 115 | 48 | 4 | – |
| audio-player | 87 | 34 | 4 | – |
| avatar | 194 | 40 | 4 | – |
| badge | 247 | 44 | 4 | – |
| box | 101 | 72 | 4 | – |
| breadcrumb | 98 | 43 | 4 | – |
| button | 711 | 69 | 4 | over budget, 1× contrast |
| call-surface | 166 | 64 | 4 | – |
| card | 250 | 61 | 4 | 1× contrast |
| carousel | 125 | 77 | 4 | – |
| chart | 215 | 53 | 4 | – |
| checkbox | 188 | 36 | 4 | – |
| chip-input | 150 | 36 | 4 | – |
| chip | 474 | 47 | 4 | over budget |
| choice-tiles | 158 | 61 | 0 | shot pending |
| code-block | 133 | 60 | 4 | – |
| collapsible | 401 | 52 | 4 | over budget |
| composer | 252 | 34 | 4 | over budget |
| date-picker | 291 | 35 | 4 | over budget, 1× contrast |
| date-range-picker | 132 | 38 | 4 | – |
| dialog | 591 | 106 | 4 | over budget, 3× contrast |
| dock-card | 147 | 115 | 4 | – |
| drawer | 205 | 78 | 4 | 1× contrast |
| empty-state | 553 | 43 | 4 | over budget, 2× contrast |
| field-group | 79 | 54 | 4 | – |
| file-upload | 145 | 37 | 4 | – |
| form-actions | 95 | 33 | 4 | – |
| form | 123 | 65 | 4 | – |
| grid | 84 | 110 | 4 | – |
| group | 354 | 64 | 4 | over budget, 2× contrast |
| heading | 86 | 41 | 4 | – |
| icon | 104 | 49 | 4 | – |
| image | 176 | 47 | 4 | – |
| input | 455 | 46 | 4 | over budget, 1× contrast |
| kbd | 72 | 31 | 4 | – |
| label | 98 | 33 | 4 | – |
| link | 84 | 34 | 4 | – |
| list | 157 | 69 | 4 | – |
| markdown | 97 | 36 | 4 | – |
| menu | 665 | 60 | 4 | over budget |
| message-bubble | 104 | 49 | 4 | – |
| meter | 298 | 48 | 4 | over budget |
| mosaic | 93 | 47 | 4 | – |
| motion | 280 | 77 | 4 | over budget |
| number-field | 159 | 41 | 4 | – |
| outline | 109 | 60 | 4 | – |
| page | 128 | 79 | 4 | – |
| pagination | 97 | 33 | 4 | – |
| plan | 142 | 90 | 4 | – |
| popover | 230 | 61 | 4 | – |
| progress | 93 | 46 | 4 | – |
| qr-code | 99 | 42 | 4 | – |
| qr-scanner | 103 | 41 | 4 | – |
| radio | 116 | 47 | 4 | – |
| rating | 119 | 38 | 4 | – |
| resizable | 161 | 68 | 4 | – |
| ring | 310 | 49 | 4 | over budget, 1× contrast |
| row | 1233 | 67 | 4 | over budget, 5× contrast |
| scroll-area | 97 | 138 | 4 | – |
| search-list | 246 | 57 | 0 | shot pending |
| section | 665 | 137 | 4 | over budget, 2× contrast |
| segmented | 314 | 62 | 4 | over budget |
| select | 885 | 54 | 4 | over budget, 3× contrast |
| separator | 124 | 34 | 4 | 1× contrast |
| sidebar | 126 | 100 | 4 | – |
| skeleton | 188 | 33 | 4 | – |
| slider | 94 | 37 | 4 | – |
| sparkline | 103 | 46 | 4 | – |
| spinner | 124 | 38 | 4 | – |
| split-view | 122 | 147 | 4 | – |
| stack | 120 | 82 | 4 | – |
| stat-tile | 104 | 57 | 4 | – |
| stepper | 99 | 41 | 4 | – |
| switch | 245 | 35 | 4 | – |
| table | 208 | 70 | 4 | – |
| tabs | 222 | 68 | 4 | – |
| text | 136 | 62 | 4 | – |
| textarea | 197 | 38 | 4 | – |
| time-picker | 116 | 35 | 4 | – |
| toast | 154 | 51 | 4 | 1× contrast |
| toggle | 95 | 43 | 4 | – |
| toolbar | 84 | 75 | 4 | – |
| tooltip | 212 | 47 | 4 | – |
| trace-item | 134 | 72 | 4 | – |
| trace | 77 | 142 | 4 | – |
| video | 119 | 33 | 4 | – |
| voice-wave | 164 | 68 | 4 | 1× contrast |
| zoomable-image | 149 | 49 | 4 | – |

### eTalk section (`ui/src/talk/*`)

| URL | W | C | P | × | aud. | reg. | duplicates | problems |
|---|---:|---:|---:|---:|---|---|---|---|
| `/talk/how-it-works/` | 337 | 0 | 0 | 1 | U/D | R | `/talk/` table; `/docs/apps/` Voice; `crates/etalk/README.md` | no picture |
| `/talk/install/` | 640 | 0 | 1 | 2 | U | G | `/docs/apps/` Voice (Settings → Assistant → Voice twice) | 2 contrast |
| `/talk/guides/` | 24 | 0 | 0 | 0 | U | I | – | thin index |
| `/talk/guides/push-to-talk/` | 327 | 0 | 0 | 0 | U | G | `/talk/install/` Push to talk | no picture |
| `/talk/guides/your-voice/` | 267 | 17 | 1 | 0 | U | G | `/talk/install/`; `public/talk/skills/voice-reference-clip/SKILL.md` | – |
| `/talk/api/` | 695 | 50 | 0 | 1 | D/A | R | `packages/etalk/README.md`, `crates/etalk/README.md` | 1 hit is a false positive |
| `/talk/numbers/` | 310 | 0 | 0 | 1 | D | R | `docs/talk-spike.md`, `apps/desktop/spikes/talk/numbers.json`, `/talk/how-it-works/` latency table | – |
| `/talk/licences/` | 282 | 0 | 0 | 0 | D | R | `docs/third-party-licences.md` (eTalk sections) | – |

## 3. Old URL → new location

Product pages keep their URLs and lose their reference sections to the docs (marked "split").

| old | new | note |
|---|---|---|
| `/` | `/` | Open libraries gains Motion |
| `/pricing/` `/download/` `/contact/` `/privacy/` `/terms/` `/imprint/` | same | |
| `/blog/` and its 3 posts | same | |
| `/ui/` | `/ui/` | split: Packages → `/docs/libraries/eui/` |
| `/talk/` | `/talk/` | |
| `/apps/` | `/apps/` | split: "The format, open" → `/docs/eapps/reference/` |
| `/net/` `/pilot/` `/mcp/` `/agent/` `/core/` | same | split: Status / Install / Packages → `/docs/libraries/<lib>/` |
| — | `/motion/` | new |
| `/docs/` | `/docs/` | Quickstart moves to `/docs/get-started/` |
| `/docs/getting-started/` | `/docs/get-started/` | |
| `/docs/self-host/` | `/docs/get-started/self-host/` | |
| `/docs/issues/` | `/docs/using/issues/` | |
| `/docs/coding/` | `/docs/using/sessions/` | Setup → `/docs/get-started/`; also receives agents + triggers |
| `/docs/feedback/` | `/docs/using/feedback/` | |
| `/docs/widget/` | `/docs/using/widget/` | |
| `/docs/apps/` | `/docs/eapps/` | §01–02 → `/docs/eapps/`; §03–09 → `/docs/eapps/build/` + `/docs/eapps/reference/`; §11–13 Mobile, Push, Steer → `/docs/using/mobile-apps/`; §10 Desktop → see Gaps; Voice → `/docs/libraries/etalk/guides/install/` |
| `/docs/actions/` | `/docs/build/prompts/` | §04 agents + triggers → `/docs/using/sessions/` |
| `/docs/cli/` | `/docs/build/cli/` | |
| `/docs/mcp/` | `/docs/build/mcp/` | |
| `/ui/concepts/` | `/docs/libraries/eui/` | Overview; Apps → `/docs/eapps/`, Motion → `/docs/libraries/motion/` |
| `/ui/guides/` | `/docs/libraries/eui/guides/` | |
| `/ui/guides/{agents,compose,define,element,extensions,gpui,host-plugins,react,solid,swiftui,themes}/` | `/docs/libraries/eui/guides/<same slug>/` | |
| `/ui/guides/security/` | `/docs/libraries/eui/reference/security/` | |
| `/ui/guides/apps/` | `/docs/eapps/build/` | |
| `/ui/guides/motion/` | `/docs/libraries/motion/guides/animate/` | dialect reference → `/docs/libraries/motion/reference/dialect/` |
| `/ui/components/` | `/docs/libraries/eui/reference/components/` | |
| `/ui/components/<slug>/` (all 91, `motion` included) | `/docs/libraries/eui/reference/components/<slug>/` | |
| `/ui/themes/` | `/docs/libraries/eui/reference/themes/` | |
| `/ui/themes/builder/` | `/docs/libraries/eui/tools/theme-builder/` | |
| `/ui/playground/` | `/docs/libraries/eui/tools/playground/` | |
| `/ui/conformance/` | `/docs/libraries/eui/reference/conformance/` | |
| `/ui/numbers/` | `/docs/libraries/eui/reference/numbers/` | |
| `/ui/eval/` | `/docs/libraries/eui/reference/eval/` | |
| `/talk/how-it-works/` | `/docs/libraries/etalk/` | Overview |
| `/talk/install/` | `/docs/libraries/etalk/guides/install/` | |
| `/talk/guides/` | `/docs/libraries/etalk/guides/` | |
| `/talk/guides/push-to-talk/` | `/docs/libraries/etalk/guides/push-to-talk/` | |
| `/talk/guides/your-voice/` | `/docs/libraries/etalk/guides/your-voice/` | |
| `/talk/api/` | `/docs/libraries/etalk/reference/api/` | |
| `/talk/numbers/` | `/docs/libraries/etalk/reference/numbers/` | |
| `/talk/licences/` | `/docs/libraries/etalk/reference/licences/` | |

Non-HTML files keep their paths: `/llms.txt`, `/sitemap.xml` (both regenerated), `/robots.txt`, `/SKILL.md`, `/install.sh`, `/NOTICES.txt`, `/talk/skills/voice-reference-clip/SKILL.md`, `/eapps/*.webp`, `/shots/**`, `/og/**`, `/social/**`, `/fonts/**`.

## 4. Redirects

Where 301s live today: `apps/marketing/public/serve.json` → `redirects` (the static server's config, copied into `dist/`; two entries now: `/blog/what-an-assistant-should-do` and `/skills/voice-reference-clip/SKILL.md`). `ui.exponential.at` 301s to the same path under `exponential.at/ui/` from `apps/marketing/ui/scripts/redirect.ts`, run by the Coolify app `exponential-ui-site` (documented in `apps/marketing/ui/README.md`).

The new redirects belong in `apps/site/public/serve.json` (carry over the two existing entries). Astro's `redirects` option emits meta-refresh pages in a static build, so serve.json stays the 301 source; mirror it in `astro.config` only if dev needs it. Update `redirect.ts` to send `ui.exponential.at/<path>` straight to the final URL, or accept two hops (ui.exponential.at → `/ui/…` → `/docs/…`). With `trailingSlash: true` each source matches with and without the slash.

```
/docs/getting-started/            → /docs/get-started/ 301
/docs/self-host/                  → /docs/get-started/self-host/ 301
/docs/issues/                     → /docs/using/issues/ 301
/docs/coding/                     → /docs/using/sessions/ 301
/docs/feedback/                   → /docs/using/feedback/ 301
/docs/widget/                     → /docs/using/widget/ 301
/docs/apps/                       → /docs/eapps/ 301
/docs/actions/                    → /docs/build/prompts/ 301
/docs/cli/                        → /docs/build/cli/ 301
/docs/mcp/                        → /docs/build/mcp/ 301
/ui/concepts/                     → /docs/libraries/eui/ 301
/ui/guides/                       → /docs/libraries/eui/guides/ 301
/ui/guides/agents/                → /docs/libraries/eui/guides/agents/ 301
/ui/guides/compose/               → /docs/libraries/eui/guides/compose/ 301
/ui/guides/define/                → /docs/libraries/eui/guides/define/ 301
/ui/guides/element/               → /docs/libraries/eui/guides/element/ 301
/ui/guides/extensions/            → /docs/libraries/eui/guides/extensions/ 301
/ui/guides/gpui/                  → /docs/libraries/eui/guides/gpui/ 301
/ui/guides/host-plugins/          → /docs/libraries/eui/guides/host-plugins/ 301
/ui/guides/react/                 → /docs/libraries/eui/guides/react/ 301
/ui/guides/solid/                 → /docs/libraries/eui/guides/solid/ 301
/ui/guides/swiftui/               → /docs/libraries/eui/guides/swiftui/ 301
/ui/guides/themes/                → /docs/libraries/eui/guides/themes/ 301
/ui/guides/security/              → /docs/libraries/eui/reference/security/ 301
/ui/guides/apps/                  → /docs/eapps/build/ 301
/ui/guides/motion/                → /docs/libraries/motion/guides/animate/ 301
/ui/components/                   → /docs/libraries/eui/reference/components/ 301
/ui/components/:slug/             → /docs/libraries/eui/reference/components/:slug/ 301   (91 slugs: accordion … zoomable-image, listed in §2)
/ui/themes/                       → /docs/libraries/eui/reference/themes/ 301
/ui/themes/builder/               → /docs/libraries/eui/tools/theme-builder/ 301
/ui/playground/                   → /docs/libraries/eui/tools/playground/ 301
/ui/conformance/                  → /docs/libraries/eui/reference/conformance/ 301
/ui/numbers/                      → /docs/libraries/eui/reference/numbers/ 301
/ui/eval/                         → /docs/libraries/eui/reference/eval/ 301
/talk/how-it-works/               → /docs/libraries/etalk/ 301
/talk/install/                    → /docs/libraries/etalk/guides/install/ 301
/talk/guides/                     → /docs/libraries/etalk/guides/ 301
/talk/guides/push-to-talk/        → /docs/libraries/etalk/guides/push-to-talk/ 301
/talk/guides/your-voice/          → /docs/libraries/etalk/guides/your-voice/ 301
/talk/api/                        → /docs/libraries/etalk/reference/api/ 301
/talk/numbers/                    → /docs/libraries/etalk/reference/numbers/ 301
/talk/licences/                   → /docs/libraries/etalk/reference/licences/ 301
/blog/what-an-assistant-should-do → /blog/ui-as-data-down-to-the-compositor/ 301   (existing)
/skills/voice-reference-clip/SKILL.md → /talk/skills/voice-reference-clip/SKILL.md 301   (existing)
```

serve.json takes `:slug` patterns; one rule covers the 91 component pages. 133 of the 152 old URLs change path; the 19 that stay are the landing, legal and blog pages and the 8 product pages.

## 5. Gaps and reference sources

### Pages the new IA needs that do not exist

| new page | built from |
|---|---|
| `/motion/` (product) | `/ui/guides/motion/` intro, `/ui/components/motion/`, shot `shots/exponential-ui-motion/` |
| `/docs/using/mobile-apps/` | `/docs/apps/` §11–13 (Mobile, Push, Steer from anywhere); shots `phone-tab-bar`, `phone-board-menu`, `steering`, `board/ios`, `board/android` |
| Desktop app home | `/docs/apps/` §10 (download, self-updating, layout, notifications, voice) has no slot in the agreed IA. Proposal: install + updating → `/docs/get-started/`, layout + git IDE → `/docs/using/sessions/`. Needs a decision. |
| `/docs/eapps/build/` | `/ui/guides/apps/` + `/docs/apps/` §02 "Build one with an agent" |
| `/docs/eapps/reference/` | `packages/exponential-ui/docs/package-format.md` (the package format: versions, records, expressions, actions, tests, tools, exposes, extensions) + `docs/spec/apps.md` §2–15. The spec carries decisions, gap list and readiness (§17–20), which belong on the spec index rather than the reference. |
| `/docs/libraries/<lib>/` overviews for enet, epilot, emcp, eagent, ecore | each product page's Status / Install / Packages sections + the crate README |
| `/docs/libraries/<lib>/guides/install/` for the git-dependency libs | `docs/libraries.md` "Consuming from another repo" (one shared page, linked from each lib, replaces the boilerplate on `/agent/`, `/mcp/`, `/pilot/`) |
| `/docs/libraries/motion/` (overview, guides/animate, reference/dialect, reference/engine) | `docs/spec/motion.md`, `crates/eui-motion/README.md`, `packages/exponential-ui/eval/motion/guide.md`, `/ui/guides/motion/`; CLI `exponential motion` from `crates/cli/src/commands/motion.rs` |
| `/docs/reference/changelog/` | `apps/web/src/lib/changelog.ts` (`CHANGELOG`, the in-app What's new; Rust twin `crates/ui/src/changelog.rs`) + `packages/exponential-ui*/CHANGELOG.md` for eUI |
| `/docs/reference/spec/` | index over `docs/spec/apps.md`, `docs/spec/motion.md`, `packages/exponential-ui/docs/round-{1,2,3,5,6}-contract.md` |
| `/docs/using/sessions/` agents + triggers | `/docs/actions/` §04 + `/docs/coding/` agents |

### Repo markdown, what it covers, where it feeds

`docs/` holds only six markdown files; most reference pages have no markdown source today and are hand-written TSX. Proposed source per reference page:

| source | covers | feeds |
|---|---|---|
| `docs/spec/apps.md` (9,698 w) | eApps spec: package, records, expressions, UI, actions, automation, MCP, access, import/export, security, runtime, sync, limits, roadmap, decisions | `/docs/eapps/reference/` (with package-format.md), `/docs/reference/spec/` |
| `docs/spec/motion.md` (1,350 w) | Motion: sources, limits, dialect, engine, display list, playback, interactivity, tooling | `/docs/libraries/motion/reference/*` |
| `docs/libraries.md` (1,631 w) | library names, packages per registry, tags, git-tag policy, published state | `/docs/libraries/` index, shared install guide |
| `docs/talk-spike.md` (5,422 w) | eTalk engine measurements and decisions (internal log) | `/docs/libraries/etalk/reference/numbers/` via `apps/desktop/spikes/talk/numbers.json`; the log itself stays internal |
| `docs/third-party-licences.md` (4,570 w) | non-OSS components; eTalk voice, engine, wake word models; Xwayland | `/docs/libraries/etalk/reference/licences/` |
| `docs/shared-ui-inventory.md` (2,081 w) | 2026-10-04 snapshot of primitives per platform | internal; no page |
| `INSTALL.md` (2,064 w) | self-hosted install from published images, upgrade, backup, troubleshooting, licensing | `/docs/get-started/self-host/` (single source; drop the TSX copy) |
| `selfhost/.env.example`, root `.env.example` | env vars | self-host env table (generate) |
| `README.md` | product, libraries, self-host, MCP client | `/docs/` |
| `apps/web/src/lib/emcp/{nouns,tools}.ts` + `contract.expToolDisplay` | the 22 MCP tools and their ops | `/docs/build/mcp/` tool reference (generate) |
| `apps/desktop/crates/cli/src/main.rs` (`USAGE`, 14 commands) | CLI | `/docs/build/cli/` command table (generate) |
| `apps/desktop/crates/coding/src/runbook.md` | the in-session runbook for agents | `/docs/using/sessions/` agent appendix or `/llms.txt` |
| `packages/exponential-ui/catalog/*.json`, `docs/components.generated.json`, `docs/themes.generated.json` | components, themes | eUI component + theme reference (already generated) |
| `packages/exponential-ui/README.md`, `crates/eui-core/README.md` | eUI core, wire format | `/docs/libraries/eui/` |
| `packages/exponential-ui-{react,solid,element,swift,compose}/README.md`, `crates/eui-gpui/README.md` | per-renderer install + API | the six renderer guides (dedupe) |
| `packages/exponential-ui/conformance/README.md`, `packages/exponential-ui-bench/README.md`, `packages/exponential-ui/eval/README.md` | conformance, numbers, eval | the three eUI reference pages |
| `packages/exponential-ui/docs/package-format.md` | eApps package format | `/docs/eapps/reference/` |
| `crates/etalk/README.md`, `packages/etalk/README.md` | eTalk crates, contract | eTalk overview + API |
| `crates/enet-core/README.md` | eNet | `/docs/libraries/enet/` |
| `crates/epilot/README.md`, `crates/epilot-screen/README.md` | ePilot | `/docs/libraries/epilot/` |
| `crates/emcp/README.md`, `packages/emcp/README.md` | eMCP server, client, vault, contract | `/docs/libraries/emcp/` |
| `crates/eagent/README.md`, `packages/eagent/README.md`, `crates/eagent-hub-core/README.md` | eAgent runtime, orchestration spec | `/docs/libraries/eagent/` |
| `crates/ecore/README.md`, `packages/ecore/README.md` | eCore types | `/docs/libraries/ecore/` |
| `packages/domain-contract/fixtures/*.json` `$comment` | per-screen behaviour spec | optional `/docs/reference/spec/` appendix |
| `apps/marketing/public/SKILL.md`, `public/talk/skills/voice-reference-clip/SKILL.md` | agent skills | keep as files; link from `/llms.txt` |

`llms.txt` (5,163 words) and `sitemap.xml` are generated by `apps/marketing/scripts/prerender.tsx`; the Astro site needs an equivalent generator over the Starlight collection plus the landing pages.

## 6. Word budgets

Budgets count prose; code blocks and generated tables are excluded.

| page type | budget | measured now | basis |
|---|---|---|---|
| product section | ≤ 120, target 60–100, 1 picture | median ~58, max 170 (home, `/net/`, `/ui/`) | the 150 ceiling with headroom |
| product page | ≤ 450, 3–5 sections, ≥ 1 picture | median 452, 6 of 8 with no picture | half the user-facing total; reference moves to docs |
| landing: home | ≤ 350 | 435 | 8 sections × ~40 |
| landing: pricing / download / contact | ≤ 250 / 100 / 80 | 335 / 47 / 72 | |
| docs index (`/docs/`, group indexes) | ≤ 150 + generated cards | 24–1,151 | the sidebar carries navigation |
| library overview (`/docs/libraries/<lib>/`) | ≤ 600 | `/ui/concepts/` 1,606, `/talk/how-it-works/` 337 | |
| guide (how-to) | ≤ 800; split above 1,200 | median 375; `/docs/coding/` 4,295, `/docs/apps/` 3,244, `/docs/issues/` 2,118 | halves the guide total (15,855 → ~8,000) |
| reference | ≤ 2,500 per page, split by subject above; sourced or generated | median 1,606, max 3,844 | completeness wins over length; agents read it |
| component reference | ≤ 200 | median 136; 7 above 500 (`row` 1,233, `select` 885, `button` 711, `menu` 665, `section` 665, `dialog` 591, `empty-state` 553) | |
| blog, legal | no budget | | |
