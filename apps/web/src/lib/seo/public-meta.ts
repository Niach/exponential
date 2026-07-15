// Server-only SEO helpers for public feedback boards. The web app renders
// client-side (`defaultSsr: false`), so link unfurlers never see route-specific
// meta. server-bun.ts rewrites the buffered HTML for the two public routes
// below, injecting OG/Twitter meta resolved here. NOTHING in this module ever
// runs on the client (it opens a DB pool) — keep it out of component imports.

import { and, eq, isNull } from "drizzle-orm"
import { issues, projects, workspaces } from "@/db/schema"

async function getDb() {
  const { db } = await import(`@/db/connection`)
  return db
}

export type PublicPathMatch = {
  workspaceSlug: string
  projectSlug: string
  issueIdentifier?: string
}

export type PublicPageMeta = {
  title: string
  description: string
  // Path only (leading slash, no origin) — the caller prefixes the origin.
  url: string
  // Path only — the caller prefixes the origin to build an absolute og:image.
  imagePath: string
}

const OG_IMAGE_PATH = `/og-card.png`
const SITE_NAME = `Exponential`
const DESCRIPTION_MAX = 200

// Parses ONLY the two public routes:
//   /t/:ws/projects/:proj
//   /t/:ws/projects/:proj/issues/:identifier
// The legacy /w/ prefix is accepted too (the server 301s it to /t/, but the
// matcher stays dual so meta survives any path that slips through). Anything
// else (including trailing slashes or extra segments) returns null so the
// rewriter leaves the response untouched.
export function matchPublicPath(pathname: string): PublicPathMatch | null {
  const decode = (value: string): string | null => {
    try {
      return decodeURIComponent(value)
    } catch {
      return null
    }
  }

  const issueMatch =
    /^\/[tw]\/([^/]+)\/projects\/([^/]+)\/issues\/([^/]+)$/.exec(pathname)
  if (issueMatch) {
    const workspaceSlug = decode(issueMatch[1])
    const projectSlug = decode(issueMatch[2])
    const issueIdentifier = decode(issueMatch[3])
    if (!workspaceSlug || !projectSlug || !issueIdentifier) return null
    return { workspaceSlug, projectSlug, issueIdentifier }
  }

  const boardMatch = /^\/[tw]\/([^/]+)\/projects\/([^/]+)$/.exec(pathname)
  if (boardMatch) {
    const workspaceSlug = decode(boardMatch[1])
    const projectSlug = decode(boardMatch[2])
    if (!workspaceSlug || !projectSlug) return null
    return { workspaceSlug, projectSlug }
  }

  return null
}

