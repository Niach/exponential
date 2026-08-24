/**
 * The renderer: `GalleryData` in, ONE self-contained HTML document out. Pure —
 * no disk, no network, no clock — so `build.ts` and `serve.ts` produce the same
 * bytes for the same store, and the page works over `file://`.
 */

import { PLATFORM_FRAME } from "@exp/view-catalog"
import type { Platform } from "@exp/view-catalog"

import { client } from "./client.ts"
import { styles } from "./styles.ts"
import type { GalleryData, Shot, ViewEntry } from "./store.ts"

const PLATFORM_LABEL: Record<Platform, string> = {
  web: `Web`,
  [`web-mobile`]: `Web mobile`,
  desktop: `Desktop`,
  ios: `iOS`,
  android: `Android`,
  ipad: `iPad`,
}

/** How tall a fitted shot renders; placeholders match it so the rail stays level. */
const RAIL_HEIGHT = 520

const STATE_TEXT: Record<Shot[`state`], string> = {
  ok: `captured`,
  missing: `declared, not captured yet`,
  [`n/a`]: `not declared for this platform`,
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
}

function formatBytes(bytes: number | undefined): string | undefined {
  if (bytes === undefined) return undefined
  if (bytes < 1024) return `${bytes} B`
  return `${Math.round(bytes / 1024)} KB`
}

function dotClass(state: Shot[`state`]): string {
  if (state === `ok`) return `ok`
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

function renderView(entry: ViewEntry, groupLabel: string): string {
  const rail = entry.shots.map((shot) => renderFigure(entry, shot)).join(``)
  return [
    `<section class="view" data-view="${escapeHtml(entry.view.id)}" id="view-${escapeHtml(entry.view.id)}">`,
    `<p class="meta-note">${escapeHtml(groupLabel)}</p>`,
    `<h2>${escapeHtml(entry.view.title)}</h2>`,
    `<div><code class="view-id">${escapeHtml(entry.view.id)}</code></div>`,
    `<p class="blurb">${escapeHtml(entry.view.blurb)}</p>`,
    `<div class="rail">${rail}</div>`,
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

function summary(data: GalleryData): string {
  const total = data.counts.ok + data.counts.missing
  const parts = [
    `${data.views.length} views`,
    `${data.counts.ok}/${total} captured`,
    data.counts.na > 0 ? `${data.counts.na} n/a` : undefined,
    data.undeclared.length > 0 ? `${data.undeclared.length} undeclared` : undefined,
    data.indexPresent ? undefined : `no index.json`,
  ].filter((part) => part !== undefined)
  return parts.join(` · `)
}

/** Serialise for a `<script type="application/json">` block. */
function inlineJson(data: GalleryData): string {
  const { groups, views, undeclared, indexPresent, counts } = data
  return JSON.stringify({ groups, views, undeclared, indexPresent, counts }).replace(
    /</g,
    `\\u003c`
  )
}

export function renderHtml(data: GalleryData): string {
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

  const sections = data.groups
    .map((section) =>
      section.views.map((entry) => renderView(entry, section.group.label)).join(``)
    )
    .join(``)

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
    `<div class="brand"><h1>Exponential views</h1><p>${escapeHtml(summary(data))}</p></div>`,
    `<div class="filter-wrap"><input id="filter" class="filter" type="search" placeholder="Filter views  ( / )" autocomplete="off" spellcheck="false"></div>`,
    `<nav>${nav}<div class="nav-empty hidden">No view matches.</div></nav>`,
    `</aside>`,
    `<main class="main">`,
    `<div class="toolbar">`,
    `<button id="toggle-size" class="btn" type="button" aria-pressed="false">Fit to height</button>`,
    `<span class="meta-note">j / k moves · / filters · click a shot for 1:1</span>`,
    `<span class="spacer"></span>`,
    `<span class="meta-note">${escapeHtml(summary(data))}</span>`,
    `</div>`,
    empty,
    sections,
    `</main>`,
    `</div>`,
    `<dialog class="lightbox"><img alt="Full size screenshot"></dialog>`,
    `<script type="application/json" id="gallery-data">${inlineJson(data)}</script>`,
    `<script>${client}</script>`,
    `</body>`,
    `</html>`,
    ``,
  ].join(`\n`)
}

/** Exported for tooling that wants the same platform labels as the page. */
export { PLATFORM_LABEL }
