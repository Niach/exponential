/* Single source of truth for per-page SEO metadata.
   Consumed by scripts/prerender.tsx (injects meta/OG/Twitter/JSON-LD + canonical
   into each dist HTML head) and scripts/generate-og.tsx (OG image copy).
   The source HTML heads carry ONLY charset/viewport/title/fonts/icons — every
   description/canonical/og:/twitter:/ld+json tag is owned here. */

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
  operatingSystem: `Web, iOS, Android, macOS, Linux`,
  description: `An issue tracker with a built-in coding IDE. Assign issues to local AI agents that run in your terminal and open GitHub pull requests.`,
  url: `${SITE_ORIGIN}/`,
  offers: {
    "@type": `Offer`,
    price: `0`,
    priceCurrency: `USD`,
  },
}

/* Pricing Offers mirror the cloud tiers in
   apps/marketing/src/components/PlanCards.tsx (which itself mirrors
   apps/web/src/lib/billing.ts). Keep in sync when prices change.
   Enterprise is deliberately absent — a schema.org Offer needs a numeric
   price, and Enterprise is contact-sales. */
const pricingProduct: Record<string, unknown> = {
  "@context": `https://schema.org`,
  "@type": `Product`,
  name: `${SITE_NAME} — plans`,
  description: `Per-seat pricing for Exponential cloud. Free for individuals; local AI agents on every tier.`,
  brand: { "@type": `Brand`, name: SITE_NAME },
  offers: [
    {
      "@type": `Offer`,
      name: `Free`,
      price: `0`,
      priceCurrency: `USD`,
      url: `${SITE_ORIGIN}/pricing/`,
    },
    {
      "@type": `Offer`,
      name: `Pro`,
      price: `5`,
      priceCurrency: `USD`,
      url: `${SITE_ORIGIN}/pricing/`,
      description: `Per seat, per month, billed yearly.`,
    },
    {
      "@type": `Offer`,
      name: `Business`,
      price: `10`,
      priceCurrency: `USD`,
      url: `${SITE_ORIGIN}/pricing/`,
      description: `Per seat, per month, billed monthly or yearly.`,
    },
  ],
}

function breadcrumb(items: { name: string; path: string }[]): Record<string, unknown> {
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
    sources: [`src/HomePage.tsx`],
    title: `Exponential — Issue tracking that ships code`,
    description: `An issue tracker with a built-in coding IDE. Assign issues to Claude or Codex — agents run locally in your terminal and open GitHub PRs. Native on web, iOS, Android, macOS and Linux. Free cloud or self-host.`,
    ogImage: `/og/og-home.png`,
    jsonLd: [organization, softwareApplication],
  },
  {
    path: `/pricing/`,
    htmlFile: `pricing/index.html`,
    sources: [`src/PricingPage.tsx`, `src/components/PlanCards.tsx`],
    title: `Pricing — Exponential`,
    description: `Per-seat pricing: Free for individuals, Pro $5/seat/mo, Business $10/seat/mo, Enterprise with SLA — let's talk. Local AI agents free on every tier. Self-host free and unlimited.`,
    ogImage: `/og/og-pricing.png`,
    jsonLd: pricingProduct,
  },
  {
    path: `/download/`,
    htmlFile: `download/index.html`,
    sources: [`src/DownloadPage.tsx`, `src/components/DownloadSection.tsx`],
    title: `Download Exponential`,
    description: `Get Exponential on every platform — the native Rust desktop IDE for macOS and Linux, plus iOS and Android companions.`,
    ogImage: `/og/og-download.png`,
  },
  {
    path: `/docs/`,
    htmlFile: `docs/index.html`,
    sources: [`src/DocsPage.tsx`],
    title: `Docs — Exponential`,
    description: `How to use Exponential — issues, the desktop and mobile apps, local AI agents, and integrations.`,
    ogImage: `/og/og-docs.png`,
    jsonLd: breadcrumb([
      { name: `Home`, path: `/` },
      { name: `Docs`, path: `/docs/` },
    ]),
  },
  {
    path: `/docs/self-host/`,
    htmlFile: `docs/self-host/index.html`,
    sources: [`src/SelfHostDocsPage.tsx`],
    title: `Self-host — Exponential`,
    description: `Self-host Exponential with one Docker Compose — Postgres, ElectricSQL, Garage and Caddy. Unlimited everything with SELF_HOSTED=true.`,
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
