/* Postbuild prerender + SEO injection. Runs under Bun after `vite build`:

     tsc --noEmit && vite build && bun run scripts/prerender.tsx

   For each page it (1) renderToString's the React component into the
   `<div id="root"></div>` marker of the built dist HTML (so crawlers and
   first paint get real markup, then the client hydrates), (2) strips any
   stray SEO tags from the head and injects the canonical/OG/Twitter/JSON-LD
   block from src/lib/seo.ts, and (3) emits dist/sitemap.xml with per-page
   lastmod from git plus dist/llms.txt (llmstxt.org) from the same page
   manifest. This is the single owner of SEO <head> markup — the source
   HTML heads carry only charset/viewport/title/fonts/icons. */

import { renderToString } from "react-dom/server"
import { execFileSync } from "node:child_process"
import { readFileSync, writeFileSync, existsSync } from "node:fs"
import { resolve, dirname } from "node:path"
import { fileURLToPath } from "node:url"
import type { ComponentType } from "react"

import {
  PAGES,
  SITE_ORIGIN,
  SITE_NAME,
  type PageSeo,
  type JsonLd,
} from "../src/lib/seo"
import { HomePage } from "../src/HomePage"
import { PricingPage } from "../src/PricingPage"
import { DownloadPage } from "../src/DownloadPage"
import { DOCS_NAV } from "../src/lib/docs-nav"
import { DocsPage } from "../src/DocsPage"
import { GettingStartedDocsPage } from "../src/GettingStartedDocsPage"
import { IssuesDocsPage } from "../src/IssuesDocsPage"
import { CodingDocsPage } from "../src/CodingDocsPage"
import { FeedbackDocsPage } from "../src/FeedbackDocsPage"
import { WidgetDocsPage } from "../src/WidgetDocsPage"
import { McpDocsPage } from "../src/McpDocsPage"
import { AppsDocsPage } from "../src/AppsDocsPage"
import { SelfHostDocsPage } from "../src/SelfHostDocsPage"
import { PrivacyPage } from "../src/PrivacyPage"
import { TermsPage } from "../src/TermsPage"
import { ImprintPage } from "../src/ImprintPage"
import { ContactPage } from "../src/ContactPage"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), `..`)
const DIST = resolve(ROOT, `dist`)
const MARKER = `<div id="root"></div>`

/* Path → page component, keyed by PageSeo.path. */
const COMPONENTS: Record<string, ComponentType> = {
  "/": HomePage,
  "/pricing/": PricingPage,
  "/download/": DownloadPage,
  "/docs/": DocsPage,
  "/docs/getting-started/": GettingStartedDocsPage,
  "/docs/issues/": IssuesDocsPage,
  "/docs/coding/": CodingDocsPage,
  "/docs/feedback/": FeedbackDocsPage,
  "/docs/widget/": WidgetDocsPage,
  "/docs/mcp/": McpDocsPage,
  "/docs/apps/": AppsDocsPage,
  "/docs/self-host/": SelfHostDocsPage,
  "/privacy/": PrivacyPage,
  "/terms/": TermsPage,
  "/imprint/": ImprintPage,
  "/contact/": ContactPage,
}

function escapeAttr(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
}

/* Remove any SEO tags that slipped into the source head, so this script is the
   sole owner of the injected block (idempotent across rebuilds). */
