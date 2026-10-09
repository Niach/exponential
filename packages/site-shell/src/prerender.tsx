/* The postbuild prerender + SEO engine every public site runs under Bun after
   `vite build` (apps/marketing scripts/prerender.tsx, apps/ui-site
   scripts/prerender.tsx). For each page it (1) renderToString's the page into
   the `<div id="root"></div>` marker of the built HTML (crawlers and first
   paint get real markup, then the client hydrates), (2) strips any stray SEO
   tags from the head and injects the canonical/OG/Twitter/JSON-LD block from
   the site's page manifest, then (3) writes dist/sitemap.xml (lastmod from
   git) and dist/llms.txt (llmstxt.org) from the same manifest, so neither can
   drift from the pages. */

import { renderToString } from "react-dom/server"
import { execFileSync } from "node:child_process"
import { existsSync, readFileSync, writeFileSync } from "node:fs"
import { resolve } from "node:path"
import type { ReactElement } from "react"
import type { JsonLd, PageSeo } from "./seo"

const MARKER = `<div id="root"></div>`

export type LlmsConfig = {
  /* The `> ` summary line under the title. */
  summary: string
  /* Plain paragraphs after the summary. */
  intro: string[]
  /* Every page path must appear in exactly one section (the build refuses otherwise). */
  sections: { heading: string; paths: string[] }[]
  /* Non-page artefacts, listed literally: `- [label](url): description`. */
  extras?: { heading: string; items: { label: string; url: string; description: string }[] }[]
}

export type PrerenderConfig = {
  /* The site's package root (dist/ lives under it; git runs in it). */
  root: string
  origin: string
  siteName: string
  pages: PageSeo[]
  /* The page element for a manifest path. */
  render: (path: string) => ReactElement
  llms: LlmsConfig
}

function escapeAttr(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
}

/* Remove any SEO tags that slipped into the source head, so the manifest is
   the sole owner of the injected block (idempotent across rebuilds). */
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

export function metaBlock(page: PageSeo, origin: string, siteName: string): string {
  const url = `${origin}${page.path}`
  const image = `${origin}${page.ogImage}`
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
    `<meta property="og:site_name" content="${escapeAttr(siteName)}" />`,
    `<meta property="og:locale" content="en_US" />`,
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${t}" />`,
    `<meta name="twitter:description" content="${d}" />`,
    `<meta name="twitter:image" content="${escapeAttr(image)}" />`,
  ]
  if (page.jsonLd) {
    const blocks: JsonLd[] = Array.isArray(page.jsonLd) ? page.jsonLd : [page.jsonLd]
    for (const block of blocks) {
      /* Escape </script> so JSON-LD can never break out of the tag. */
      const json = JSON.stringify(block).replace(/<\//g, `<\\/`)
      lines.push(`<script type="application/ld+json">${json}</script>`)
    }
  }
  return lines.map((l) => `    ${l}`).join(`\n`)
}

function gitLastmod(root: string, sources: string[]): string {
  /* One call with every source: git returns the latest commit touching ANY
     of the paths, so a component-level edit moves the page's lastmod. */
  try {
    const iso = execFileSync(`git`, [`log`, `-1`, `--format=%cI`, `--`, ...sources], {
      cwd: root,
      encoding: `utf8`,
    }).trim()
    if (iso) return iso
  } catch {
    /* fall through to the build-date fallback (git-less builds) */
  }
  return new Date().toISOString()
}

function prerenderPage(config: PrerenderConfig, page: PageSeo): void {
  const dist = resolve(config.root, `dist`)
  const htmlPath = resolve(dist, page.htmlFile)
  if (!existsSync(htmlPath)) {
    throw new Error(`prerender: dist file missing for ${page.path}: ${htmlPath}`)
  }
  let html = readFileSync(htmlPath, `utf8`)
  if (!html.includes(MARKER)) {
    throw new Error(
      `prerender: marker ${MARKER} not found in ${page.htmlFile} — cannot inject SSR markup`
    )
  }
  const body = renderToString(config.render(page.path))
  html = html.replace(MARKER, () => `<div id="root">${body}</div>`)
  if (!html.includes(`</head>`)) {
    throw new Error(`prerender: </head> not found in ${page.htmlFile}`)
  }
  html = html
    .replace(/<head>([\s\S]*?)<\/head>/i, (_m, head) => `<head>${stripExistingSeo(head)}</head>`)
    .replace(`</head>`, () => `${metaBlock(page, config.origin, config.siteName)}\n  </head>`)
  writeFileSync(htmlPath, html)
  console.log(`prerendered ${page.path} → ${page.htmlFile} (${body.length} bytes body)`)
}

function writeSitemap(config: PrerenderConfig): void {
  const urls = config.pages.map((p) => {
    const loc = `${config.origin}${p.path}`
    return `  <url><loc>${loc}</loc><lastmod>${gitLastmod(config.root, p.sources)}</lastmod></url>`
  })
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls.join(`\n`)}
</urlset>
`
  writeFileSync(resolve(config.root, `dist`, `sitemap.xml`), xml)
  console.log(`wrote dist/sitemap.xml (${config.pages.length} urls)`)
}

/* llms.txt (https://llmstxt.org): a markdown site map for LLM crawlers. */
export function llmsTxt(config: Omit<PrerenderConfig, `render` | `root`>): string {
  const { llms, pages, origin, siteName } = config
  const byPath = new Map(pages.map((p) => [p.path, p]))
  const lines: string[] = [`# ${siteName}`, ``, `> ${llms.summary}`]
  for (const paragraph of llms.intro) lines.push(``, paragraph)
  for (const section of llms.sections) {
    lines.push(``, `## ${section.heading}`, ``)
    for (const path of section.paths) {
      const page = byPath.get(path)
      if (!page) throw new Error(`llms.txt: unknown page path ${path}`)
      lines.push(`- [${page.title}](${origin}${page.path}): ${page.description}`)
    }
  }
  for (const extra of llms.extras ?? []) {
    lines.push(``, `## ${extra.heading}`, ``)
    for (const item of extra.items) lines.push(`- [${item.label}](${item.url}): ${item.description}`)
  }
  const covered = new Set(llms.sections.flatMap((s) => s.paths))
  const missing = pages.filter((p) => !covered.has(p.path))
  if (missing.length) {
    throw new Error(`llms.txt: pages missing from a section: ${missing.map((p) => p.path).join(`, `)}`)
  }
  return `${lines.join(`\n`)}\n`
}

/** Prerender every page, then write sitemap.xml and llms.txt. */
export function prerenderSite(config: PrerenderConfig): void {
  for (const page of config.pages) prerenderPage(config, page)
  writeSitemap(config)
  writeFileSync(resolve(config.root, `dist`, `llms.txt`), llmsTxt(config))
  console.log(`wrote dist/llms.txt (${config.pages.length} pages)`)
  console.log(`prerender complete`)
}
