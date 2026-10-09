/* The SEO types every public site's page manifest uses (apps/marketing
   src/lib/seo.ts, apps/ui-site src/lib/seo.ts) and the helpers they share.
   The manifest is the single owner of description/canonical/OG/Twitter/
   JSON-LD markup; prerender.tsx injects it. */

/* Emitted verbatim inside <script type="application/ld+json">. */
export type JsonLd = Record<string, unknown> | Record<string, unknown>[]

export type PageSeo = {
  /* Site-root-relative path WITH trailing slash, matching the dist layout and
     the vite rollup inputs. Home is `/`. */
  path: string
  /* dist/<...>/index.html path relative to dist, used by the prerender rewriter. */
  htmlFile: string
  /* Source file(s) of the page, used for sitemap lastmod (git log). */
  sources: string[]
  title: string
  description: string
  /* Absolute-from-root OG image path. */
  ogImage: string
  jsonLd?: JsonLd
}

export function breadcrumb(
  origin: string,
  items: { name: string; path: string }[]
): Record<string, unknown> {
  return {
    "@context": `https://schema.org`,
    "@type": `BreadcrumbList`,
    itemListElement: items.map((it, i) => ({
      "@type": `ListItem`,
      position: i + 1,
      name: it.name,
      item: `${origin}${it.path}`,
    })),
  }
}
