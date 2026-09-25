/**
 * The renderer: `GalleryData` in, ONE self-contained HTML document out. Pure —
 * no disk, no network, no clock — so `build.ts` and `serve.ts` produce the same
 * bytes for the same store, and the page works over `file://`.
 *
 * The one input that is not data is `uiCss`: the compiled `@exp/ui` stylesheet
 * (EXP-887). Compiling it needs the Tailwind oxide scanner and a filesystem,
 * which is exactly the impurity this module refuses — so the CALLER compiles
 * it (`build.ts` once per build, `serve.ts` once per process) and passes the
 * string in. Empty = no islands are painted, which is what the pure tests
 * exercise when they do not care about CSS.
 */

import { PLATFORM_FRAME } from "@exp/view-catalog"
import type { Platform } from "@exp/view-catalog"
import {
  ISLAND_CLIENT_SCRIPT,
  renderIsland,
  renderIslandCssTemplate,
} from "@exp/ui/island"

import { client } from "./client.ts"
import { COMPONENTS, COMPONENT_PLATFORMS } from "./components.tsx"
import type {
  ComponentKind,
  ComponentPlatform,
  ComponentSpec,
  ComponentStatus,
} from "./components.tsx"
import { ENTRIES } from "./entries/index.ts"
import type { StyleguideEntry } from "./entries/types.ts"
import {
  buildPage,
  isPageIsland,
  sectionLabel,
  sectionShortLabel,
} from "./sections/page.ts"
import type { PageEntry, PageSection } from "./sections/page.ts"
import { escapeHtml } from "./html.ts"
import { styles } from "./styles.ts"
import type { GalleryData, Shot, ViewEntry } from "./store.ts"

const PLATFORM_LABEL: Record<Platform, string> = {
  web: `Web`,
  [`web-mobile`]: `Web mobile`,
  desktop: `Desktop`,
  ios: `iOS`,
  android: `Android`,
}

/** The component table has no web-mobile column — a control is one control. */
const COMPONENT_PLATFORM_LABEL: Record<ComponentPlatform, string> = {
  web: `Web`,
  desktop: `Desktop`,
  ios: `iOS`,
  android: `Android`,
}

/** How tall a fitted shot renders; placeholders match it so the rail stays level. */
const RAIL_HEIGHT = 520

const STATE_TEXT: Record<Shot[`state`], string> = {
  ok: `captured`,
  missing: `declared, not captured yet`,
  manual: `awaiting a manual capture (--manual <view-id>)`,
  [`n/a`]: `not declared for this platform`,
}

function formatBytes(bytes: number | undefined): string | undefined {
  if (bytes === undefined) return undefined
  if (bytes < 1024) return `${bytes} B`
  return `${Math.round(bytes / 1024)} KB`
}

function dotClass(state: Shot[`state`]): string {
  if (state === `ok`) return `ok`
  if (state === `manual`) return `manual`
  return state === `missing` ? `missing` : `na`
}

function placeholderWidth(platform: Platform): number {
  const frame = PLATFORM_FRAME[platform]
  const width = Math.round((RAIL_HEIGHT * frame.w) / frame.h)
  return Math.min(780, Math.max(200, width))
}

function renderFigure(entry: ViewEntry, shot: Shot): string {
  const label = escapeHtml(PLATFORM_LABEL[shot.platform])
  const dims = shot.w !== undefined && shot.h !== undefined ? `${shot.w}×${shot.h}` : undefined
  const size = formatBytes(shot.bytes)
  const meta = [dims, size].filter((part) => part !== undefined).join(` · `)
  const caption = `<figcaption><span class="platform">${label}</span><span class="dims">${escapeHtml(meta)}</span></figcaption>`

  if (shot.state === `ok`) {
    const alt = escapeHtml(`${entry.view.title} on ${PLATFORM_LABEL[shot.platform]}`)
    const dimAttrs =
      shot.w !== undefined && shot.h !== undefined ? ` width="${shot.w}" height="${shot.h}"` : ``
    return `<figure class="shot" data-platform="${shot.platform}" data-state="ok">${caption}<img src="${escapeHtml(shot.url)}"${dimAttrs} alt="${alt}" loading="lazy" decoding="async"></figure>`
  }

  const why = shot.note !== undefined ? `<p class="why">${escapeHtml(shot.note)}</p>` : ``
  const width = placeholderWidth(shot.platform)
  return [
    `<figure class="shot" data-platform="${shot.platform}" data-state="${shot.state}">`,
    caption,
    `<div class="placeholder" style="width:${width}px;height:${RAIL_HEIGHT}px">`,
    `<span class="state">${escapeHtml(STATE_TEXT[shot.state])}</span>`,
    why,
    `</div>`,
    `</figure>`,
  ].join(``)
}

