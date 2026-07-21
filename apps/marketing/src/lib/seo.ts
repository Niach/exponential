/* Single source of truth for per-page SEO metadata.
   Consumed by scripts/prerender.tsx (injects meta/OG/Twitter/JSON-LD + canonical
   into each dist HTML head) and scripts/generate-og.tsx (OG image copy).
   The source HTML heads carry ONLY charset/viewport/title/fonts/icons — every
   description/canonical/og:/twitter:/ld+json tag is owned here. */

import { CLOUD_PLANS } from "./plans"

export const SITE_ORIGIN = `https://exponential.at`
export const SITE_NAME = `Exponential`

/* JSON-LD is emitted verbatim as the contents of a
   <script type="application/ld+json"> tag. Kept as plain objects (or arrays of
   objects → emitted as an @graph-less array) so the manifest stays declarative. */
export type JsonLd = Record<string, unknown> | Record<string, unknown>[]

export type PageSeo = {
  /* Site-root-relative path WITH trailing slash, matching the dist layout and
     the vite rollup inputs. Home is `/`. */
  path: string
  /* dist/<...>/index.html path relative to dist, used by the prerender rewriter. */
  htmlFile: string
  /* src page-component source file(s), used for sitemap lastmod (git log). */
  sources: string[]
  title: string
  description: string
  /* Absolute-from-root OG image path under /og/. */
  ogImage: string
  jsonLd?: JsonLd
}

const organization: Record<string, unknown> = {
  "@context": `https://schema.org`,
  "@type": `Organization`,
  name: SITE_NAME,
  url: `${SITE_ORIGIN}/`,
  logo: `${SITE_ORIGIN}/icon-512.png`,
  sameAs: [`https://github.com/Niach/exponential`],
}

const softwareApplication: Record<string, unknown> = {
  "@context": `https://schema.org`,
  "@type": `SoftwareApplication`,
  name: SITE_NAME,
  applicationCategory: `DeveloperApplication`,
  operatingSystem: `Web, iOS, Android, macOS, Windows, Linux`,
  description: `An issue tracker with a built-in coding IDE. Assign issues to local AI agents that run in your terminal and open GitHub pull requests.`,
  url: `${SITE_ORIGIN}/`,
  offers: {
    "@type": `Offer`,
    price: `0`,
    priceCurrency: `USD`,
  },
}

/* Pricing Offers derive from the canonical plan module (lib/plans.ts,
   which mirrors apps/web/src/lib/billing.ts). Enterprise is deliberately
   absent — a schema.org Offer needs a numeric price, Enterprise is
   contact-sales, and only plans with a priceNumber make the list. */
const pricingProduct: Record<string, unknown> = {
  "@context": `https://schema.org`,
  "@type": `Product`,
  name: `${SITE_NAME} — plans`,
  description: `Per-seat pricing for Exponential cloud. Free for individuals; local AI agents on every tier.`,
  /* Google's merchant-listing validation REQUIRES image on Product — 16:9 and
     1:1 variants per its aspect-ratio recommendations (GSC flags the page
     invalid without it). */
  image: [`${SITE_ORIGIN}/og/og-pricing.png`, `${SITE_ORIGIN}/icon-512.png`],
  brand: { "@type": `Brand`, name: SITE_NAME },
  offers: CLOUD_PLANS.filter((plan) => plan.priceNumber !== undefined).map(
    (plan) => ({
      "@type": `Offer`,
      name: plan.name,
      price: String(plan.priceNumber),
      priceCurrency: `USD`,
      url: `${SITE_ORIGIN}/pricing/`,
      ...(plan.priceDescription ? { description: plan.priceDescription } : {}),
    })
  ),
}

function breadcrumb(
  items: { name: string; path: string }[]
): Record<string, unknown> {
  return {
    "@context": `https://schema.org`,
    "@type": `BreadcrumbList`,
    itemListElement: items.map((it, i) => ({
      "@type": `ListItem`,
      position: i + 1,
      name: it.name,
      item: `${SITE_ORIGIN}${it.path}`,
    })),
  }
}

