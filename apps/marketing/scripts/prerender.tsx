/* Postbuild prerender + SEO injection for exponential.at. Runs under Bun after
   `vite build`:

     tsc --noEmit && vite build && bun run scripts/prerender.tsx

   The engine (renderToString into the root marker, the canonical/OG/Twitter/
   JSON-LD head block, sitemap.xml with git lastmod, llms.txt) is shared with
   ui.exponential.at: @exp/site-shell/prerender. This file is the site's
   config: the page components, the llms.txt sections, and the docs-nav
   parity gate. The source HTML heads carry only charset/viewport/title/fonts/
   icons; every SEO tag is owned by src/lib/seo.ts. */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs"
import { resolve, dirname } from "node:path"
import { fileURLToPath } from "node:url"
import type { ComponentType } from "react"
import { renderToStaticMarkup } from "react-dom/server"
import { prerenderSite } from "@exp/site-shell/prerender"

import { PAGES, SITE_ORIGIN, SITE_NAME } from "../src/lib/seo"
import { HomePage } from "../src/HomePage"
import { PricingPage } from "../src/PricingPage"
import { DownloadPage } from "../src/DownloadPage"
import { DOCS_NAV } from "../src/lib/docs-nav"
import { DocsPage } from "../src/DocsPage"
import { GettingStartedDocsPage } from "../src/GettingStartedDocsPage"
import { IssuesDocsPage } from "../src/IssuesDocsPage"
import { CodingDocsPage } from "../src/CodingDocsPage"
import { ActionsDocsPage } from "../src/ActionsDocsPage"
import { CliDocsPage } from "../src/CliDocsPage"
import { FeedbackDocsPage } from "../src/FeedbackDocsPage"
import { WidgetDocsPage } from "../src/WidgetDocsPage"
import { McpDocsPage } from "../src/McpDocsPage"
import { AppsDocsPage } from "../src/AppsDocsPage"
import { SelfHostDocsPage } from "../src/SelfHostDocsPage"
import { PrivacyPage } from "../src/PrivacyPage"
import { TermsPage } from "../src/TermsPage"
import { ImprintPage } from "../src/ImprintPage"
import { ContactPage } from "../src/ContactPage"
import { BlogIndexPage } from "../src/BlogIndexPage"
import { BlogPostPage } from "../src/BlogPostPage"
import { BLOG_POSTS } from "../src/blog/posts"
import { rssXml } from "../src/blog/rss"
import { BLOG_PATH, postPath, readingMinutesOf } from "../src/blog/types"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), `..`)

