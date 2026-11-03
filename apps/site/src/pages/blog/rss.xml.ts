/* /blog/rss.xml: RSS 2.0 with the atom self link, from the blog registry. */
import { POSTS, postPath } from "../../components/landing/blog.ts"
import { SITE_ORIGIN } from "../../components/landing/seo.ts"

const escapeXml = (value: string) => value.replace(/&/g, `&amp;`).replace(/</g, `&lt;`).replace(/>/g, `&gt;`).replace(/"/g, `&quot;`)
/** RFC 822 at noon UTC: a post carries a day. */
const rfc822 = (iso: string) => new Date(`${iso}T12:00:00Z`).toUTCString()

export function GET() {
  const items = POSTS.map((post) => {
    const url = `${SITE_ORIGIN}${postPath(post.slug)}`
    return `    <item>
      <title>${escapeXml(post.title)}</title>
      <link>${url}</link>
      <guid isPermaLink="true">${url}</guid>
      <pubDate>${rfc822(post.date)}</pubDate>
      <author>${escapeXml(post.author.name)}</author>
      <description>${escapeXml(post.description)}</description>
      <enclosure url="${SITE_ORIGIN}${post.cover.src}" type="${post.cover.src.endsWith(`.png`) ? `image/png` : `image/webp`}" length="0" />
    </item>`
  })
  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>Exponential blog</title>
    <link>${SITE_ORIGIN}/blog/</link>
    <atom:link href="${SITE_ORIGIN}/blog/rss.xml" rel="self" type="application/rss+xml" />
    <description>What we build and why: humans and agents in one thread, and the generative UI the apps are made of.</description>
    <language>en</language>
    <lastBuildDate>${rfc822(POSTS[0]!.date)}</lastBuildDate>
${items.join(`\n`)}
  </channel>
</rss>
`
  return new Response(xml, { headers: { "Content-Type": `application/rss+xml; charset=utf-8` } })
}