export const PAGES: PageSeo[] = [
  {
    path: `/`,
    htmlFile: `index.html`,
    sources: [
      `src/HomePage.tsx`,
      `src/components/HomePricing.tsx`,
      `src/components/PlanCards.tsx`,
      `src/lib/plans.ts`,
    ],
    title: `Exponential — The development platform for teams and agents`,
    description: `Issue tracking with coding agents built in. Feedback in, pull requests out. Native on web, iOS, Android, macOS, Windows and Linux. Free for individuals, free to self-host under 10 people.`,
    ogImage: `/og/og-home.png`,
    jsonLd: [organization, softwareApplication],
  },
  {
    path: `/pricing/`,
    htmlFile: `pricing/index.html`,
    sources: [
      `src/PricingPage.tsx`,
      `src/components/PlanCards.tsx`,
      `src/lib/plans.ts`,
    ],
    title: `Pricing — Exponential`,
    description: `Per-seat pricing: Free for individuals, Pro $5/seat/mo, Business $10/seat/mo, Enterprise custom — contact sales. Local AI agents free on every tier. Self-host free under 10 people.`,
    ogImage: `/og/og-pricing.png`,
    jsonLd: pricingProduct,
  },
  {
    path: `/download/`,
    htmlFile: `download/index.html`,
    sources: [`src/DownloadPage.tsx`, `src/components/DownloadSection.tsx`],
    title: `Download Exponential`,
    description: `Get Exponential on every platform — the native Rust desktop IDE for macOS, Windows and Linux, plus iOS and Android companions.`,
    ogImage: `/og/og-download.png`,
  },
  {
    path: `/docs/`,
    htmlFile: `docs/index.html`,
    sources: [`src/DocsPage.tsx`],
    title: `Docs — Exponential`,
    description: `Everything about Exponential — quickstart plus guides for issues and boards, coding with Claude, feedback and the helpdesk, the embeddable widget, MCP, and the apps.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
    ]),
  },
  {
    path: `/docs/getting-started/`,
    htmlFile: `docs/getting-started/index.html`,
    sources: [`src/GettingStartedDocsPage.tsx`],
    title: `Getting started — Exponential docs`,
    description: `Sign up, create your first board, connect GitHub, invite your team, and pick a plan — from zero to a working tracker.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Getting started`, path: `/docs/getting-started/` },
    ]),
  },
  {
    path: `/docs/issues/`,
    htmlFile: `docs/issues/index.html`,
    sources: [`src/IssuesDocsPage.tsx`],
    title: `Issues & boards — Exponential docs`,
    description: `The board, all seven statuses, markdown issues with @mentions and #issue refs, notifications with the hourly email digest, and how branches and PRs link back.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Issues & boards`, path: `/docs/issues/` },
    ]),
  },
  {
    path: `/docs/coding/`,
    htmlFile: `docs/coding/index.html`,
    sources: [`src/CodingDocsPage.tsx`],
    title: `Coding with Claude — Exponential docs`,
    description: `Hand issues to Claude from the desktop IDE: setup, the start-coding dialog, single and batch runs, steering live sessions, reviewing and merging, and run configs.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Coding with Claude`, path: `/docs/coding/` },
    ]),
  },
  {
    path: `/docs/feedback/`,
    htmlFile: `docs/feedback/index.html`,
    sources: [`src/FeedbackDocsPage.tsx`],
    title: `Feedback & helpdesk — Exponential docs`,
    description: `Collect feedback with the embeddable widget and run the team helpdesk — email conversations with reporters, answered from a shared support inbox and escalated to issues in one click.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Feedback & helpdesk`, path: `/docs/feedback/` },
    ]),
  },
  {
    path: `/docs/widget/`,
    htmlFile: `docs/widget/index.html`,
    sources: [`src/WidgetDocsPage.tsx`],
    title: `Feedback widget — Exponential docs`,
    description: `Embed the feedback widget on any site: the snippet, feedback and support modes, the JS API (identify, custom data), annotated screenshots, and what lands in your tracker.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Feedback widget`, path: `/docs/widget/` },
    ]),
  },
  {
    path: `/docs/mcp/`,
    htmlFile: `docs/mcp/index.html`,
    sources: [`src/McpDocsPage.tsx`],
    title: `MCP & API — Exponential docs`,
    description: `Connect Claude, ChatGPT, Codex, Claude Code, or Cursor to your tracker over MCP — OAuth with per-team scoping or API keys, plus the full tool reference.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `MCP & API`, path: `/docs/mcp/` },
    ]),
  },
  {
    path: `/docs/apps/`,
    htmlFile: `docs/apps/index.html`,
    sources: [`src/AppsDocsPage.tsx`],
    title: `Mobile & desktop apps — Exponential docs`,
    description: `The native apps: self-updating desktop for macOS, Windows and Linux, iOS and Android companions with push notifications and live agent steering.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Mobile & desktop apps`, path: `/docs/apps/` },
    ]),
  },
  {
    path: `/docs/self-host/`,
    htmlFile: `docs/self-host/index.html`,
    sources: [`src/SelfHostDocsPage.tsx`],
    title: `Self-host — Exponential`,
    description: `Self-host Exponential with one Docker Compose — Postgres, ElectricSQL, Garage and Caddy. No plan limits with SELF_HOSTED=true, free under 10 people.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
      { name: `Self-host`, path: `/docs/self-host/` },
    ]),
  },
  {
    path: `/privacy/`,
    htmlFile: `privacy/index.html`,
    sources: [`src/PrivacyPage.tsx`],
    title: `Privacy Policy — Exponential`,
    description: `How Exponential collects, uses and protects your data.`,
    ogImage: `/og/og-default.png`,
  },
  {
    path: `/terms/`,
    htmlFile: `terms/index.html`,
    sources: [`src/TermsPage.tsx`],
    title: `Terms of Service — Exponential`,
    description: `The terms that govern your use of Exponential.`,
    ogImage: `/og/og-default.png`,
  },
  {
    path: `/imprint/`,
    htmlFile: `imprint/index.html`,
    sources: [`src/ImprintPage.tsx`],
    title: `Imprint — Exponential`,
    description: `Legal disclosure (Impressum) for Exponential.`,
    ogImage: `/og/og-default.png`,
  },
  {
    path: `/contact/`,
    htmlFile: `contact/index.html`,
    sources: [`src/ContactPage.tsx`, `src/components/ContactForm.tsx`],
    title: `Contact sales — Exponential`,
    description: `Talk to us about Enterprise and running Exponential in-house.`,
    ogImage: `/og/og-default.png`,
  },
]