/* Path → page component, keyed by PageSeo.path. */
const COMPONENTS: Record<string, ComponentType> = {
  "/": HomePage,
  "/pricing/": PricingPage,
  "/download/": DownloadPage,
  "/docs/": DocsPage,
  "/docs/getting-started/": GettingStartedDocsPage,
  "/docs/issues/": IssuesDocsPage,
  "/docs/coding/": CodingDocsPage,
  "/docs/actions/": ActionsDocsPage,
  "/docs/cli/": CliDocsPage,
  "/docs/feedback/": FeedbackDocsPage,
  "/docs/widget/": WidgetDocsPage,
  "/docs/mcp/": McpDocsPage,
  "/docs/apps/": AppsDocsPage,
  "/docs/self-host/": SelfHostDocsPage,
  "/privacy/": PrivacyPage,
  "/terms/": TermsPage,
  "/imprint/": ImprintPage,
  "/contact/": ContactPage,
  /* The blog (EXP-1366): the index and one page per registered post. */
  [BLOG_PATH]: BlogIndexPage,
  ...Object.fromEntries(
    BLOG_POSTS.map((post) => [postPath(post.meta.slug), () => <BlogPostPage post={post} />])
  ),
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

/* Title parity: prerender rewrites og:title and twitter:title from PAGES but
   never <title>, so a shell whose <title> drifts from its manifest entry
   ships two different titles. Every page, the blog shells included. */
function assertTitleParity(): void {
  const offenders = PAGES.flatMap((page) => {
    const html = readFileSync(resolve(ROOT, page.htmlFile), `utf8`)
    const match = html.match(/<title>([\s\S]*?)<\/title>/)
    const shell = match ? match[1].replaceAll(`&amp;`, `&`).trim() : undefined
    return shell === page.title ? [] : [`${page.htmlFile}: <title>${shell}</title> != \`${page.title}\``]
  })
  if (offenders.length) {
    throw new Error(`title parity:\n${offenders.join(`\n`)}`)
  }
}

/* A post's reading time is declared on its meta (the index card and the
   post head print it on the server and the client alike); the rendered
   body says what it really is. Off by more than a minute = the build
   refuses, with the number to put in. */
function assertReadingTimes(): void {
  for (const { meta, Post } of BLOG_POSTS) {
    const text = renderToStaticMarkup(<Post />)
      .replace(/<[^>]+>/g, ` `)
      .replace(/&[a-z#0-9]+;/g, ` `)
    const words = text.split(/\s+/).filter(Boolean).length
    const minutes = readingMinutesOf(words)
    if (Math.abs(minutes - meta.readingMinutes) > 1) {
      throw new Error(
        `blog: "${meta.slug}" declares readingMinutes ${meta.readingMinutes} but its body has ${words} words (${minutes} min)`
      )
    }
  }
}

const SECTIONS = [
    { heading: `Product`, paths: [`/`, `/pricing/`, `/download/`] },
    {
      heading: `Docs`,
      paths: [
        `/docs/`,
        `/docs/getting-started/`,
        `/docs/issues/`,
        `/docs/coding/`,
        `/docs/actions/`,
        `/docs/cli/`,
        `/docs/feedback/`,
        `/docs/widget/`,
        `/docs/mcp/`,
        `/docs/apps/`,
        `/docs/self-host/`,
      ],
    },
    {
      heading: `Blog`,
      paths: [BLOG_PATH, ...BLOG_POSTS.map((post) => postPath(post.meta.slug))],
    },
    {
      heading: `Company & legal`,
      paths: [`/contact/`, `/privacy/`, `/terms/`, `/imprint/`],
    },
  ]

assertDocsNavParity()
assertTitleParity()
assertReadingTimes()
prerenderSite({
  root: ROOT,
  origin: SITE_ORIGIN,
  siteName: SITE_NAME,
  pages: PAGES,
  render: (path) => {
    const Component = COMPONENTS[path]
    if (!Component) throw new Error(`prerender: no component for ${path}`)
    return <Component />
  },
  llms: {
    summary: `An open-source (Apache-2.0) realtime tracker for issues, user feedback and coding agents. Assign issues to AI agents that run locally in your terminal and open GitHub pull requests (unlimited sessions, flat price). Native apps for web, iOS, Android, macOS, Windows and Linux. Free cloud tier for teams of three, or self-host it for free at any company size.`,
    intro: [
      `The app itself lives at https://app.exponential.at; the source code is at https://github.com/Niach/exponential. The open generative-UI SDK the apps are built on, Exponential UI, has its own site at https://ui.exponential.at.`,
    ],
    sections: SECTIONS,
    extras: [
      {
        heading: `Agent resources`,
        items: [
          { label: `SKILL.md`, url: `${SITE_ORIGIN}/SKILL.md`, description: `Agent-readable product brief: the MCP endpoint and auth, tool families, core concepts, the exponential CLI, and feedback-widget integration.` },
          { label: `install.sh`, url: `${SITE_ORIGIN}/install.sh`, description: `CLI installer for Linux and macOS. One script for cloud and self-host; the instance rides EXP_INSTANCE.` },
        ],
      },
    ],
  },
})

/* /blog/rss.xml from the same registry the pages come from. */
const RSS_DIR = resolve(ROOT, `dist/blog`)
mkdirSync(RSS_DIR, { recursive: true })
writeFileSync(
  resolve(RSS_DIR, `rss.xml`),
  rssXml({ origin: SITE_ORIGIN, siteName: SITE_NAME, posts: BLOG_POSTS.map((post) => post.meta) })
)
console.log(`wrote dist/blog/rss.xml (${BLOG_POSTS.length} posts)`)