/**
 * Platforms the view does not claim get NO card. A mobile-only surface used to
 * sit next to three full-height "not declared for this platform" boxes, which
 * made every such view read as three quarters broken. The reasons still matter,
 * so they collapse into one line under the rail instead of dominating it.
 */
function renderNotApplicable(shots: Shot[]): string {
  const na = shots.filter((shot) => shot.state === `n/a`)
  if (na.length === 0) return ``
  const labels = na.map((shot) => PLATFORM_LABEL[shot.platform]).join(`, `)
  const reasons = na
    .filter((shot) => shot.note !== undefined)
    .map(
      (shot) =>
        `<li><b>${escapeHtml(PLATFORM_LABEL[shot.platform])}</b> ${escapeHtml(shot.note!)}</li>`
    )
    .join(``)
  const summaryText = `Not on ${escapeHtml(labels)}`
  if (reasons.length === 0) return `<p class="na-note">${summaryText}</p>`
  return `<details class="na-note"><summary>${summaryText}</summary><ul>${reasons}</ul></details>`
}

function renderView(entry: ViewEntry, groupLabel: string): string {
  const rail = entry.shots
    .filter((shot) => shot.state !== `n/a`)
    .map((shot) => renderFigure(entry, shot))
    .join(``)
  return [
    `<section class="view" data-mode="views" data-view="${escapeHtml(entry.view.id)}" id="view-${escapeHtml(entry.view.id)}">`,
    `<p class="meta-note">${escapeHtml(groupLabel)}</p>`,
    `<h2>${escapeHtml(entry.view.title)}</h2>`,
    `<div><code class="view-id">${escapeHtml(entry.view.id)}</code></div>`,
    `<p class="blurb">${escapeHtml(entry.view.blurb)}</p>`,
    `<div class="rail">${rail}</div>`,
    renderNotApplicable(entry.shots),
    `</section>`,
  ].join(``)
}

function renderNavLink(entry: ViewEntry): string {
  const dots = entry.shots
    .map(
      (shot) =>
        `<span class="dot ${dotClass(shot.state)}" title="${escapeHtml(`${PLATFORM_LABEL[shot.platform]}: ${STATE_TEXT[shot.state]}`)}"></span>`
    )
    .join(``)
  const search = escapeHtml(
    `${entry.view.id} ${entry.view.title} ${entry.view.group} ${entry.view.blurb}`.toLowerCase()
  )
  return [
    `<a class="nav-link" href="#${escapeHtml(entry.view.id)}" data-view="${escapeHtml(entry.view.id)}"`,
    ` data-title="${escapeHtml(entry.view.title)}" data-search="${search}">`,
    `<span class="label">${escapeHtml(entry.view.title)}</span>`,
    `<span class="dots">${dots}</span>`,
    `</a>`,
  ].join(``)
}

/* ------------------------------------------------------------- components */

/** `n/a` gets the same hollow dot a not-declared shot does; `leftover` its own. */
function componentDotClass(state: ComponentStatus[`state`]): string {
  if (state === `ok`) return `ok`
  return state === `leftover` ? `leftover` : `na`
}

function fileBasename(file: string): string {
  const at = file.lastIndexOf(`/`)
  return at < 0 ? file : file.slice(at + 1)
}

