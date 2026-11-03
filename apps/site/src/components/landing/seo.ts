/* Per-page SEO for the landing area, ported from apps/marketing/src/lib/seo.ts:
   Starlight already writes description, canonical and og:title/url/
   description; seoHead adds the full <title>, the OG/Twitter image, the
   og:type and the page's JSON-LD (StarlightPage `head`). */
import type { StarlightPageProps } from "@astrojs/starlight/props"
import { LINKS } from "./links.ts"
import { PLANS } from "./plans.ts"

export const SITE_ORIGIN = `https://exponential.at`
export const SITE_NAME = `Exponential`

type Head = NonNullable<StarlightPageProps[`frontmatter`][`head`]>
type JsonLd = Record<string, unknown>

export function seoHead({ title, ogImage = `/og/og-default.png`, type = `website`, jsonLd = [] }: { title: string; ogImage?: string; type?: string; jsonLd?: JsonLd[] }): Head {
  const image = `${SITE_ORIGIN}${ogImage}`
  return [
    { tag: `title`, content: title },
    { tag: `meta`, attrs: { property: `og:type`, content: type } },
    { tag: `meta`, attrs: { property: `og:image`, content: image } },
    { tag: `meta`, attrs: { property: `og:image:width`, content: `1200` } },
    { tag: `meta`, attrs: { property: `og:image:height`, content: `630` } },
    { tag: `meta`, attrs: { name: `twitter:image`, content: image } },
    { tag: `link`, attrs: { rel: `manifest`, href: `/site.webmanifest` } },
    { tag: `link`, attrs: { rel: `apple-touch-icon`, href: `/apple-touch-icon.png` } },
    ...jsonLd.map((data) => ({ tag: `script` as const, attrs: { type: `application/ld+json` }, content: JSON.stringify(data) })),
  ]
}

export function breadcrumb(items: { name: string; path: string }[]): JsonLd {
  return {
    "@context": `https://schema.org`,
    "@type": `BreadcrumbList`,
    itemListElement: items.map((item, i) => ({ "@type": `ListItem`, position: i + 1, name: item.name, item: `${SITE_ORIGIN}${item.path}` })),
  }
}

export const ORGANIZATION: JsonLd = {
  "@context": `https://schema.org`,
  "@type": `Organization`,
  name: SITE_NAME,
  url: `${SITE_ORIGIN}/`,
  logo: `${SITE_ORIGIN}/icon-512.png`,
  sameAs: [LINKS.github.repo, LINKS.downloads.ios, LINKS.downloads.android],
}

export const SOFTWARE_APPLICATION: JsonLd = {
  "@context": `https://schema.org`,
  "@type": `SoftwareApplication`,
  name: SITE_NAME,
  applicationCategory: `DeveloperApplication`,
  operatingSystem: `Web, iOS, Android, macOS, Windows, Linux`,
  description: `The next generation dev platform for teams and agents. Agents plan the work, write the code and build apps, running Claude Code or Codex on your own machines and subscription.`,
  url: `${SITE_ORIGIN}/`,
  image: [`${SITE_ORIGIN}/og/og-home.png`, `${SITE_ORIGIN}/icon-512.png`],
  offers: { "@type": `Offer`, url: `${SITE_ORIGIN}/pricing/`, price: `0`, priceCurrency: `EUR`, availability: `https://schema.org/InStock` },
}

/* Offers derive from the plans; only plans with a numeric price qualify. */
export const PRICING_PRODUCT: JsonLd = {
  "@context": `https://schema.org`,
  "@type": `Product`,
  name: `${SITE_NAME} plans`,
  description: `Per-seat pricing for Exponential cloud. Free for teams of three; local AI agents on every tier.`,
  url: `${SITE_ORIGIN}/pricing/`,
  image: [`${SITE_ORIGIN}/og/og-pricing.png`, `${SITE_ORIGIN}/icon-512.png`],
  brand: { "@type": `Brand`, name: SITE_NAME },
  offers: PLANS.filter((plan) => plan.priceNumber !== undefined).map((plan) => ({
    "@type": `Offer`,
    name: plan.name,
    price: String(plan.priceNumber),
    priceCurrency: `EUR`,
    availability: `https://schema.org/InStock`,
    url: `${SITE_ORIGIN}/pricing/`,
    ...(plan.priceDescription ? { description: plan.priceDescription } : {}),
  })),
}
