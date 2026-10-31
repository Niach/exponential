/* The blog registry, newest first. Adding a post = one TSX module in this
   directory (exporting `meta` + `Post`), one line here, one HTML shell under
   blog/<slug>/ and `bun run scripts/generate-og.tsx` for its card. The
   prerender refuses a post whose shell, title or reading time disagrees. */
import type { ComponentType } from "react"
import type { PostMeta } from "./types"
import * as uiAsData from "./ui-as-data-down-to-the-compositor"
import * as sessionUi from "./session-as-interface"
import * as agentSeats from "./agents-get-their-own-seat"

export type BlogPost = { meta: PostMeta; Post: ComponentType }

const POSTS: BlogPost[] = [agentSeats, uiAsData, sessionUi].map((m) => ({ meta: m.meta, Post: m.Post }))

export const BLOG_POSTS: BlogPost[] = [...POSTS].sort((a, b) => (a.meta.date < b.meta.date ? 1 : a.meta.date > b.meta.date ? -1 : 0))

export function postBySlug(slug: string): BlogPost | undefined {
  return BLOG_POSTS.find((p) => p.meta.slug === slug)
}
