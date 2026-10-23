/* The blog's PAGES entries (one for the index, one per post), spliced into
   src/lib/seo.ts. BlogPosting + breadcrumb structured data per post, a Blog
   item on the index. Titles use the site's middle dot; no em dashes
   (apps/web/src/lib/copy-style.test.ts gates the manifest). */
import { breadcrumb, type PageSeo } from "@exp/site-shell/seo"
import { BLOG_POSTS } from "./posts"
import { BLOG_PATH, postPath } from "./types"

export const BLOG_INDEX_TITLE = `Blog · Exponential`
export const BLOG_INDEX_DESCRIPTION = `Notes on how Exponential works: the coding agent's session as the interface, and the generative UI the apps are made of. Each claim has a screenshot or an issue number.`

export const postTitle = (title: string) => `${title} · Exponential blog`

export function blogPages(origin: string, siteName: string): PageSeo[] {
  const organization = {
    "@type": `Organization`,
    name: siteName,
    url: `${origin}/`,
    logo: `${origin}/icon-512.png`,
  }
  const crumbs = (items: { name: string; path: string }[]) => breadcrumb(origin, items)

  const index: PageSeo = {
    path: BLOG_PATH,
    htmlFile: `blog/index.html`,
    sources: [`src/BlogIndexPage.tsx`, `src/blog`],
    title: BLOG_INDEX_TITLE,
    description: BLOG_INDEX_DESCRIPTION,
    ogImage: `/og/og-blog.png`,
    jsonLd: [
      {
        "@context": `https://schema.org`,
        "@type": `Blog`,
        name: `${siteName} blog`,
        url: `${origin}${BLOG_PATH}`,
        publisher: organization,
        blogPost: BLOG_POSTS.map(({ meta }) => ({
          "@type": `BlogPosting`,
          headline: meta.title,
          url: `${origin}${postPath(meta.slug)}`,
          datePublished: meta.date,
        })),
      },
      crumbs([
        { name: `Home`, path: `/` },
        { name: `Blog`, path: BLOG_PATH },
      ]),
    ],
  }

  const posts: PageSeo[] = BLOG_POSTS.map(({ meta }) => {
    const path = postPath(meta.slug)
    const url = `${origin}${path}`
    return {
      path,
      htmlFile: `blog/${meta.slug}/index.html`,
      sources: meta.sources,
      title: postTitle(meta.title),
      description: meta.description,
      ogImage: meta.ogImage,
      jsonLd: [
        {
          "@context": `https://schema.org`,
          "@type": `BlogPosting`,
          headline: meta.title,
          description: meta.description,
          image: [`${origin}${meta.ogImage}`, `${origin}${meta.cover.src}`],
          datePublished: meta.date,
          dateModified: meta.updated ?? meta.date,
          author: { "@type": `Person`, name: meta.author.name, ...(meta.author.url ? { url: meta.author.url } : {}) },
          publisher: organization,
          mainEntityOfPage: url,
          url,
        },
        crumbs([
          { name: `Home`, path: `/` },
          { name: `Blog`, path: BLOG_PATH },
          { name: meta.title, path },
        ]),
      ],
    }
  })

  return [index, ...posts]
}