function renderStatus(entry: PageEntry): string {
  // A placeholder has no table at all: four `unknown` rows would read as a
  // claim about four platforms nobody has looked at yet.
  if (entry.status === undefined) return ``
  const table = entry.status
  const rows = COMPONENT_PLATFORMS.map((platform) => {
    const status = table[platform]
    const dot = `<span class="dot ${componentDotClass(status.state)}"></span>`
    const symbol =
      status.symbol === undefined
        ? `<span class="path">${escapeHtml(status.state)}</span>`
        : `<code>${escapeHtml(status.symbol)}</code>`
    const file =
      status.file === undefined ? `` : `<span class="path">${escapeHtml(status.file)}</span>`
    const note = status.note === undefined ? `` : `<span class="note">${escapeHtml(status.note)}</span>`
    return [
      `<tr class="${componentDotClass(status.state)}">`,
      `<th scope="row">${dot} ${escapeHtml(COMPONENT_PLATFORM_LABEL[platform])}</th>`,
      `<td>${symbol}${file}${note}</td>`,
      `</tr>`,
    ].join(``)
  }).join(``)
  return `<table class="cmp-status"><tbody>${rows}</tbody></table>`
}

/**
 * The web call sites that still draw this semantic by hand (EXP-941). The
 * status table says whether a platform HAS the control; this says where the
 * product ignores the one it has, which is the only half a reference page
 * cannot infer from the code it points at.
 */
function renderLeftovers(entry: PageEntry): string {
  const rows = entry.leftovers ?? []
  if (rows.length === 0) return ``
  const items = rows
    .map(
      (row) =>
        `<li><code>${escapeHtml(row.file)}</code><span class="note">${escapeHtml(row.note)}</span></li>`
    )
    .join(``)
  return `<div class="leftovers"><div class="leftovers-head">Still drawn by hand</div><ul>${items}</ul></div>`
}

/**
 * The demo body. Both arms land in the SAME `.cmp-demo` canvas: that wrapper
 * paints the app's gradient ground, which an island never carries itself (a
 * fixed `bg-app-gradient` layer inside a shadow tree is wrong — see
 * `@exp/ui/island`).
 */
function renderDemo(entry: PageEntry): string {
  return isPageIsland(entry) ? renderIsland(entry.island()) : (entry.render?.() ?? ``)
}

function renderComponentSection(entry: PageEntry, section: PageSection): string {
  const owner = entry.owner === undefined ? `` : ` · ${entry.owner}`
  return [
    `<section class="view component" data-mode="${section.section.id}" data-view="${escapeHtml(entry.id)}" id="view-${escapeHtml(entry.id)}">`,
    `<p class="meta-note">${escapeHtml(`${section.section.title} · ${entry.kind}${owner}`)}</p>`,
    `<h2>${escapeHtml(entry.title)}</h2>`,
    `<div><code class="view-id">${escapeHtml(entry.id)}</code></div>`,
    `<p class="blurb">${escapeHtml(entry.blurb)}</p>`,
    `<div class="cmp-demo">${renderDemo(entry)}</div>`,
    renderStatus(entry),
    renderLeftovers(entry),
    `</section>`,
  ].join(``)
}

function renderComponentNavLink(entry: PageEntry, section: PageSection): string {
  const table = entry.status
  // No table, no dots: a placeholder makes no claim about any platform.
  const platformDots =
    table === undefined
      ? ``
      : COMPONENT_PLATFORMS.map((platform) => {
          const status = table[platform]
          const title = `${COMPONENT_PLATFORM_LABEL[platform]}: ${status.state}`
          return `<span class="dot ${componentDotClass(status.state)}" title="${escapeHtml(title)}"></span>`
        }).join(``)
  // One EXTRA yellow dot when the web app still draws this by hand somewhere:
  // four platforms agreeing means nothing if the call sites ignore them.
  const leftovers = entry.leftovers ?? []
  const handDot =
    leftovers.length === 0
      ? ``
      : `<span class="dot leftover" title="${escapeHtml(`${leftovers.length} web call site${leftovers.length === 1 ? `` : `s`} still drawn by hand`)}"></span>`
  const dots = `${platformDots}${handDot}`
  const searchable = [
    entry.id,
    entry.title,
    section.section.title,
    entry.kind,
    entry.owner,
    entry.blurb,
    ...(table === undefined
      ? []
      : COMPONENT_PLATFORMS.flatMap((platform) => {
          const status = table[platform]
          return [status.symbol, status.file === undefined ? undefined : fileBasename(status.file)]
        })),
  ]
    .filter((part) => part !== undefined)
    .join(` `)
  return [
    `<a class="nav-link" href="#${escapeHtml(entry.id)}" data-view="${escapeHtml(entry.id)}"`,
    ` data-title="${escapeHtml(entry.title)}" data-search="${escapeHtml(searchable.toLowerCase())}">`,
    `<span class="label">${escapeHtml(entry.title)}</span>`,
    `<span class="dots">${dots}</span>`,
    `</a>`,
  ].join(``)
}

