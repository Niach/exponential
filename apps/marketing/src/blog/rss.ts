/* /blog/rss.xml (RSS 2.0 + the atom self link), written by scripts/prerender.tsx
   from the same registry the pages come from. Pure: no DOM, no React. */
import type { PostMeta } from "./types"
import { BLOG_PATH, postPath } from "./types"

function escapeXml(value: string): string {
  return value
    .replace(/&/g, `&amp;`)
    .replace(/</g, `&lt;`)
    .replace(/>/g, `&gt;`)
    .replace(/"/g, `&quot;`)
}

/** RFC 822 at noon UTC: the post carries a day, not a time. */
function rfc822(iso: string): string {
  return new Date(`${iso}T12:00:00Z`).toUTCString()
}

export function rssXml({ origin, siteName, posts }: { origin: string; siteName: string; posts: PostMeta[] }): string {
  const sorted = [...posts].sort((a, b) => (a.date < b.date ? 1 : -1))
  const items = sorted.map((post) => {
    const url = `${origin}${postPath(post.slug)}`
    return `    <item>
      <title>${escapeXml(post.title)}</title>
      <link>${url}</link>
      <guid isPermaLink="true">${url}</guid>
      <pubDate>${rfc822(post.date)}</pubDate>
      <author>${escapeXml(post.author.name)}</author>
      <description>${escapeXml(post.description)}</description>
      <enclosure url="${origin}${post.cover.src}" type="${post.cover.src.endsWith(`.png`) ? `image/png` : `image/webp`}" length="0" />
    </item>`
  })
  const newest = sorted[0] ? rfc822(sorted[0].date) : new Date().toUTCString()
  return `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>${escapeXml(siteName)} blog</title>
    <link>${origin}${BLOG_PATH}</link>
    <atom:link href="${origin}${BLOG_PATH}rss.xml" rel="self" type="application/rss+xml" />
    <description>What we build and why: humans and agents in one thread, and the generative UI the apps are made of.</description>
    <language>en</language>
    <lastBuildDate>${newest}</lastBuildDate>
${items.join(`\n`)}
  </channel>
</rss>
`
}