function stripExistingSeo(head: string): string {
  return head
    .replace(/[ \t]*<meta\s+name=["']description["'][^>]*>\s*/gi, ``)
    .replace(/[ \t]*<link\s+rel=["']canonical["'][^>]*>\s*/gi, ``)
    .replace(/[ \t]*<meta\s+property=["']og:[^"']*["'][^>]*>\s*/gi, ``)
    .replace(/[ \t]*<meta\s+name=["']twitter:[^"']*["'][^>]*>\s*/gi, ``)
    .replace(
      /[ \t]*<script\s+type=["']application\/ld\+json["'][^>]*>[\s\S]*?<\/script>\s*/gi,
      ``
    )
}

function metaBlock(page: PageSeo): string {
  const url = `${SITE_ORIGIN}${page.path}`
  const image = `${SITE_ORIGIN}${page.ogImage}`
  const t = escapeAttr(page.title)
  const d = escapeAttr(page.description)
  const lines: string[] = [
    `<meta name="description" content="${d}" />`,
    `<link rel="canonical" href="${escapeAttr(url)}" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:url" content="${escapeAttr(url)}" />`,
    `<meta property="og:title" content="${t}" />`,
    `<meta property="og:description" content="${d}" />`,
    `<meta property="og:image" content="${escapeAttr(image)}" />`,
    `<meta property="og:image:width" content="1200" />`,
    `<meta property="og:image:height" content="630" />`,
    `<meta property="og:image:alt" content="${t}" />`,
    `<meta property="og:site_name" content="${escapeAttr(SITE_NAME)}" />`,
    `<meta property="og:locale" content="en_US" />`,
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${t}" />`,
    `<meta name="twitter:description" content="${d}" />`,
    `<meta name="twitter:image" content="${escapeAttr(image)}" />`,
  ]
  if (page.jsonLd) {
    const blocks: JsonLd[] = Array.isArray(page.jsonLd)
      ? page.jsonLd
      : [page.jsonLd]
    for (const block of blocks) {
      /* Escape </script> so JSON-LD can never break out of the tag. */
      const json = JSON.stringify(block).replace(/<\//g, `<\\/`)
      lines.push(`<script type="application/ld+json">${json}</script>`)
    }
  }
  return lines.map((l) => `    ${l}`).join(`\n`)
}

function gitLastmod(sources: string[]): string {
  for (const src of sources) {
    try {
      const iso = execFileSync(
        `git`,
        [`log`, `-1`, `--format=%cI`, `--`, src],
        { cwd: ROOT, encoding: `utf8` }
      ).trim()
      if (iso) return iso
    } catch {
      /* fall through to next source / build-date fallback */
    }
  }
  return new Date().toISOString()
}

function prerenderPage(page: PageSeo): void {
  const htmlPath = resolve(DIST, page.htmlFile)
  if (!existsSync(htmlPath)) {
    throw new Error(
      `prerender: dist file missing for ${page.path}: ${htmlPath}`
    )
  }
  const Component = COMPONENTS[page.path]
  if (!Component) throw new Error(`prerender: no component for ${page.path}`)

  let html = readFileSync(htmlPath, `utf8`)
  if (!html.includes(MARKER)) {
    throw new Error(
      `prerender: marker ${MARKER} not found in ${page.htmlFile} — cannot inject SSR markup`
    )
  }

  const body = renderToString(<Component />)
  html = html.replace(MARKER, () => `<div id="root">${body}</div>`)

  const headClose = `</head>`
  if (!html.includes(headClose)) {
    throw new Error(`prerender: </head> not found in ${page.htmlFile}`)
  }
  const withStrippedHead = html.replace(
    /<head>([\s\S]*?)<\/head>/i,
    (_m, head) => {
      return `<head>${stripExistingSeo(head)}</head>`
    }
  )
  html = withStrippedHead.replace(
    headClose,
    () => `${metaBlock(page)}\n  ${headClose}`
  )

  writeFileSync(htmlPath, html)
  console.log(
    `prerendered ${page.path} → ${page.htmlFile} (${body.length} bytes body)`
  )
}

function writeSitemap(): void {
  const urls = PAGES.map((p) => {
    const loc = `${SITE_ORIGIN}${p.path}`
    const lastmod = gitLastmod(p.sources)
    return `  <url><loc>${loc}</loc><lastmod>${lastmod}</lastmod></url>`
  })
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls.join(`\n`)}
</urlset>
`
  writeFileSync(resolve(DIST, `sitemap.xml`), xml)
  console.log(`wrote dist/sitemap.xml (${PAGES.length} urls)`)
}

/* llms.txt (https://llmstxt.org) — a markdown site map for LLM crawlers,
   generated from the same PAGES manifest as the sitemap so it can't drift. */
function writeLlmsTxt(): void {
  const SECTIONS: { heading: string; paths: string[] }[] = [
    { heading: `Product`, paths: [`/`, `/pricing/`, `/download/`] },
    {
      heading: `Docs`,
      paths: [
        `/docs/`,
        `/docs/getting-started/`,
        `/docs/issues/`,
        `/docs/coding/`,
        `/docs/feedback/`,
        `/docs/widget/`,
        `/docs/mcp/`,
        `/docs/apps/`,
        `/docs/self-host/`,
      ],
    },
    {
      heading: `Company & legal`,
      paths: [`/contact/`, `/privacy/`, `/terms/`, `/imprint/`],
    },
  ]
  const byPath = new Map(PAGES.map((p) => [p.path, p]))
  const lines: string[] = [
    `# ${SITE_NAME}`,
    ``,
    `> A source-available issue tracker with a built-in coding IDE. Assign issues to AI agents that run locally in your terminal and open GitHub pull requests. Native apps for web, iOS, Android, macOS and Linux. Free cloud tier, or self-host it for free while your company has fewer than 10 people.`,
    ``,
    `The app itself lives at https://app.exponential.at; the source code is at https://github.com/Niach/exponential.`,
  ]
  for (const section of SECTIONS) {
    lines.push(``, `## ${section.heading}`, ``)
    for (const path of section.paths) {
      const page = byPath.get(path)
      if (!page) throw new Error(`llms.txt: unknown page path ${path}`)
      lines.push(
        `- [${page.title}](${SITE_ORIGIN}${page.path}): ${page.description}`
      )
    }
  }
  const covered = new Set(SECTIONS.flatMap((s) => s.paths))
  const missing = PAGES.filter((p) => !covered.has(p.path))
  if (missing.length) {
    throw new Error(
      `llms.txt: pages missing from a section: ${missing.map((p) => p.path).join(`, `)}`
    )
  }
  writeFileSync(resolve(DIST, `llms.txt`), `${lines.join(`\n`)}\n`)
  console.log(`wrote dist/llms.txt (${PAGES.length} pages)`)
}

/* Docs-nav parity: the sidebar/hub list (DOCS_NAV) and the prerender
   registry (PAGES) enumerate the docs pages independently — a page present
   in one but not the other silently ships a broken nav or an orphan URL,
   so the build refuses instead. */
function assertDocsNavParity(): void {
  const navPaths = new Set(DOCS_NAV.map((entry) => entry.path))
  const docsPages = new Set(
    PAGES.filter((p) => p.path.startsWith(`/docs/`)).map((p) => p.path)
  )
  const notInPages = [...navPaths].filter((path) => !docsPages.has(path))
  const notInNav = [...docsPages].filter((path) => !navPaths.has(path))
  if (notInPages.length || notInNav.length) {
    throw new Error(
      `docs nav parity: DOCS_NAV-only: [${notInPages.join(`, `)}] PAGES-only: [${notInNav.join(`, `)}]`
    )
  }
}

assertDocsNavParity()
for (const page of PAGES) prerenderPage(page)
writeSitemap()
writeLlmsTxt()
console.log(`prerender complete`)