/**
 * The summary line: what is captured, then how big each of the four sections
 * is. Views counts the photographed catalog PLUS whatever registered entries
 * the section holds, because both are things a reader can open there.
 */
function summary(data: GalleryData, page: PageSection[]): string {
  const total = data.counts.ok + data.counts.missing
  const sizes = page.map((section) => {
    const count =
      section.section.id === `views`
        ? data.views.length + section.entries.length
        : section.entries.length
    return count > 0 ? `${count} ${sectionLabel(section.section)}` : undefined
  })
  const parts = [
    `${data.counts.ok}/${total} captured`,
    data.counts.na > 0 ? `${data.counts.na} n/a` : undefined,
    ...sizes,
    data.undeclared.length > 0 ? `${data.undeclared.length} undeclared` : undefined,
    data.indexPresent ? undefined : `no index.json`,
  ].filter((part) => part !== undefined)
  return parts.join(` · `)
}

/** Serialise for a `<script type="application/json">` block. */
function inlineJson(data: GalleryData, page: PageSection[]): string {
  const { groups, views, undeclared, indexPresent, counts } = data
  // The client routes WITHIN a section, so every id it knows carries one.
  const modes: Record<string, string> = {}
  for (const entry of data.views) modes[entry.view.id] = `views`
  for (const section of page) {
    for (const entry of section.entries) modes[entry.id] = section.section.id
  }
  return JSON.stringify({
    groups,
    views,
    undeclared,
    indexPresent,
    counts,
    modes,
    // Only what the client routes on, in NAV order: `render` is a function and
    // the status table is already in the document.
    components: page.flatMap((section) =>
      section.entries.map((entry) => ({
        id: entry.id,
        title: entry.title,
        mode: section.section.id,
      }))
    ),
  }).replace(/</g, `\\u003c`)
}

/** A kind's nav section id — `Inputs & pickers` → `inputs-pickers`. */
function kindSlug(kind: ComponentKind): string {
  return kind
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-|-$/g, ``)
}

/** The bands of ONE section's nav, `kind` by `kind` in `BAND_ORDER`. */
function renderSectionBands(section: PageSection): string {
  return section.bands
    .map((band) =>
      [
        `<div class="group-section" data-group="${escapeHtml(kindSlug(band.kind))}">`,
        `<div class="group-label">${escapeHtml(band.kind)}</div>`,
        band.entries.map((entry) => renderComponentNavLink(entry, section)).join(``),
        `</div>`,
      ].join(``)
    )
    .join(``)
}

/**
 * The four-segment capsule above the filter, in `sections.json` order, with
 * the contract's title and blurb as the tooltip. No count rides a segment:
 * four labels and four numbers do not fit a sidebar without ellipsising the
 * words, and the summary line directly above already prints every section's
 * size ("10 style · 72 general components · …").
 */
function renderSectionBar(page: PageSection[]): string {
  const segments = page
    .map((section) => {
      const { id, title, blurb } = section.section
      return [
        `<button class="mode-btn" type="button" data-mode="${escapeHtml(id)}"`,
        ` aria-pressed="${id === OPENING_SECTION ? `true` : `false`}"`,
        ` title="${escapeHtml(`${title} — ${blurb}`)}">`,
        `<span class="label">${escapeHtml(sectionShortLabel(section.section))}</span>`,
        `</button>`,
      ].join(``)
    })
    .join(``)
  return `<div class="mode-bar" role="group" aria-label="Section">${segments}</div>`
}

/**
 * The section a script-less page shows. The photographed catalog: it is the
 * only section whose entries a reader cannot reproduce from the code, and the
 * one the page has always opened in.
 */
const OPENING_SECTION = `views`

