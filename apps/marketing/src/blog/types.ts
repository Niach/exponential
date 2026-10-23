/* The blog's post model (EXP-1366). A post is a TSX module exporting its
   `meta` (this shape) and its `Post` body component; src/blog/posts.ts is the
   registry every consumer reads: the index page, the post pages, the SEO
   manifest (one PAGES entry per post), the RSS feed, the OG card generator
   and the home strip. MDX was considered and passed over: the prerender runs
   the page components under Bun and the client hydrates the same tree, and
   a TSX post needs no loader on either side. */

export type BlogAuthor = { name: string; url?: string }

export type BlogCover = {
  /* Site-root-relative image path (a store shot or public/blog/…). */
  src: string
  alt: string
  width: number
  height: number
}

export type PostMeta = {
  /* URL slug: /blog/<slug>/ */
  slug: string
  title: string
  /* The meta description: the OG/Twitter card and the index card lead. */
  description: string
  /* ISO date (YYYY-MM-DD), the publication day. */
  date: string
  /* ISO date, set when the body changed after publication. */
  updated?: string
  author: BlogAuthor
  cover: BlogCover
  /* Minutes at READING_WPM words per minute; scripts/prerender.tsx counts
     the rendered body and refuses a value that is off by more than one. */
  readingMinutes: number
  /* Absolute-from-root OG image path (scripts/generate-og.tsx writes it). */
  ogImage: string
  /* The post's source files, for the sitemap lastmod (git log). */
  sources: string[]
}

export const READING_WPM = 230

export const BLOG_PATH = `/blog/`
export const postPath = (slug: string) => `${BLOG_PATH}${slug}/`

const MONTHS = [`January`, `February`, `March`, `April`, `May`, `June`, `July`, `August`, `September`, `October`, `November`, `December`]

/** `2026-10-23` → `October 23, 2026` (locale-free: the prerender and the
 *  client must print the same string). */
export function formatDate(iso: string): string {
  const [y, m, d] = iso.split(`-`).map(Number)
  return `${MONTHS[m - 1]} ${d}, ${y}`
}

/** Word count → minutes, never below one. */
export function readingMinutesOf(words: number): number {
  return Math.max(1, Math.round(words / READING_WPM))
}
