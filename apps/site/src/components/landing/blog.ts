/* The blog registry, newest first: the index, the posts' head, the RSS
   feed. Adding a post = one page in src/pages/blog/ and one entry here. */
export interface Post {
  slug: string
  title: string
  /** The meta description: OG/Twitter card, RSS. */
  description: string
  /** ISO date of publication. */
  date: string
  updated?: string
  author: { name: string; url?: string }
  cover: { src: string; alt: string; width: number; height: number }
  ogImage: string
}

const DANNY = { name: `Danny`, url: `https://github.com/Niach` }

export const POSTS: Post[] = [
  {
    slug: `exponentialos`,
    title: `ExponentialOS: a computer built for agents`,
    description: `A Linux system where agents are part of the computer: their own seats in the compositor, a desktop drawn as data, every service a tool, and a handful of libraries that fit together through MCP. One architecture, part by part.`,
    date: `2026-10-31`,
    author: DANNY,
    cover: { src: `/blog/os-stack.webp`, alt: `ExponentialOS as layers, from Linux and the compositor up through services, the glue and the SDK modules to apps and you`, width: 1392, height: 768 },
    ogImage: `/og/og-blog-exponentialos.png`,
  },
  {
    slug: `ui-as-data-down-to-the-compositor`,
    title: `UI as data, down to the compositor`,
    description: `A screen described as data and painted natively is one idea with four layers: the card an agent answers with, a desktop whose panels are that data on disk, a compositor whose tree an agent can address, and a view for watching it work. What exists of each, and what is a drawing.`,
    date: `2026-10-24`,
    author: DANNY,
    cover: { src: `/blog/watch-view.webp`, alt: `Draft of the watch view: the live Firefox window with the agent's cursor on Export CSV, a line of seven steps and the agent's spoken caption`, width: 1392, height: 784 },
    ogImage: `/og/og-blog-ui-as-data-down-to-the-compositor.png`,
  },
  {
    slug: `session-as-interface`,
    title: `The session as the interface`,
    description: `The tracker's MCP server runs inside the coding agent's session, and the session view knows the tools: a call is drawn by its kind, an issue reference is a chip, a surface is painted natively, and sessions message each other.`,
    date: `2026-10-23`,
    author: DANNY,
    cover: { src: `/shots/script-card/web.webp`, alt: `A session feed: a code-mode script card, a comment written through MCP, the queue`, width: 1800, height: 1200 },
    ogImage: `/og/og-blog-session-as-interface.png`,
  },
]

export const BLOG_TITLE = `Blog · Exponential`
export const BLOG_DESCRIPTION = `Notes on how Exponential works: the coding agent's session as the interface, and the generative UI the apps are made of. Each claim has a screenshot.`

export const postPath = (slug: string) => `/blog/${slug}/`
export const postBySlug = (slug: string) => POSTS.find((p) => p.slug === slug)!

const MONTHS = [`January`, `February`, `March`, `April`, `May`, `June`, `July`, `August`, `September`, `October`, `November`, `December`]
/** `2026-10-23` → `October 23, 2026`. */
export function formatDate(iso: string): string {
  const [y, m, d] = iso.split(`-`).map(Number)
  return `${MONTHS[m! - 1]} ${d}, ${y}`
}