// Hand-rolled markdown→plaintext stripper for description previews. Not a full
// parser — it drops the structural markers of the supported GFM subset and
// leaves @email mentions verbatim (they round-trip as plain text). Whitespace
// is collapsed to single spaces so the result slots into a meta tag.
export function stripMarkdownToPlainText(md: string | null | undefined): string {
  if (!md) return ``
  let text = md
  // Fenced code blocks: drop the ``` fence lines, keep the inner code text.
  text = text.replace(/```[^\n]*\n?/g, ` `)
  // Inline images ![alt](url) — drop entirely (before links, whose syntax nests).
  text = text.replace(/!\[[^\]]*\]\([^)]*\)/g, ` `)
  // Links [text](url) -> text.
  text = text.replace(/\[([^\]]*)\]\([^)]*\)/g, `$1`)
  // Inline code `code` -> code.
  text = text.replace(/`([^`]*)`/g, `$1`)
  // Headings: leading #'s.
  text = text.replace(/^\s{0,3}#{1,6}\s+/gm, ``)
  // Blockquotes.
  text = text.replace(/^\s{0,3}>\s?/gm, ``)
  // Task-list markers: `- [ ]` / `- [x]`.
  text = text.replace(/^\s*[-*+]\s+\[[ xX]\]\s+/gm, ``)
  // List bullets (unordered + ordered).
  text = text.replace(/^\s*[-*+]\s+/gm, ``)
  text = text.replace(/^\s*\d+\.\s+/gm, ``)
  // Emphasis / strikethrough markers.
  text = text.replace(/[*_~]/g, ``)
  // Collapse all whitespace.
  return text.replace(/\s+/g, ` `).trim()
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  const clipped = text.slice(0, max)
  const lastSpace = clipped.lastIndexOf(` `)
  const base = lastSpace > max * 0.6 ? clipped.slice(0, lastSpace) : clipped
  return `${base.trimEnd()}…`
}

type CacheEntry = { meta: PublicPageMeta | null; expires: number }
const META_TTL_MS = 60_000
// Size cap: every unique unauthenticated /t/*/projects/* path inserts an
// entry, so an unbounded map is a crawler-driven memory leak.
const META_CACHE_MAX_ENTRIES = 500
const metaCache = new Map<string, CacheEntry>()

// Drop expired entries; if still over the cap, evict oldest-inserted first
// (Map preserves insertion order).
function pruneMetaCache(now: number): void {
  if (metaCache.size < META_CACHE_MAX_ENTRIES) return
  for (const [key, entry] of metaCache) {
    if (entry.expires <= now) metaCache.delete(key)
  }
  for (const key of metaCache.keys()) {
    if (metaCache.size < META_CACHE_MAX_ENTRIES) break
    metaCache.delete(key)
  }
}

// Project mutations (trash, retype, privacy toggles) must not keep serving
// stale OG meta for the TTL window.
export function invalidatePublicMetaCache(): void {
  metaCache.clear()
}

function cacheKey(match: PublicPathMatch): string {
  return `${match.workspaceSlug}\u0000${match.projectSlug}\u0000${
    match.issueIdentifier ?? ``
  }`
}

// Resolves the public meta for a matched path via one (or two) drizzle reads.
// Returns null unless the project is an unarchived feedback board (and, for an
// issue path, the issue exists and is unarchived). Results — including null —
// are cached for 60s keyed by the path so a burst of unfurler hits costs one
// query.
export async function resolvePublicPageMeta(
  match: PublicPathMatch
): Promise<PublicPageMeta | null> {
  const key = cacheKey(match)
  const cached = metaCache.get(key)
  const now = Date.now()
  if (cached && cached.expires > now) return cached.meta

  const meta = await resolveUncached(match)
  pruneMetaCache(now)
  metaCache.set(key, { meta, expires: now + META_TTL_MS })
  return meta
}

async function resolveUncached(
  match: PublicPathMatch
): Promise<PublicPageMeta | null> {
  const db = await getDb()
  const [project] = await db
    .select({
      id: projects.id,
      name: projects.name,
      type: projects.type,
      archivedAt: projects.archivedAt,
      deletedAt: projects.deletedAt,
      workspaceName: workspaces.name,
    })
    .from(projects)
    .innerJoin(workspaces, eq(projects.workspaceId, workspaces.id))
    .where(
      and(
        eq(workspaces.slug, match.workspaceSlug),
        eq(projects.slug, match.projectSlug)
      )
    )
    .limit(1)

  if (
    !project ||
    project.type !== `feedback` ||
    project.archivedAt !== null ||
    // A trashed feedback board must not get OG meta or the noindex→index flip
    // during its retention window.
    project.deletedAt !== null
  ) {
    return null
  }

  const boardUrl = `/t/${match.workspaceSlug}/projects/${match.projectSlug}`

  if (!match.issueIdentifier) {
    return {
      title: `${project.name} · Feedback board`,
      description: `Public feedback board for ${project.name} on ${project.workspaceName}.`,
      url: boardUrl,
      imagePath: OG_IMAGE_PATH,
    }
  }

  const [issue] = await db
    .select({
      identifier: issues.identifier,
      title: issues.title,
      description: issues.description,
      archivedAt: issues.archivedAt,
    })
    .from(issues)
    .where(
      and(
        eq(issues.projectId, project.id),
        eq(issues.identifier, match.issueIdentifier),
        isNull(issues.archivedAt)
      )
    )
    .limit(1)

  if (!issue) return null

  const stripped = stripMarkdownToPlainText(issue.description)
  const description = stripped
    ? truncate(stripped, DESCRIPTION_MAX)
    : `Issue on the ${project.name} feedback board`

  return {
    title: `${issue.identifier}: ${issue.title} · ${project.name}`,
    description,
    url: `${boardUrl}/issues/${issue.identifier}`,
    imagePath: OG_IMAGE_PATH,
  }
}

function htmlEscape(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
    .replace(/'/g, `&#39;`)
}

// Rewrites a buffered HTML document: injects the resolved OG/Twitter/canonical
// block before </head>. The app-wide `noindex` from __root.tsx is PRESERVED —
// this block exists for link unfurlers (which ignore robots meta), not for
// search engines: public boards carry user-submitted feedback and must stay out
// of the index. Every interpolated value is HTML-escaped, so hostile issue
// titles/descriptions (e.g. `<script>`) can never break out of the attribute.
export function injectMeta(
  html: string,
  meta: PublicPageMeta,
  origin: string
): string {
  const cleanOrigin = origin.replace(/\/$/, ``)
  const absoluteUrl = `${cleanOrigin}${meta.url}`
  const absoluteImage = `${cleanOrigin}${meta.imagePath}`

  const tag = (attrs: string) => `    ${attrs}\n`
  const block =
    `\n` +
    tag(`<meta property="og:type" content="website" />`) +
    tag(`<meta property="og:site_name" content="${htmlEscape(SITE_NAME)}" />`) +
    tag(`<meta property="og:url" content="${htmlEscape(absoluteUrl)}" />`) +
    tag(`<meta property="og:title" content="${htmlEscape(meta.title)}" />`) +
    tag(
      `<meta property="og:description" content="${htmlEscape(
        meta.description
      )}" />`
    ) +
    tag(`<meta property="og:image" content="${htmlEscape(absoluteImage)}" />`) +
    tag(`<meta name="twitter:card" content="summary_large_image" />`) +
    tag(`<meta name="twitter:title" content="${htmlEscape(meta.title)}" />`) +
    tag(
      `<meta name="twitter:description" content="${htmlEscape(
        meta.description
      )}" />`
    ) +
    tag(`<meta name="twitter:image" content="${htmlEscape(absoluteImage)}" />`) +
    tag(`<link rel="canonical" href="${htmlEscape(absoluteUrl)}" />`)

  // Re-assert noindex rather than trusting the shell: a public board is the one
  // page a crawler is most likely to reach (it's linked from the wild), so if
  // the shell ever ships without a robots tag, add one here.
  let injectBlock = block
  const robotsRe = /<meta[^>]*name=["']robots["'][^>]*>/i
  if (!robotsRe.test(html)) {
    injectBlock += tag(`<meta name="robots" content="noindex" />`)
  }

  // Replacer FUNCTIONs, not strings: the block embeds escaped titles where a
  // `$'` (escaped to `$&#39;`) would otherwise be read as a replace() pattern
  // and splice document content into the attribute.
  const headClose = /<\/head>/i
  if (headClose.test(html)) {
    return html.replace(headClose, () => `${injectBlock}</head>`)
  }
  // React 19's streamed production shell emits NO `</head>` at all (the
  // browser parser auto-closes head at the first flow element), so falling
  // back to the opening tag is the path that actually runs in prod — meta
  // placement anywhere inside head is valid for crawlers and unfurlers.
  const headOpen = /<head(?:\s[^>]*)?>/i
  if (!headOpen.test(html)) return html
  return html.replace(headOpen, (m) => `${m}${injectBlock}`)
}
