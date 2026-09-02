/**
 * The renderer: `GalleryData` in, ONE self-contained HTML document out. Pure —
 * no disk, no network, no clock — so `build.ts` and `serve.ts` produce the same
 * bytes for the same store, and the page works over `file://`.
 */

import { PLATFORM_FRAME } from "@exp/view-catalog"
import type { Platform } from "@exp/view-catalog"

import { client } from "./client.ts"
import { COMPONENTS, COMPONENTS_GROUP, COMPONENT_PLATFORMS } from "./components.ts"
import type { ComponentPlatform, ComponentSpec, ComponentStatus } from "./components.ts"
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
    `<section class="view" data-view="${escapeHtml(entry.view.id)}" id="view-${escapeHtml(entry.view.id)}">`,
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

function renderStatus(spec: ComponentSpec): string {
  const rows = COMPONENT_PLATFORMS.map((platform) => {
    const status = spec.status[platform]
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

function renderComponentSection(spec: ComponentSpec): string {
  return [
    `<section class="view component" data-view="${escapeHtml(spec.id)}" id="view-${escapeHtml(spec.id)}">`,
    `<p class="meta-note">${escapeHtml(`${COMPONENTS_GROUP.label} · ${spec.kind}`)}</p>`,
    `<h2>${escapeHtml(spec.title)}</h2>`,
    `<div><code class="view-id">${escapeHtml(spec.id)}</code></div>`,
    `<p class="blurb">${escapeHtml(spec.blurb)}</p>`,
    `<div class="cmp-demo">${spec.render()}</div>`,
    renderStatus(spec),
    `</section>`,
  ].join(``)
}

function renderComponentNavLink(spec: ComponentSpec): string {
  const dots = COMPONENT_PLATFORMS.map((platform) => {
    const status = spec.status[platform]
    const title = `${COMPONENT_PLATFORM_LABEL[platform]}: ${status.state}`
    return `<span class="dot ${componentDotClass(status.state)}" title="${escapeHtml(title)}"></span>`
  }).join(``)
  const searchable = [
    spec.id,
    spec.title,
    COMPONENTS_GROUP.label,
    spec.kind,
    spec.blurb,
    ...COMPONENT_PLATFORMS.flatMap((platform) => {
      const status = spec.status[platform]
      return [status.symbol, status.file === undefined ? undefined : fileBasename(status.file)]
    }),
  ]
    .filter((part) => part !== undefined)
    .join(` `)
  return [
    `<a class="nav-link" href="#${escapeHtml(spec.id)}" data-view="${escapeHtml(spec.id)}"`,
    ` data-title="${escapeHtml(spec.title)}" data-search="${escapeHtml(searchable.toLowerCase())}">`,
    `<span class="label">${escapeHtml(spec.title)}</span>`,
    `<span class="dots">${dots}</span>`,
    `</a>`,
  ].join(``)
}

function summary(data: GalleryData, components: readonly ComponentSpec[]): string {
  const total = data.counts.ok + data.counts.missing
  const parts = [
    `${data.views.length} views`,
    `${data.counts.ok}/${total} captured`,
    data.counts.na > 0 ? `${data.counts.na} n/a` : undefined,
    components.length > 0 ? `${components.length} components` : undefined,
    data.undeclared.length > 0 ? `${data.undeclared.length} undeclared` : undefined,
    data.indexPresent ? undefined : `no index.json`,
  ].filter((part) => part !== undefined)
  return parts.join(` · `)
}

/** Serialise for a `<script type="application/json">` block. */
function inlineJson(data: GalleryData, components: readonly ComponentSpec[]): string {
  const { groups, views, undeclared, indexPresent, counts } = data
  return JSON.stringify({
    groups,
    views,
    undeclared,
    indexPresent,
    counts,
    // Only what the client routes on: `render` is a function and the status
    // table is already in the document.
    components: components.map((spec) => ({ id: spec.id, title: spec.title })),
  }).replace(/</g, `\\u003c`)
}

export function renderHtml(data: GalleryData, components = COMPONENTS): string {
  const nav = data.groups
    .map((section) =>
      [
        `<div class="group-section" data-group="${escapeHtml(section.group.id)}">`,
        `<div class="group-label" title="${escapeHtml(section.group.blurb)}">${escapeHtml(section.group.label)}</div>`,
        section.views.map(renderNavLink).join(``),
        `</div>`,
      ].join(``)
    )
    .join(``)

  // The Components group is SYNTHETIC — no catalog entry, no shots, no
  // `--check`. It is appended after the photographed groups rather than
  // interleaved, because it answers a different question.
  const componentNav =
    components.length === 0
      ? ``
      : [
          `<div class="group-section" data-group="${escapeHtml(COMPONENTS_GROUP.id)}">`,
          `<div class="group-label" title="${escapeHtml(COMPONENTS_GROUP.blurb)}">${escapeHtml(COMPONENTS_GROUP.label)}</div>`,
          components.map(renderComponentNavLink).join(``),
          `</div>`,
        ].join(``)

  const sections = data.groups
    .map((section) =>
      section.views.map((entry) => renderView(entry, section.group.label)).join(``)
    )
    .join(``)

  const componentSections = components.map(renderComponentSection).join(``)

  const empty =
    data.views.length === 0
      ? `<p class="blurb">The view catalog is empty.</p>`
      : data.counts.ok === 0
        ? `<p class="blurb">Nothing captured yet — run <code>bun run shots</code> to fill the store.</p>`
        : ``

  return [
    `<!doctype html>`,
    `<html lang="en" class="dark">`,
    `<head>`,
    `<meta charset="utf-8">`,
    `<meta name="viewport" content="width=device-width, initial-scale=1">`,
    `<meta name="robots" content="noindex">`,
    `<title>Exponential views</title>`,
    `<style>${styles}</style>`,
    `</head>`,
    `<body>`,
    `<div class="layout">`,
    `<aside class="sidebar">`,
    `<div class="brand"><h1>Exponential views</h1><p>${escapeHtml(summary(data, components))}</p></div>`,
    `<div class="filter-wrap"><input id="filter" class="filter" type="search" placeholder="Filter views &amp; components  ( / )" autocomplete="off" spellcheck="false"></div>`,
    `<nav>${nav}${componentNav}<div class="nav-empty hidden">Nothing matches.</div></nav>`,
    `</aside>`,
    `<main class="main">`,
    `<div class="toolbar">`,
    `<button id="toggle-size" class="btn" type="button" aria-pressed="false">Fit to height</button>`,
    `<span class="meta-note">j / k moves · / filters · click a shot for 1:1</span>`,
    `<span class="spacer"></span>`,
    `<span class="meta-note">${escapeHtml(summary(data, components))}</span>`,
    `</div>`,
    empty,
    sections,
    componentSections,
    `</main>`,
    `</div>`,
    `<dialog class="lightbox"><img alt="Full size screenshot"></dialog>`,
    `<script type="application/json" id="gallery-data">${inlineJson(data, components)}</script>`,
    `<script>${client}</script>`,
    `</body>`,
    `</html>`,
    ``,
  ].join(`\n`)
}

/** Exported for tooling that wants the same platform labels as the page. */
export { PLATFORM_LABEL }
