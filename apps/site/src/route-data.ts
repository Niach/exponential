/* SEO for every Starlight route (#EXP-1591): Starlight writes canonical,
   description and og:title/url/description; this adds what it leaves out.
   An OG/Twitter image where the page has none (docs and SDK pages; landing
   pages keep seoHead's), twitter:title/description, a genUI suffix in the
   <title> of genUI docs, and BreadcrumbList + TechArticle JSON-LD on docs. */
import { defineRouteMiddleware } from "@astrojs/starlight/route-data"
import { getEntry } from "astro:content"
import { SITE_NAME, SITE_ORIGIN, breadcrumb } from "./components/landing/seo.ts"

const DOCS_OG = `/og/og-docs.png`
const GENUI = `/docs/sdk/genui/`
const COMPONENTS = `${GENUI}reference/components/`

type HeadEntry = { tag: string; attrs?: Record<string, string | boolean | undefined>; content?: string }

const has = (head: HeadEntry[], key: `property` | `name`, value: string) => head.some((h) => h.tag === `meta` && h.attrs?.[key] === value)
const meta = (key: `property` | `name`, value: string, content: string): HeadEntry => ({ tag: `meta`, attrs: { [key]: value, content } })

/** The <title> a genUI docs page gets (undefined = Starlight's default). */
export function genuiTitle(path: string, title: string): string | undefined {
  if (!path.startsWith(GENUI) || path === GENUI) return undefined
  const name = path.startsWith(COMPONENTS) && path !== COMPONENTS ? `${title} component` : title
  return `${name} · genUI | ${SITE_NAME}`
}

/** Breadcrumb names for a docs path: each ancestor that is a page, by its title. */
async function crumbs(path: string, title: string) {
  const parts = path.split(`/`).filter(Boolean)
  const items = [{ name: SITE_NAME, path: `/` }]
  for (let i = 1; i < parts.length; i++) {
    const id = parts.slice(0, i).join(`/`)
    const entry = await getEntry(`docs`, id)
    if (entry) items.push({ name: entry.data.title, path: `/${id}/` })
  }
  items.push({ name: title, path })
  return items
}

export const onRequest = defineRouteMiddleware(async (context) => {
  const route = context.locals.starlightRoute
  const head = route.head as HeadEntry[]
  const path = context.url.pathname
  const { title, description } = route.entry.data

  const custom = genuiTitle(path, title)
  const titleTag = head.find((h) => h.tag === `title`)
  if (custom && titleTag) titleTag.content = custom

  if (!has(head, `property`, `og:image`)) {
    const image = `${SITE_ORIGIN}${DOCS_OG}`
    head.push(meta(`property`, `og:image`, image), meta(`property`, `og:image:width`, `1200`), meta(`property`, `og:image:height`, `630`))
    if (!has(head, `name`, `twitter:image`)) head.push(meta(`name`, `twitter:image`, image))
  }
  if (!has(head, `name`, `twitter:title`)) head.push(meta(`name`, `twitter:title`, title))
  if (description && !has(head, `name`, `twitter:description`)) head.push(meta(`name`, `twitter:description`, description))

  if (path.startsWith(`/docs/`)) {
    const article = {
      "@context": `https://schema.org`,
      "@type": `TechArticle`,
      headline: title,
      ...(description ? { description } : {}),
      url: `${SITE_ORIGIN}${path}`,
      image: `${SITE_ORIGIN}${DOCS_OG}`,
      publisher: { "@type": `Organization`, name: SITE_NAME, url: `${SITE_ORIGIN}/` },
    }
    for (const data of [breadcrumb(await crumbs(path, title)), article]) {
      head.push({ tag: `script`, attrs: { type: `application/ld+json` }, content: JSON.stringify(data) })
    }
  }
})