export function renderHtml(
  data: GalleryData,
  components: readonly ComponentSpec[] = COMPONENTS,
  uiCss = ``,
  entries: readonly StyleguideEntry[] = ENTRIES
): string {
  // EXP-1019: FOUR sections, in the contract's order (`sections/sections.json`
  // — the same index the IDE styleguide reads). Style is the values, General
  // the control set, Special the compositions built on top of it, Views the
  // photographed screens. Every existing spec lands in one of the first three
  // (`sectionOfSpec`) and every registered entry sits where the contract puts
  // it.
  const page = buildPage(components, entries)

  const viewNav = data.groups
    .map((section) =>
      [
        `<div class="group-section" data-group="${escapeHtml(section.group.id)}">`,
        `<div class="group-label" title="${escapeHtml(section.group.blurb)}">${escapeHtml(section.group.label)}</div>`,
        section.views.map(renderNavLink).join(``),
        `</div>`,
      ].join(``)
    )
    .join(``)

  // One `.mode-section` per section, banded by `kind`. The three synthetic
  // sections have no catalog entry, no shots and `--check` never sees them;
  // Views prepends the photographed catalog's own groups to its bands.
  const nav = page
    .map((section) =>
      [
        `<div class="mode-section" data-mode="${escapeHtml(section.section.id)}">`,
        `<p class="section-blurb">${escapeHtml(section.section.blurb)}</p>`,
        section.section.id === `views` ? viewNav : ``,
        renderSectionBands(section),
        `</div>`,
      ].join(``)
    )
    .join(``)

  const sections = data.groups
    .map((section) =>
      section.views.map((entry) => renderView(entry, section.group.label)).join(``)
    )
    .join(``)

  const componentSections = page
    .flatMap((section) => section.entries.map((entry) => renderComponentSection(entry, section)))
    .join(``)

  const empty =
    data.views.length === 0
      ? `<p class="blurb views-only">The view catalog is empty.</p>`
      : data.counts.ok === 0
        ? `<p class="blurb views-only">Nothing captured yet — run <code>bun run shots</code> to fill the store.</p>`
        : ``

  return [
    `<!doctype html>`,
    `<html lang="en" class="dark">`,
    `<head>`,
    `<meta charset="utf-8">`,
    `<meta name="viewport" content="width=device-width, initial-scale=1">`,
    `<meta name="robots" content="noindex">`,
    `<title>Exponential styleguide</title>`,
    `<style>${styles}</style>`,
    // The islands' stylesheet rides ONE inert <template>; the script at the
    // end of <body> turns it into a single adopted CSSStyleSheet for every
    // shadow root on the page.
    uiCss === `` ? `` : renderIslandCssTemplate(uiCss),
    `</head>`,
    // The section the page opens in; the client re-reads it from the hash and
    // from localStorage, but a `file://` page with no script still renders one.
    `<body data-mode="${OPENING_SECTION}">`,
    `<div class="layout">`,
    `<aside class="sidebar">`,
    `<div class="brand"><h1>Exponential styleguide</h1><p>${escapeHtml(summary(data, page))}</p></div>`,
    `<div class="filter-wrap">`,
    renderSectionBar(page),
    `<input id="filter" class="filter" type="search" placeholder="Filter this section  ( / )" autocomplete="off" spellcheck="false">`,
    `</div>`,
    `<nav>${nav}<div class="nav-empty hidden">Nothing matches.</div></nav>`,
    `</aside>`,
    `<main class="main">`,
    `<div class="toolbar">`,
    `<button id="toggle-size" class="btn views-only" type="button" aria-pressed="false">Fit to height</button>`,
    `<span class="meta-note views-only">click a shot for 1:1</span>`,
    `<span class="meta-note">j / k moves · / filters · 1 / 2 / 3 / 4 switches section</span>`,
    `<span class="spacer"></span>`,
    `<span class="meta-note">${escapeHtml(summary(data, page))}</span>`,
    `</div>`,
    empty,
    sections,
    componentSections,
    `</main>`,
    `</div>`,
    `<dialog class="lightbox"><img alt="Full size screenshot"></dialog>`,
    `<script type="application/json" id="gallery-data">${inlineJson(data, page)}</script>`,
    `<script>${client}</script>`,
    uiCss === `` ? `` : `<script>${ISLAND_CLIENT_SCRIPT}</script>`,
    `</body>`,
    `</html>`,
    ``,
  ].join(`\n`)
}

/** Exported for tooling that wants the same platform labels as the page. */
export { PLATFORM_LABEL }
